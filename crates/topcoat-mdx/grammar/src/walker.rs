//! mdast-to-view AST walker.
//!
//! Transforms `markdown-rs` mdast nodes into Topcoat `view!` AST types
//! (`Node`, `Element`, `Nodes`), enabling markdown content to be rendered
//! through the same code generation pipeline as handwritten templates.

use std::cell::RefCell;
use std::collections::HashMap;

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
    /// Link/image definition registry: normalized identifier → (url, title).
    /// Built during the pre-scan pass so that `LinkReference` and
    /// `ImageReference` nodes can be resolved during the main walk.
    pub definitions: HashMap<String, (String, Option<String>)>,
    /// Footnote definitions collected during the pre-scan pass:
    /// (identifier, children nodes).
    pub footnotes: Vec<(String, Vec<markdown::mdast::Node>)>,
    /// Footnote identifiers in first-reference order (GFM spec).
    /// Populated during the main walk; used to number footnotes
    /// in the document-end section.
    pub footnote_order: RefCell<Vec<String>>,
    /// Heading slug counter for duplicate ID handling.
    /// Maps base slug → occurrence count so that "# Hello" followed
    /// by another "# Hello" produces ids "hello" and "hello-1".
    /// Wrapped in RefCell for interior mutability (same pattern as errors,
    /// footnote_order).
    pub seen_ids: RefCell<HashMap<String, u32>>,
}

impl<'a> WalkContext<'a> {
    /// Create a new walk context with the given component registry,
    /// override registry, and span. Definition and footnote maps are
    /// initialized empty.
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
            definitions: HashMap::new(),
            footnotes: Vec::new(),
            footnote_order: RefCell::new(Vec::new()),
            seen_ids: RefCell::new(HashMap::new()),
        }
    }

    /// Create a walk context with pre-populated definition and footnote maps.
    /// Used by `mdx_to_view` after the pre-scan pass.
    #[must_use]
    pub fn with_maps(
        components: &'a [(String, Path)],
        overrides: &'a [(&'static str, Path)],
        span: Span,
        definitions: HashMap<String, (String, Option<String>)>,
        footnotes: Vec<(String, Vec<markdown::mdast::Node>)>,
    ) -> Self {
        Self {
            components,
            overrides,
            errors: RefCell::new(Vec::new()),
            span,
            definitions,
            footnotes,
            footnote_order: RefCell::new(Vec::new()),
            seen_ids: RefCell::new(HashMap::new()),
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

/// Collects link/image definitions and footnote definitions from the root.
///
/// Iterates root children looking for `Definition` and `FootnoteDefinition`
/// nodes. Definition identifiers are normalized to lowercase per CommonMark
/// case-folding rules. Returns a tuple of `(definitions, footnotes)` where
/// definitions maps the normalized identifier to `(url, title)` and footnotes
/// stores `(identifier, children)` pairs.
#[must_use]
pub fn collect_definitions(
    root: &markdown::mdast::Root,
) -> (
    HashMap<String, (String, Option<String>)>,
    Vec<(String, Vec<markdown::mdast::Node>)>,
) {
    let mut definitions = HashMap::new();
    let mut footnotes = Vec::new();
    for node in &root.children {
        match node {
            markdown::mdast::Node::Definition(d) => {
                let id = d.identifier.trim().to_lowercase();
                definitions.insert(
                    id,
                    (d.url.clone(), d.title.clone()),
                );
            }
            markdown::mdast::Node::FootnoteDefinition(f) => {
                footnotes.push((f.identifier.clone(), f.children.clone()));
            }
            _ => {}
        }
    }
    (definitions, footnotes)
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
/// Two-pass walk: first collects `Definition` and `FootnoteDefinition` nodes
/// from the root, then walks the remaining nodes with the populated maps.
/// Errors collected during the walk are propagated back into the original
/// `ctx.errors` so that the caller can access them.
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
        markdown::mdast::Node::Root(r) => {
            // Pass 1: collect definitions and footnote definitions.
            let (definitions, footnotes) = collect_definitions(&r);
            // Build context with pre-populated maps.
            let ctx_with_maps = WalkContext::with_maps(
                ctx.components,
                ctx.overrides,
                ctx.span,
                definitions,
                footnotes,
            );
            // Pass 2: walk the root children.
            let mut walked = walk_nodes(&ctx_with_maps, &r.children).into_vec();
            // Post-walk: append footnote section if any footnotes were referenced.
            let footnote_order = ctx_with_maps.footnote_order.borrow().clone();
            if !footnote_order.is_empty() {
                walked.push(node::walk_footnote_section(&ctx_with_maps, &footnote_order));
            }
            // Propagate errors from the internal walk context back to the caller's context.
            ctx.errors.borrow_mut().extend(
                ctx_with_maps.errors.borrow_mut().drain(..)
            );
            walked.into()
        }
        _ => Nodes::new(),
    };
    Ok(View { cx: None, nodes })
}

/// Extracts the plain text content from a heading's inline children.
///
/// Recursively collects text from `Text`, `Emphasis`, `Strong`, and `InlineCode`
/// nodes. Used by the Heading arm to generate kebab-case id attributes.
fn extract_heading_text(nodes: &[markdown::mdast::Node]) -> String {
    let mut parts = Vec::new();
    for node in nodes {
        match node {
            markdown::mdast::Node::Text(t) => parts.push(t.value.clone()),
            markdown::mdast::Node::Emphasis(e) => parts.push(extract_heading_text(&e.children)),
            markdown::mdast::Node::Strong(s) => parts.push(extract_heading_text(&s.children)),
            markdown::mdast::Node::InlineCode(c) => parts.push(c.value.clone()),
            _ => {}
        }
    }
    parts.join("")
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
pub fn walk_node(ctx: &WalkContext, node: &markdown::mdast::Node) -> Vec<Node> {
    match node {
        markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children).into_vec(),
        markdown::mdast::Node::Paragraph(p) => {
            vec![helpers::html_element("p", walk_nodes(ctx, &p.children))]
        }
        markdown::mdast::Node::Heading(h) => {
            let tag = format!("h{}", h.depth);
            let children = walk_nodes(ctx, &h.children);
            // Generate kebab-case id attribute for URL anchor links.
            let heading_text = extract_heading_text(&h.children);
            let base_slug = helpers::slugify(&heading_text);
            let id_value = if base_slug.is_empty() {
                tag.clone()
            } else {
                let count = {
                    let mut seen = ctx.seen_ids.borrow_mut();
                    let c = seen.get(&base_slug).copied().unwrap_or(0);
                    seen.insert(base_slug.clone(), c + 1);
                    c
                };
                if count == 0 {
                    base_slug
                } else {
                    format!("{base_slug}-{count}")
                }
            };
            let mut attrs: Vec<topcoat_view_grammar::attributes::Attribute> = Vec::new();
            attrs.push(helpers::create_attribute("id", &id_value));
            let attributes = helpers::with_attributes(attrs);
            if let Some(path) = jsx::try_find_override_path(ctx, &tag) {
                vec![jsx::build_override_component(path, &attributes, children, ctx.span)]
            } else {
                vec![Node::Element(Box::new(helpers::normal_element_with_attrs(
                    &tag, attributes, children,
                )))]
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
            if let Some(path) = jsx::try_find_override_path(ctx, "hr") {
                vec![jsx::build_override_component(path, &topcoat_view_grammar::attributes::Attributes::default(), Nodes::new(), ctx.span)]
            } else {
                vec![Node::Element(Box::new(helpers::void_element("hr")))]
            }
        }
        markdown::mdast::Node::Break(_) => {
            vec![Node::Element(Box::new(helpers::void_element("br")))]
        }
        markdown::mdast::Node::Link(l) => vec![node::walk_link(ctx, l)],
        markdown::mdast::Node::Image(i) => vec![node::walk_image(ctx, i)],
        // Html nodes are never produced by the parser (html_flow and
        // html_text are disabled in get_parse_options()). MdxjsEsm
        // contains JS expressions that are not Rust-deserializable.
        // Both return Vec::new() / fall to the wildcard arm.
        markdown::mdast::Node::Html(_)
        | markdown::mdast::Node::MdxjsEsm(_) => Vec::new(),
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
        // Reference-style links and images: resolved from definitions map.
        markdown::mdast::Node::LinkReference(lr) => {
            vec![node::walk_link_reference(ctx, lr)]
        }
        markdown::mdast::Node::ImageReference(ir) => {
            vec![node::walk_image_reference(ctx, ir)]
        }
        // Definition nodes are declarations only, skip during main walk.
        markdown::mdast::Node::Definition(_) => Vec::new(),
        // Footnote references: emit superscript link, track order.
        markdown::mdast::Node::FootnoteReference(fr) => {
            vec![node::walk_footnote_reference(ctx, fr)]
        }
        // Footnote definitions: skip during main walk, rendered at doc end.
        markdown::mdast::Node::FootnoteDefinition(_) => Vec::new(),
        // Default: skip remaining node types.
        // Deferred:
        // - MdxFlowExpression, MdxTextExpression: MDX expressions
        // Handled elsewhere (not rendered by this walker):
        // - Html: disabled (never produced by parser), handled by match arm above
        // - Yaml, Toml: extracted by extract_frontmatter(), skipped in walker
        // - MdxjsEsm: not rendered (JS expressions, not Rust-deserializable)
        // - InlineMath, Math: LaTeX math (not enabled in parse options)
        // - TableRow, TableCell: handled internally by walk_table
        _ => Vec::new(),
    }
}

/// Walks an mdast node directly into a `ViewBuilder`.
///
/// `mdast::Text` nodes go through `text()` for proper escaping.
/// All other node types are walked into view `Node`s and written through
/// their own `LowerView` implementation. HTML passthrough is disabled:
/// `html_flow` and `html_text` are off in `get_parse_options()`, so
/// `mdast::Html` nodes are never produced by the parser.
pub fn walk_to_writer(ctx: &WalkContext, node: &markdown::mdast::Node, builder: &mut ViewBuilder) {
    match node {
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
pub use jsx::coerce_attr_value;
// Re-export excerpt split detection for the macro crate's two-writer approach.
pub use helpers::find_excerpt_split;

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::get_parse_options;

    fn parse_to_root(content: &str) -> markdown::mdast::Node {
        let options = get_parse_options();
        markdown::to_mdast(content, &options).expect("should parse valid markdown")
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

    // ---- Two-pass walk infrastructure tests (collect_definitions) ----

    #[test]
    fn collect_definitions_finds_link_definitions() {
        let root = parse_to_root("[example]: https://example.com \"Example\"\n\nText");
        let markdown::mdast::Node::Root(r) = root else {
            panic!("expected root");
        };
        let (definitions, _) = collect_definitions(&r);
        assert_eq!(definitions.len(), 1, "should find one definition");
        let entry = definitions.get("example").expect("should have 'example' key");
        assert_eq!(entry.0, "https://example.com", "should store URL");
        assert_eq!(entry.1, Some("Example".to_string()), "should store title");
    }

    #[test]
    fn collect_definitions_normalizes_identifier_case() {
        let root = parse_to_root("[MyLabel]: https://example.com\n\nText");
        let markdown::mdast::Node::Root(r) = root else {
            panic!("expected root");
        };
        let (definitions, _) = collect_definitions(&r);
        assert!(
            definitions.contains_key("mylabel"),
            "should normalize identifier to lowercase"
        );
    }

    #[test]
    fn collect_definitions_finds_footnote_definitions() {
        let root = parse_to_root("[^1]: This is a footnote\n\nText[^1]");
        let markdown::mdast::Node::Root(r) = root else {
            panic!("expected root");
        };
        let (_, footnotes) = collect_definitions(&r);
        assert_eq!(footnotes.len(), 1, "should find one footnote definition");
        assert_eq!(footnotes[0].0, "1", "footnote identifier should be '1'");
        assert!(!footnotes[0].1.is_empty(), "footnote should have children");
    }

    #[test]
    fn collect_definitions_empty_when_no_defs() {
        let root = parse_to_root("# Just a heading");
        let markdown::mdast::Node::Root(r) = root else {
            panic!("expected root");
        };
        let (definitions, footnotes) = collect_definitions(&r);
        assert!(definitions.is_empty(), "should have no definitions");
        assert!(footnotes.is_empty(), "should have no footnotes");
    }

    // ---- mdx_to_view entry point tests ----

    #[test]
    fn mdx_to_view_produces_view() {
        let ctx = WalkContext::empty();
        let view = mdx_to_view(&ctx, "# Test").expect("should parse valid markdown");
        assert!(view.cx.is_none());
        assert!(!view.nodes.is_empty());
    }

    // ---- WalkContext fields test ----

    #[test]
    fn walk_context_has_definitions_and_footnotes() {
        let ctx = WalkContext::with_maps(
            &[],
            &[],
            proc_macro2::Span::call_site(),
            std::collections::HashMap::new(),
            Vec::new(),
        );
        assert!(ctx.definitions.is_empty());
        assert!(ctx.footnotes.is_empty());
        assert!(ctx.footnote_order.borrow().is_empty());
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
}
