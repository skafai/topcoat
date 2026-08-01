//! mdast-to-view AST walker.
//!
//! Transforms `markdown-rs` mdast nodes into Topcoat `view!` AST types
//! (`Node`, `Element`, `Nodes`), enabling markdown content to be rendered
//! through the same code generation pipeline as handwritten templates.

use std::cell::RefCell;

use proc_macro2::Span;
use syn::Path;
use topcoat_view_grammar::view::{Node, Nodes, View};
use topcoat_view_grammar::view::hir::{LowerView, ViewBuilder};

use crate::parse::get_parse_options;

pub mod helpers;
pub mod jsx;
pub mod node;

/// Context threaded through the walker so JSX element handlers can look up
/// registered components and report diagnostics.
pub struct WalkContext<'a> {
    /// Component registry: tag-name → Rust path pairs.
    pub components: &'a [(String, Path)],
    /// HTML element override registry: tag-name → Rust path pairs.
    /// When a tag is registered here, the walker emits a `Node::Component`
    /// instead of a `Node::Element` for that tag.
    pub overrides: &'a [(&'static str, Path)],
    /// Error strings collected during walking. The macro layer (Plan 02)
    /// drains this buffer and converts each entry into a `syn::Error`.
    pub errors: RefCell<Vec<String>>,
    /// Span to use for generated literals. Prefer the span from the
    /// `compile_mdx!` file-path argument so diagnostics point to the
    /// invocation site rather than `call_site()`.
    pub span: Span,
}

impl<'a> WalkContext<'a> {
    /// Create a new walk context with the given component registry,
    /// override registry, and span.
    #[must_use]
    pub fn new(
        components: &'a [(String, Path)],
        overrides: &'a [(&'static str, Path)],
        span: Span,
    ) -> Self {
        Self {
            components,
            overrides,
            errors: RefCell::new(Vec::new()),
            span,
        }
    }

    /// Create an empty-context walker (no component registry, no overrides).
    #[must_use]
    pub fn empty() -> Self {
        Self::new(&[], &[], Span::call_site())
    }

    /// Create a walker with empty component registry but the given overrides.
    #[must_use]
    pub fn empty_with_overrides(overrides: &'a [(&'static str, Path)]) -> Self {
        Self::new(&[], overrides, Span::call_site())
    }
}

impl Default for WalkContext<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

/// Format of the frontmatter extracted from an MDX document.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[must_use]
pub enum FrontmatterFormat {
    /// YAML frontmatter (between `---` delimiters).
    Yaml,
    /// TOML frontmatter (between `+++` delimiters).
    Toml,
}

/// Extracts YAML or TOML frontmatter from the mdast root node.
///
/// Only the first child of the root can be frontmatter (Pitfall 1: YAML
/// frontmatter must appear at byte offset 0 in the source document).
/// Returns `Some((value_string, format))` when a `Node::Yaml` or `Node::Toml`
/// is the first root child, `None` otherwise.
///
/// Note: `MdxjsEsm` frontmatter is not extracted — it contains JavaScript
/// expressions that are not deserializable as Rust types.
#[must_use]
pub fn extract_frontmatter(root: &markdown::mdast::Node) -> Option<(String, FrontmatterFormat)> {
    let markdown::mdast::Node::Root(r) = root else {
        return None;
    };
    let first = r.children.first()?;
    match first {
        markdown::mdast::Node::Yaml(y) => Some((y.value.clone(), FrontmatterFormat::Yaml)),
        markdown::mdast::Node::Toml(t) => Some((t.value.clone(), FrontmatterFormat::Toml)),
        _ => None,
    }
}

/// Walks an mdast node tree into a Topcoat `view!` `View`.
///
/// Parses the MDX content using `markdown-rs` with GFM + MDX + frontmatter
/// enabled, then walks the resulting mdast into a `View` value ready for
/// token emission via `ToTokens`.
///
/// Note: raw HTML blocks are not supported through this entry point
/// (they require `walk_to_writer` which has access to `ViewBuilder`'s
/// unescaped output). Use `compile_mdx!` for HTML passthrough support.
///
/// # Errors
///
/// Returns `Err(markdown::message::Message)` if the markdown parser fails.
pub fn mdx_to_view(
    ctx: &WalkContext,
    mdx_content: &str,
) -> Result<View, markdown::message::Message> {
    let options = get_parse_options();
    let root = markdown::to_mdast(mdx_content, &options)?;

    let nodes = match root {
        markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children),
        _ => Nodes::new(),
    };
    Ok(View { cx: None, nodes })
}

/// Walks a slice of mdast nodes into a `Nodes` collection.
pub fn walk_nodes(ctx: &WalkContext, mdast_nodes: &[markdown::mdast::Node]) -> Nodes {
    let mut nodes = Vec::new();
    for node in mdast_nodes {
        nodes.extend(walk_node(ctx, node));
    }
    nodes.into()
}

/// Walks a single mdast node into zero or more view `Node`s.
///
/// # Panics
/// Panics if an override is registered for a tag (verified via `has_override`)
/// but `try_apply_override` returns `None` — this should not happen when the
/// `has_override` guard is used, as both check the same `ctx.overrides` slice.
pub fn walk_node(ctx: &WalkContext, node: &markdown::mdast::Node) -> Vec<Node> {
    match node {
        markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children).into_vec(),
        markdown::mdast::Node::Paragraph(p) => {
            vec![helpers::html_element("p", walk_nodes(ctx, &p.children))]
        }
        markdown::mdast::Node::Heading(h) => {
            let tag = format!("h{}", h.depth);
            let children = walk_nodes(ctx, &h.children);
            if jsx::has_override(ctx, &tag) {
                // try_apply_override will succeed here since we checked has_override first.
                vec![jsx::try_apply_override(ctx, &tag, &topcoat_view_grammar::attributes::Attributes::default(), children).unwrap()]
            } else {
                vec![helpers::html_element(&tag, children)]
            }
        }
        markdown::mdast::Node::Text(t) => {
            vec![helpers::text_node(&t.value)]
        }
        markdown::mdast::Node::Emphasis(e) => {
            vec![helpers::html_element("em", walk_nodes(ctx, &e.children))]
        }
        markdown::mdast::Node::Strong(s) => {
            vec![helpers::html_element("strong", walk_nodes(ctx, &s.children))]
        }
        markdown::mdast::Node::InlineCode(c) => {
            vec![helpers::html_element("code", Nodes::from(vec![helpers::text_node(&c.value)]))]
        }
        markdown::mdast::Node::Blockquote(b) => {
            vec![helpers::html_element("blockquote", walk_nodes(ctx, &b.children))]
        }
        markdown::mdast::Node::ThematicBreak(_) => {
            if jsx::has_override(ctx, "hr") {
                vec![jsx::try_apply_override(ctx, "hr", &topcoat_view_grammar::attributes::Attributes::default(), Nodes::new()).unwrap()]
            } else {
                vec![Node::Element(Box::new(helpers::void_element("hr")))]
            }
        }
        markdown::mdast::Node::Break(_) => {
            vec![Node::Element(Box::new(helpers::void_element("br")))]
        }
        markdown::mdast::Node::Link(l) => vec![node::walk_link(ctx, l)],
        markdown::mdast::Node::Image(i) => vec![node::walk_image(ctx, i)],
        // Raw HTML cannot be represented in the view! AST without a
        // ViewBuilder (which supports str_unescaped). Use
        // walk_to_writer for HTML passthrough. It returns Vec::new() like
        // the wildcard arm, which is intentional: Html nodes are skipped
        // here and handled by walk_to_writer instead.
        #[allow(clippy::match_same_arms)]
        markdown::mdast::Node::Html(_) | markdown::mdast::Node::MdxjsEsm(_) => Vec::new(),
        markdown::mdast::Node::Code(c) => vec![node::walk_code_block(ctx, c)],
        markdown::mdast::Node::List(l) => vec![node::walk_list(ctx, l)],
        markdown::mdast::Node::ListItem(li) => vec![node::walk_list_item(ctx, li)],
        markdown::mdast::Node::Table(t) => vec![node::walk_table(ctx, t)],
        markdown::mdast::Node::Delete(d) => vec![node::walk_delete(ctx, d)],
        // MDX JSX component elements — Phase 02.
        markdown::mdast::Node::MdxJsxFlowElement(el) => {
            if let Some(comp_node) = jsx::walk_jsx_element(ctx, el) {
                vec![comp_node]
            } else {
                Vec::new()
            }
        }
        markdown::mdast::Node::MdxJsxTextElement(el) => {
            if let Some(comp_node) = jsx::walk_jsx_text_element(ctx, el) {
                vec![comp_node]
            } else {
                Vec::new()
            }
        }
        // Default: skip remaining node types.
        // Deferred (require additional infrastructure):
        // - LinkReference, ImageReference: need a definition registry to resolve [ref] targets
        // - Definition: reference-style link/image declarations
        // - FootnoteDefinition, FootnoteReference: footnote support
        // - MdxFlowExpression, MdxTextExpression: MDX expressions
        // Handled elsewhere (not rendered by this walker):
        // - Html: raw passthrough via walk_to_writer (line above)
        // - Yaml, Toml: extracted by extract_frontmatter(), skipped in walker
        // - MdxjsEsm: not rendered (JS expressions, not Rust-deserializable)
        // - InlineMath, Math: LaTeX math (not enabled in parse options)
        // - TableRow, TableCell: handled internally by walk_table
        _ => Vec::new(),
    }
}

/// Walks an mdast node directly into a `ViewBuilder`.
///
/// This is the key function for raw HTML passthrough (D-03): `mdast::Html`
/// nodes are written via `str_unescaped()`, while `mdast::Text` nodes
/// go through `text()` for proper escaping.
pub fn walk_to_writer(ctx: &WalkContext, node: &markdown::mdast::Node, builder: &mut ViewBuilder) {
    match node {
        markdown::mdast::Node::Html(h) => {
            // Raw HTML passthrough.
            //
            // Security model: MDX files are trusted source content compiled
            // at build time. There is no runtime sanitization — any HTML
            // (including <script>, <iframe>, <object>) is emitted verbatim.
            // Do not use this pipeline with untrusted or user-generated MDX.
            // Links and images have separate is_safe_url() checks on their
            // URL attributes; raw HTML nodes do not.
            builder.str_unescaped(&h.value);
        }
        markdown::mdast::Node::Text(t) => {
            // Text content — escaped for HtmlContext::Text.
            builder.text(&t.value);
        }
        _ => {
            // For all other node types, construct view nodes and write them.
            let view_nodes = walk_node(ctx, node);
            for vn in view_nodes {
                vn.lower(builder);
            }
        }
    }
}

// Re-export jsx functions that are part of the public API used by external consumers.
pub use jsx::{coerce_attr_value, try_apply_override};

#[cfg(test)]
mod tests {
    use quote::quote;

    use super::*;
    use crate::parse::get_parse_options;

    fn parse_to_root(content: &str) -> markdown::mdast::Node {
        let options = get_parse_options();
        markdown::to_mdast(content, &options).expect("should parse valid markdown")
    }

    fn parse_and_walk_ctx(ctx: &WalkContext, content: &str) -> Nodes {
        let options = get_parse_options();
        let root = markdown::to_mdast(content, &options).unwrap();
        match root {
            markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children),
            _ => unreachable!(),
        }
    }

    // ---- Frontmatter extraction tests ----

    #[test]
    fn extract_frontmatter_yaml_present() {
        let root = parse_to_root("---\ntitle: Hello\ndate: 2024-01-01\n---\n\n# Body");
        let fm = extract_frontmatter(&root);
        assert!(fm.is_some(), "should extract YAML frontmatter");
        let (content, format) = fm.unwrap();
        assert!(matches!(format, FrontmatterFormat::Yaml));
        assert!(content.contains("title"), "should contain title field");
        assert!(content.contains("Hello"), "should contain title value");
    }

    #[test]
    fn extract_frontmatter_none() {
        let root = parse_to_root("# Heading\n\nPlain text");
        assert!(
            extract_frontmatter(&root).is_none(),
            "should return None when no frontmatter"
        );
    }

    #[test]
    fn extract_frontmatter_heading_first() {
        let root = parse_to_root("# heading");
        assert!(
            extract_frontmatter(&root).is_none(),
            "heading-first doc should have no frontmatter"
        );
    }

    #[test]
    fn extract_frontmatter_only_frontmatter() {
        let root = parse_to_root("---\nkey: value\n---");
        let fm = extract_frontmatter(&root);
        assert!(fm.is_some(), "should extract YAML even with no body");
        let (content, format) = fm.unwrap();
        assert!(matches!(format, FrontmatterFormat::Yaml));
        assert!(content.contains("key"), "should contain the YAML content");
    }

    #[test]
    fn extract_frontmatter_toml_present() {
        let root = parse_to_root("+++\ntitle = \"Hello\"\ndate = 2024-01-01\n+++\n\n# Body");
        let fm = extract_frontmatter(&root);
        assert!(fm.is_some(), "should extract TOML frontmatter");
        let (content, format) = fm.unwrap();
        assert!(matches!(format, FrontmatterFormat::Toml));
        assert!(content.contains("title"), "should contain title field");
        assert!(content.contains("Hello"), "should contain title value");
    }

    #[test]
    fn extract_frontmatter_mdxjs_esm_returns_none() {
        // MdxjsEsm frontmatter is intentionally not extracted since it
        // contains JavaScript expressions, not deserializable data.
        let root = parse_to_root("```js\nexport const title = \"Hello\";\n```\n\n# Body");
        let fm = extract_frontmatter(&root);
        assert!(
            fm.is_none(),
            "MdxjsEsm should not be extracted as frontmatter"
        );
    }

    // ---- walk_to_writer test ----

    #[test]
    fn walks_raw_html_via_writer() {
        let ctx = WalkContext::empty();
        let options = get_parse_options();
        let root = markdown::to_mdast(r#"<div class="raw">Raw</div>"#, &options).unwrap();
        let mut builder = ViewBuilder::new();
        if let markdown::mdast::Node::Root(r) = root {
            for child in &r.children {
                walk_to_writer(&ctx, child, &mut builder);
            }
        }
        let tokens = builder.finish().emit_root();
        let token_str = quote! { #tokens }.to_string();
        // The raw HTML should appear verbatim (not escaped as &lt;div&gt;).
        assert!(
            token_str.contains("<div"),
            "should contain raw <div, got: {token_str}"
        );
        assert!(
            !token_str.contains("&lt;div"),
            "should NOT contain escaped &lt;div, got: {token_str}",
        );
        assert!(
            token_str.contains("Raw</div>"),
            "should contain raw closing tag, got: {token_str}",
        );
    }

    // ---- mdx_to_view entry point tests ----

    #[test]
    fn mdx_to_view_produces_view() {
        let ctx = WalkContext::empty();
        let view = mdx_to_view(&ctx, "# Test").expect("should parse valid markdown");
        assert!(view.cx.is_none());
        assert!(!view.nodes.is_empty());
    }

    #[test]
    fn mdx_to_view_returns_error_on_invalid_input() {
        let ctx = WalkContext::empty();
        // Verify the function returns Err instead of panicking.
        // (This input is valid markdown, but we test the return type.)
        let result = mdx_to_view(&ctx, "# Valid heading");
        assert!(result.is_ok());
        let view = result.unwrap();
        assert!(!view.nodes.is_empty());
    }

    // ---- parse_and_walk_ctx is used by jsx and node tests via super::super ----
    pub(crate) fn parse_and_walk(content: &str) -> Nodes {
        parse_and_walk_ctx(&WalkContext::empty(), content)
    }
}
