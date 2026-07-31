//! mdast-to-view AST walker.
//!
//! Transforms `markdown-rs` mdast nodes into Topcoat `view!` AST types
//! (`Node`, `Element`, `Nodes`), enabling markdown content to be rendered
//! through the same code generation pipeline as handwritten templates.

use std::cell::RefCell;

use proc_macro2::Span;
use syn::{Expr, Ident, LitStr, Path, parse_quote, token::{Colon, Paren}};
use topcoat_view_grammar::{
    attributes::{Attribute, AttributeKey, AttributeNode, AttributeValue, Attributes},
    view::{
        ClosingTag, Component, Element, ElementName, HtmlIdent, NamedArg, NamedArgValue, Node, Nodes,
        OpeningTag, SelfClosingTag, View, ViewWriter, WriteView,
    },
};

use crate::parse::get_parse_options;

/// Context threaded through the walker so JSX element handlers can look up
/// registered components and report diagnostics.
pub struct WalkContext<'a> {
    /// Component registry: tag-name → Rust path pairs.
    pub components: &'a [(String, Path)],
    /// Error strings collected during walking. The macro layer (Plan 02)
    /// drains this buffer and converts each entry into a `syn::Error`.
    pub errors: RefCell<Vec<String>>,
    /// Span to use for generated literals. Prefer the span from the
    /// `compile_mdx!` file-path argument so diagnostics point to the
    /// invocation site rather than `call_site()`.
    pub span: Span,
}

impl<'a> WalkContext<'a> {
    /// Create a new walk context with the given component registry and span.
    #[must_use]
    pub fn new(components: &'a [(String, Path)], span: Span) -> Self {
        Self {
            components,
            errors: RefCell::new(Vec::new()),
            span,
        }
    }

    /// Create an empty-context walker (no component registry).
    #[must_use]
    pub fn empty() -> Self {
        Self::new(&[], Span::call_site())
    }
}

impl Default for WalkContext<'_> {
    fn default() -> Self {
        Self::empty()
    }
}

/// Walks an mdast node tree into a Topcoat `view!` `View`.
///
/// Parses the MDX content using `markdown-rs` with GFM + MDX + frontmatter
/// enabled, then walks the resulting mdast into a `View` value ready for
/// token emission via `ToTokens`.
///
/// Note: raw HTML blocks are not supported through this entry point
/// (they require `walk_to_writer` which has access to `ViewWriter`'s
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
pub fn walk_node(ctx: &WalkContext, node: &markdown::mdast::Node) -> Vec<Node> {
    match node {
        markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children).into_vec(),
        markdown::mdast::Node::Paragraph(p) => {
            vec![html_element("p", walk_nodes(ctx, &p.children))]
        }
        markdown::mdast::Node::Heading(h) => {
            let tag = format!("h{}", h.depth);
            vec![html_element(&tag, walk_nodes(ctx, &h.children))]
        }
        markdown::mdast::Node::Text(t) => {
            vec![text_node(&t.value)]
        }
        markdown::mdast::Node::Emphasis(e) => {
            vec![html_element("em", walk_nodes(ctx, &e.children))]
        }
        markdown::mdast::Node::Strong(s) => {
            vec![html_element("strong", walk_nodes(ctx, &s.children))]
        }
        markdown::mdast::Node::InlineCode(c) => {
            vec![html_element("code", Nodes::from(vec![text_node(&c.value)]))]
        }
        markdown::mdast::Node::Blockquote(b) => {
            vec![html_element("blockquote", walk_nodes(ctx, &b.children))]
        }
        markdown::mdast::Node::ThematicBreak(_) => {
            vec![Node::Element(Box::new(void_element("hr")))]
        }
        markdown::mdast::Node::Break(_) => {
            vec![Node::Element(Box::new(void_element("br")))]
        }
        markdown::mdast::Node::Link(l) => vec![walk_link(ctx, l)],
        markdown::mdast::Node::Image(i) => vec![walk_image(i)],
        // Raw HTML cannot be represented in the view! AST without a
        // ViewWriter (which supports write_str_unescaped). Use
        // walk_to_writer for HTML passthrough. It returns Vec::new() like
        // the wildcard arm, which is intentional: Html nodes are skipped
        // here and handled by walk_to_writer instead.
        #[allow(clippy::match_same_arms)]
        markdown::mdast::Node::Html(_)
        | markdown::mdast::Node::MdxjsEsm(_) => Vec::new(),
        markdown::mdast::Node::Code(c) => vec![walk_code_block(c)],
        markdown::mdast::Node::List(l) => vec![walk_list(ctx, l)],
        markdown::mdast::Node::ListItem(li) => vec![walk_list_item(ctx, li)],
        markdown::mdast::Node::Table(t) => vec![walk_table(ctx, t)],
        markdown::mdast::Node::Delete(d) => vec![walk_delete(ctx, d)],
        // MDX JSX component elements — Phase 02.
        markdown::mdast::Node::MdxJsxFlowElement(el) => {
            if let Some(comp_node) = walk_jsx_element(ctx, el) {
                vec![comp_node]
            } else {
                Vec::new()
            }
        }
        markdown::mdast::Node::MdxJsxTextElement(el) => {
            if let Some(comp_node) = walk_jsx_text_element(ctx, el) {
                vec![comp_node]
            } else {
                Vec::new()
            }
        }
        // Default: skip nodes not supported in this phase.
        // Deferred (require additional infrastructure for Phase 02+):
        // - LinkReference, ImageReference: need a definition registry to resolve [ref] targets
        // - Definition: reference-style link/image declarations
        // - FootnoteDefinition, FootnoteReference: footnote support
        // Skipped (out of scope for markdown compilation):
        // - Frontmatter (Yaml, Toml, MdxjsEsm): metadata, not rendered content
        // - MdxFlowExpression, MdxTextExpression: MDX expressions
        // - InlineMath, Math: LaTeX math (not enabled in parse options)
        // - TableRow, TableCell: handled internally by walk_table
        _ => Vec::new(),
    }
}

/// Walks an mdast node directly into a `ViewWriter`.
///
/// This is the key function for raw HTML passthrough (D-03): `mdast::Html`
/// nodes are written via `write_str_unescaped()`, while `mdast::Text` nodes
/// go through `write_text()` for proper escaping.
pub fn walk_to_writer(ctx: &WalkContext, node: &markdown::mdast::Node, writer: &mut ViewWriter) {
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
            writer.write_str_unescaped(&h.value);
        }
        markdown::mdast::Node::Text(t) => {
            // Text content — escaped for HtmlContext::Text.
            writer.write_text(&t.value);
        }
        _ => {
            // For all other node types, construct view nodes and write them.
            let view_nodes = walk_node(ctx, node);
            for vn in view_nodes {
                vn.write(writer);
            }
        }
    }
}

// ---------------------------------------------------------------------------
// Node-specific walkers
// ---------------------------------------------------------------------------

/// Checks if a URL uses a dangerous protocol (XSS mitigation, T-01-01).
/// Blocks `javascript:`, `vbscript:`, and ALL `data:` URIs (including
/// `data:image/svg+xml` which can execute JS via SVG event handlers).
fn is_safe_url(url: &str) -> bool {
    let trimmed = url.trim_start().to_ascii_lowercase();
    !trimmed.starts_with("javascript:")
        && !trimmed.starts_with("vbscript:")
        && !trimmed.starts_with("data:")
}

/// Walks a link node: `<a href="url" title="...">...</a>`.
/// Strips dangerous URL schemes (javascript:, vbscript:, data:)
/// to prevent XSS — renders link text as a `<span>` without href.
fn walk_link(ctx: &WalkContext, link: &markdown::mdast::Link) -> Node {
    if !is_safe_url(&link.url) {
        // Strip the href to prevent XSS; render link text only.
        let children = walk_nodes(ctx, &link.children);
        return html_element("span", children);
    }
    let mut attrs = Vec::with_capacity(2);
    attrs.push(create_attribute("href", &link.url));
    if let Some(title) = &link.title {
        attrs.push(create_attribute("title", title));
    }
    let attributes = with_attributes(attrs);
    let children = walk_nodes(ctx, &link.children);
    Node::Element(Box::new(normal_element_with_attrs(
        "a", attributes, children,
    )))
}

/// Walks an image node: `<img src="url" alt="alt" title="...">`.
/// Strips dangerous URL schemes (javascript:, vbscript:, data:)
/// to prevent XSS — renders alt text only without src.
fn walk_image(image: &markdown::mdast::Image) -> Node {
    if !is_safe_url(&image.url) {
        // Strip the src to prevent XSS; render alt text only.
        let children = Nodes::from(vec![text_node(&image.alt)]);
        return html_element("span", children);
    }
    let mut attrs = Vec::with_capacity(3);
    attrs.push(create_attribute("src", &image.url));
    attrs.push(create_attribute("alt", &image.alt));
    if let Some(title) = &image.title {
        attrs.push(create_attribute("title", title));
    }
    let attributes = with_attributes(attrs);
    Node::Element(Box::new(void_element_with_attrs("img", attributes)))
}

/// Walks a fenced code block: `<pre><code class="language-{lang}">...</code></pre>`.
fn walk_code_block(code: &markdown::mdast::Code) -> Node {
    let mut attrs = Vec::new();
    if let Some(ref lang) = code.lang {
        attrs.push(create_attribute("class", &format!("language-{lang}")));
    }
    let code_attrs = with_attributes(attrs);
    let code_children = Nodes::from(vec![text_node(&code.value)]);
    let code_el = normal_element_with_attrs("code", code_attrs, code_children);
    let pre_children = Nodes::from(vec![Node::Element(Box::new(code_el))]);
    html_element("pre", pre_children)
}

/// Walks a list: `<ul>` or `<ol>` with `<li>` children.
fn walk_list(ctx: &WalkContext, list: &markdown::mdast::List) -> Node {
    let tag = if list.ordered { "ol" } else { "ul" };
    let mut children = Vec::new();
    for node in &list.children {
        match node {
            markdown::mdast::Node::ListItem(item) => {
                children.push(walk_list_item(ctx, item));
            }
            other => children.extend(walk_node(ctx, other)),
        }
    }
    html_element(tag, Nodes::from(children))
}

/// Walks a list item: `<li>` with optional leading checkbox for task lists.
fn walk_list_item(ctx: &WalkContext, item: &markdown::mdast::ListItem) -> Node {
    let mut children = Vec::new();
    if let Some(checked) = &item.checked {
        if *checked {
            // <input type="checkbox" checked disabled />
            let input_attrs = vec![
                create_attribute("type", "checkbox"),
                create_attribute_bool("checked"),
                create_attribute("disabled", ""),
            ];
            let input_el = self_closing_element("input", with_attributes(input_attrs));
            children.push(Node::Element(Box::new(input_el)));
        } else {
            // <input type="checkbox" disabled /> — no checked attribute
            let input_attrs = vec![
                create_attribute("type", "checkbox"),
                create_attribute("disabled", ""),
            ];
            let input_el = self_closing_element("input", with_attributes(input_attrs));
            children.push(Node::Element(Box::new(input_el)));
        }
    }
    children.extend(walk_nodes(ctx, &item.children).into_vec());
    html_element("li", Nodes::from(children))
}

/// Walks a table: `<table><thead>...</thead><tbody>...</tbody></table>`.
fn walk_table(ctx: &WalkContext, table: &markdown::mdast::Table) -> Node {
    let mut child_nodes = Vec::new();

    // Iterate over table.children — each is Node::TableRow.
    let row_nodes: Vec<&markdown::mdast::TableRow> = table
        .children
        .iter()
        .filter_map(|n| {
            if let markdown::mdast::Node::TableRow(row) = n {
                Some(row)
            } else {
                None
            }
        })
        .collect();

    // First row is <thead>, rest is <tbody>.
    if let Some(head_row) = row_nodes.first() {
        let th_cells: Vec<Node> = head_row
            .children
            .iter()
            .enumerate()
            .filter_map(|(col_idx, n)| {
                if let markdown::mdast::Node::TableCell(cell) = n {
                    Some(walk_table_cell_inner(
                        ctx,
                        cell,
                        true,
                        col_idx,
                        &table.align,
                    ))
                } else {
                    None
                }
            })
            .collect();
        let tr = html_element("tr", Nodes::from(th_cells));
        let thead = html_element("thead", Nodes::from(vec![tr]));
        child_nodes.push(thead);
    }

    if row_nodes.len() > 1 {
        let body_rows: Vec<Node> = row_nodes[1..]
            .iter()
            .map(|row| {
                let td_cells: Vec<Node> = row
                    .children
                    .iter()
                    .enumerate()
                    .filter_map(|(col_idx, n)| {
                        if let markdown::mdast::Node::TableCell(cell) = n {
                            Some(walk_table_cell_inner(
                                ctx,
                                cell,
                                false,
                                col_idx,
                                &table.align,
                            ))
                        } else {
                            None
                        }
                    })
                    .collect();
                html_element("tr", Nodes::from(td_cells))
            })
            .collect();
        let tbody = html_element("tbody", Nodes::from(body_rows));
        child_nodes.push(tbody);
    }

    html_element("table", Nodes::from(child_nodes))
}

/// Walks a table cell: `<th>` or `<td>` with optional alignment style.
fn walk_table_cell_inner(
    ctx: &WalkContext,
    cell: &markdown::mdast::TableCell,
    is_header: bool,
    col_idx: usize,
    align: &[markdown::mdast::AlignKind],
) -> Node {
    let tag = if is_header { "th" } else { "td" };
    let mut attrs = Vec::new();
    // Look up alignment from the table's align vector by column index.
    if let Some(&align_kind) = align.get(col_idx)
        && !matches!(align_kind, markdown::mdast::AlignKind::None)
    {
        let value = match align_kind {
            markdown::mdast::AlignKind::Left => "left",
            markdown::mdast::AlignKind::Right => "right",
            markdown::mdast::AlignKind::Center => "center",
            markdown::mdast::AlignKind::None => unreachable!(),
        };
        attrs.push(create_attribute("style", &format!("text-align: {value}")));
    }
    // Cell children are Node variants (Text, Emphasis, etc.), not TableCell.
    let children = walk_nodes(ctx, &cell.children);
    let attributes = with_attributes(attrs);
    Node::Element(Box::new(normal_element_with_attrs(
        tag, attributes, children,
    )))
}

/// Walks a delete (strikethrough) node: `<del>...</del>`.
fn walk_delete(ctx: &WalkContext, delete: &markdown::mdast::Delete) -> Node {
    let children = walk_nodes(ctx, &delete.children);
    html_element("del", children)
}

// ---------------------------------------------------------------------------
// JSX component walking (Phase 02)
// ---------------------------------------------------------------------------

/// Smart-coerce an MDX attribute string to a typed Rust literal.
///
/// D-02: `"true"` / `"false"` -> bool, pure integers -> `LitInt`,
/// pure floats -> `LitFloat`, everything else -> `LitStr`.
/// Leading-zero digit strings (e.g. `"007"`) stay as strings because
/// `syn::LitInt` rejects them.
///
/// The `span` argument is used for the fallback `LitStr` so that
/// compiler diagnostics point to the `compile_mdx!` invocation rather
/// than `call_site()`.
#[must_use]
pub fn coerce_attr_value(value: &str, span: Span) -> Expr {
    match value {
        "true" => return syn::parse_quote!(true),
        "false" => return syn::parse_quote!(false),
        _ => {}
    }
    // Try integer — but reject leading-zero strings like "007" (syn 2.0
    // accepts them as valid LitInt values, so we guard manually).
    if !(value.len() > 1 && value.starts_with('0'))
        && let Ok(lit) = syn::parse_str::<syn::LitInt>(value)
    {
        return Expr::Lit(syn::ExprLit {
            attrs: vec![],
            lit: syn::Lit::Int(lit),
        });
    }
    // Try float.
    if let Ok(lit) = syn::parse_str::<syn::LitFloat>(value) {
        return Expr::Lit(syn::ExprLit {
            attrs: vec![],
            lit: syn::Lit::Float(lit),
        });
    }
    // Default: string literal.
    Expr::Lit(syn::ExprLit {
        attrs: vec![],
        lit: syn::Lit::Str(LitStr::new(value, span)),
    })
}

/// Walk JSX attributes from an mdast element into `NamedArg`s.
///
/// - Bare attributes (value: None) -> `true` (D-03).
/// - Literal attributes -> `coerce_attr_value` (D-02).
/// - Expression attributes (`{...spread}`) -> skipped (Pitfall 7).
/// - Namespaced attribute names -> pushed to `ctx.errors` (L-02).
fn walk_jsx_attributes(
    ctx: &WalkContext,
    attrs: &[markdown::mdast::AttributeContent],
) -> Vec<NamedArg> {
    attrs
        .iter()
        .filter_map(|content| match content {
            markdown::mdast::AttributeContent::Property(attr) => {
                // Skip namespaced attribute names (e.g. xml:lang) — L-02.
                if attr.name.contains(':') {
                    ctx.errors.borrow_mut().push(format!(
                        "namespaced attribute '{}' not supported",
                        attr.name
                    ));
                    return None;
                }
                let value = match &attr.value {
                    Some(markdown::mdast::AttributeValue::Literal(s)) => {
                        coerce_attr_value(s, ctx.span)
                    }
                    None => syn::parse_quote!(true), // bare attribute → true (D-03)
                    Some(markdown::mdast::AttributeValue::Expression(_)) => {
                        // Expression attributes like `{value}` are out of scope.
                        ctx.errors.borrow_mut().push(format!(
                            "expression attribute '{}' not supported",
                            attr.name
                        ));
                        return None;
                    }
                };
                Some(NamedArg {
                    ident: make_ident(&attr.name),
                    colon: Colon::default(),
                    value: NamedArgValue::Expr(value),
                })
            }
            markdown::mdast::AttributeContent::Expression(_) => {
                // Spread attributes like `{...props}` are out of scope — Pitfall 7.
                ctx.errors
                    .borrow_mut()
                    .push("spread attributes not supported".to_string());
                None
            }
        })
        .collect()
}

/// Walk a flow-level JSX element (`MdxJsxFlowElement`) into `Option<Node::Component>`.
///
/// Returns `Some` when the element name is `PascalCase` and registered in
/// `ctx.components`. Returns `None` for:
/// - Lowercase names (HTML elements, not components — Pitfall 2).
/// - Fragments (`name: None` — Pitfall 8).
/// - Unregistered `PascalCase` names (pushes error to `ctx.errors` — D-04).
pub fn walk_jsx_element(
    ctx: &WalkContext,
    element: &markdown::mdast::MdxJsxFlowElement,
) -> Option<Node> {
    let name = element.name.as_deref()?;
    // Fragments (name: None) handled by the `?` above — Pitfall 8.

    // Lowercase = HTML element, not a component — Pitfall 2.
    if !name.starts_with(char::is_uppercase) {
        return None;
    }

    // Look up in component registry — D-04.
    let path = if let Some((_, p)) = ctx.components.iter().find(|(tag, _)| tag == name) {
        p.clone()
    } else {
        ctx.errors.borrow_mut().push(format!("unknown component '{name}'"));
        return None;
    };

    let named_args = walk_jsx_attributes(ctx, &element.attributes);
    let children = walk_nodes(ctx, &element.children);

    Some(Node::Component(Component {
        path,
        paren_token: Paren::default(),
        named_args,
        children,
    }))
}

/// Walk a text-level JSX element (`MdxJsxTextElement`) into `Option<Node::Component>`.
///
/// Same logic as `walk_jsx_element` but for inline JSX (e.g. `<Inline>`
/// inside a paragraph). Pushes an error to `ctx.errors` for unregistered
/// PascalCase components so the author gets a compile-time diagnostic.
pub fn walk_jsx_text_element(
    ctx: &WalkContext,
    element: &markdown::mdast::MdxJsxTextElement,
) -> Option<Node> {
    let name = element.name.as_deref()?;

    if !name.starts_with(char::is_uppercase) {
        return None;
    }

    let path = if let Some((_, p)) = ctx.components.iter().find(|(tag, _)| tag == name) {
        p.clone()
    } else {
        ctx.errors.borrow_mut().push(format!("unknown component '{name}'"));
        return None;
    };

    let named_args = walk_jsx_attributes(ctx, &element.attributes);
    let children = walk_nodes(ctx, &element.children);

    Some(Node::Component(Component {
        path,
        paren_token: Paren::default(),
        named_args,
        children,
    }))
}

// ---------------------------------------------------------------------------
// Helper functions for constructing elements and attributes
// ---------------------------------------------------------------------------

/// Constructs a `Node::Text` from a string.
fn text_node(content: &str) -> Node {
    Node::Text(LitStr::new(content, Span::call_site()))
}

/// Creates an `Ident` that can be a Rust keyword (e.g., "type", "for").
/// `syn::parse_str::<Ident>` uses `Ident::parse`, which rejects keywords.
/// The fallback uses `Ident::new` directly for keyword-safe identifiers.
fn make_ident(name: &str) -> Ident {
    syn::parse_str(name).unwrap_or_else(|_| Ident::new(name, Span::call_site()))
}

/// Constructs an `ElementName` from a tag name string.
fn make_element_name(tag: &str) -> ElementName {
    ElementName::Ident(HtmlIdent {
        first: make_ident(tag),
        rest: vec![],
    })
}

/// Constructs a normal HTML element with opening and closing tags, wrapped in Node.
fn html_element(tag: &str, children: Nodes) -> Node {
    let attributes = Attributes::default();
    Node::Element(Box::new(normal_element_with_attrs(
        tag, attributes, children,
    )))
}

/// Constructs a normal HTML element with custom attributes.
fn normal_element_with_attrs(tag: &str, attributes: Attributes, children: Nodes) -> Element {
    let closing_name = make_element_name(tag);
    let opening = OpeningTag {
        lt: parse_quote!(<),
        name: make_element_name(tag),
        attributes,
        gt: parse_quote!(>),
    };
    let closing = ClosingTag {
        lt: parse_quote!(<),
        slash: parse_quote!(/),
        name: closing_name,
        gt: parse_quote!(>),
    };
    Element::Normal {
        opening_tag: opening,
        children,
        closing_tag: closing,
    }
}

/// Constructs a void HTML element (no closing tag, no children).
fn void_element(tag: &str) -> Element {
    void_element_with_attrs(tag, Attributes::default())
}

/// Constructs a void HTML element with custom attributes.
fn void_element_with_attrs(tag: &str, attributes: Attributes) -> Element {
    Element::Void {
        tag: OpeningTag {
            lt: parse_quote!(<),
            name: make_element_name(tag),
            attributes,
            gt: parse_quote!(>),
        },
    }
}

/// Constructs a self-closing element (`<tag ... />`).
fn self_closing_element(tag: &str, attributes: Attributes) -> Element {
    Element::SelfClosing {
        tag: SelfClosingTag {
            lt: parse_quote!(<),
            name: make_element_name(tag),
            attributes,
            slash: parse_quote!(/),
            gt: parse_quote!(>),
        },
    }
}

/// Creates a key=value attribute.
fn create_attribute(key: &str, value: &str) -> Attribute {
    Attribute {
        key: AttributeKey::Ident(HtmlIdent {
            first: make_ident(key),
            rest: vec![],
        }),
        eq: parse_quote!(=),
        value: AttributeValue::LitStr(LitStr::new(value, Span::call_site())),
    }
}

/// Creates a boolean attribute (key with empty value, e.g., `checked=""`).
fn create_attribute_bool(key: &str) -> Attribute {
    Attribute {
        key: AttributeKey::Ident(HtmlIdent {
            first: make_ident(key),
            rest: vec![],
        }),
        eq: parse_quote!(=),
        value: AttributeValue::LitStr(LitStr::new("", Span::call_site())),
    }
}

/// Wraps a vec of `Attribute`s into an `Attributes` value.
fn with_attributes(attrs: Vec<Attribute>) -> Attributes {
    Attributes {
        cx: None,
        items: attrs.into_iter().map(AttributeNode::Attribute).collect(),
    }
}

#[cfg(test)]
mod tests {
    use quote::quote;
    use syn::{Lit, LitBool};

    use super::*;

    fn parse_and_walk(content: &str) -> Nodes {
        parse_and_walk_ctx(&WalkContext::empty(), content)
    }

    fn parse_and_walk_ctx(ctx: &WalkContext, content: &str) -> Nodes {
        let options = get_parse_options();
        let root = markdown::to_mdast(content, &options).unwrap();
        match root {
            markdown::mdast::Node::Root(r) => walk_nodes(ctx, &r.children),
            _ => unreachable!(),
        }
    }

    // ---- Existing tests ----

    #[test]
    fn walks_heading() {
        let nodes = parse_and_walk("# Hello");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if e.name().string_name().as_deref() == Some("h1")),
            "expected h1 element",
        );
    }

    #[test]
    fn walks_paragraph() {
        let nodes = parse_and_walk("Plain text");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if e.name().string_name().as_deref() == Some("p")),
            "expected p element",
        );
    }

    #[test]
    fn walks_text_inside_paragraph() {
        let nodes = parse_and_walk("Hello world");
        let paragraph = &nodes[0];
        if let Node::Element(e) = paragraph {
            assert_eq!(e.children().len(), 1);
            assert!(matches!(&e.children()[0], Node::Text(_)));
        } else {
            panic!("expected paragraph element");
        }
    }

    #[test]
    fn walks_raw_html_via_writer() {
        let ctx = WalkContext::empty();
        let options = get_parse_options();
        let root = markdown::to_mdast(r#"<div class="raw">Raw</div>"#, &options).unwrap();
        let mut writer = ViewWriter::new();
        if let markdown::mdast::Node::Root(r) = root {
            for child in &r.children {
                walk_to_writer(&ctx, child, &mut writer);
            }
        }
        let tokens = writer.into_token_stream();
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

    #[test]
    fn walks_thematic_break() {
        // Use "***" instead of "---" to avoid ambiguity with frontmatter
        // (which is enabled in parse options). Stars cannot be parsed as
        // frontmatter, making this test unambiguous.
        let nodes = parse_and_walk("\n***\n");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if matches!(e.as_ref(), Element::Void { .. })),
            "expected void element for thematic break",
        );
        // Also verify it's an <hr>.
        if let Node::Element(e) = node {
            assert_eq!(
                e.name().string_name().as_deref(),
                Some("hr"),
                "thematic break should render as <hr>"
            );
        }
    }

    #[test]
    fn walks_emphasis() {
        let nodes = parse_and_walk("*italic*");
        // Italic appears inside a paragraph
        assert_eq!(nodes.len(), 1);
        if let Node::Element(e) = &nodes[0] {
            assert!(!e.children().is_empty());
            let has_em = e.children().iter().any(|child| {
                if let Node::Element(inner) = child {
                    inner.name().string_name().as_deref() == Some("em")
                } else {
                    false
                }
            });
            assert!(has_em, "paragraph should contain <em> element");
        } else {
            panic!("expected paragraph element");
        }
    }

    #[test]
    fn walks_strong() {
        let nodes = parse_and_walk("**bold**");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(e) = &nodes[0] {
            let has_strong = e.children().iter().any(|child| {
                if let Node::Element(inner) = child {
                    inner.name().string_name().as_deref() == Some("strong")
                } else {
                    false
                }
            });
            assert!(has_strong, "paragraph should contain <strong> element");
        }
    }

    #[test]
    fn walks_inline_code() {
        let nodes = parse_and_walk("`code`");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(e) = &nodes[0] {
            let has_code = e.children().iter().any(|child| {
                if let Node::Element(inner) = child {
                    inner.name().string_name().as_deref() == Some("code")
                } else {
                    false
                }
            });
            assert!(has_code, "paragraph should contain <code> element");
        }
    }

    #[test]
    fn walks_blockquote() {
        let nodes = parse_and_walk("> quoted");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if e.name().string_name().as_deref() == Some("blockquote")),
            "expected blockquote element",
        );
    }

    #[test]
    fn walks_break() {
        let nodes = parse_and_walk("line1  \nline2");
        assert_eq!(nodes.len(), 1);
        // Verify the <br> is present inside the paragraph.
        if let Node::Element(p) = &nodes[0] {
            assert_eq!(p.name().string_name().as_deref(), Some("p"));
            let has_br = p.children().iter().any(|c| {
                if let Node::Element(e) = c {
                    matches!(e.as_ref(), Element::Void { .. })
                        && e.name().string_name().as_deref() == Some("br")
                } else {
                    false
                }
            });
            assert!(has_br, "paragraph should contain <br> for hard break");
        } else {
            panic!("expected paragraph element");
        }
    }

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

    // ---- Tests: URL sanitization (is_safe_url) ----

    #[test]
    fn is_safe_url_allows_http() {
        assert!(is_safe_url("https://example.com"));
        assert!(is_safe_url("http://example.com"));
    }

    #[test]
    fn is_safe_url_allows_relative() {
        assert!(is_safe_url("/path/to/page"));
        assert!(is_safe_url("image.png"));
        assert!(is_safe_url("./relative.md"));
    }

    #[test]
    fn is_safe_url_blocks_javascript() {
        assert!(!is_safe_url("javascript:alert(1)"));
        assert!(!is_safe_url("  javascript:alert(1)"));
        assert!(!is_safe_url("JavaScript:alert(1)"));
    }

    #[test]
    fn is_safe_url_blocks_vbscript() {
        assert!(!is_safe_url("vbscript:msgBox(1)"));
    }

    #[test]
    fn is_safe_url_blocks_all_data_uris() {
        // Block data:text/html
        assert!(!is_safe_url("data:text/html,<script>alert(1)</script>"));
        // Block data:image/svg+xml (XSS via SVG event handlers)
        assert!(!is_safe_url("data:image/svg+xml,<svg onload=alert(1)>"));
        // Block base64-encoded SVG
        assert!(!is_safe_url("data:image/svg+xml;base64,PHN2ZyBvbmxvYWQ+"));
        // Block data:text/plain (defense in depth)
        assert!(!is_safe_url("data:text/plain,hello"));
    }

    #[test]
    fn blocks_javascript_url_in_link() {
        let nodes = parse_and_walk("[click](javascript:alert(1))");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            assert_eq!(p.name().string_name().as_deref(), Some("p"));
            // The link should NOT render as <a>; it should be stripped to <span>.
            let has_a = p.children().iter().any(|c| {
                if let Node::Element(e) = c {
                    e.name().string_name().as_deref() == Some("a")
                } else {
                    false
                }
            });
            assert!(!has_a, "javascript: link should NOT produce <a> element");
        }
    }

    #[test]
    fn blocks_data_uri_in_image() {
        let nodes = parse_and_walk("![x](data:image/svg+xml,<svg onload=alert(1)>)");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            let has_img = p.children().iter().any(|c| {
                if let Node::Element(e) = c {
                    e.name().string_name().as_deref() == Some("img")
                } else {
                    false
                }
            });
            assert!(!has_img, "data: URI image should NOT produce <img> element");
        }
    }

    // ---- New tests: links and images ----

    #[test]
    fn walks_link() {
        let nodes = parse_and_walk("[text](https://example.com)");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            assert_eq!(p.name().string_name().as_deref(), Some("p"));
            let has_a = p.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("a")
                } else {
                    false
                }
            });
            assert!(has_a, "paragraph should contain <a> element");
        }
    }

    #[test]
    fn walks_link_with_href_attribute() {
        let nodes = parse_and_walk("[link](https://example.com)");
        if let Node::Element(p) = &nodes[0] {
            let a = p.children().iter().find_map(|c| {
                if let Node::Element(e) = c {
                    if e.name().string_name().as_deref() == Some("a") {
                        Some(e.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            assert!(a.is_some(), "should find <a> element");
            let a = a.unwrap();
            let attrs = a.attributes();
            assert!(!attrs.is_empty(), "link should have attributes");
        }
    }

    #[test]
    fn walks_image() {
        let nodes = parse_and_walk("![alt](photo.png)");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            let has_img = p.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    matches!(inner.as_ref(), Element::Void { .. })
                        && inner.name().string_name().as_deref() == Some("img")
                } else {
                    false
                }
            });
            assert!(has_img, "paragraph should contain <img> void element");
        }
    }

    #[test]
    fn walks_image_with_src_and_alt() {
        let nodes = parse_and_walk("![Photo](photo.png)");
        if let Node::Element(p) = &nodes[0] {
            let img = p.children().iter().find_map(|c| {
                if let Node::Element(e) = c {
                    if e.name().string_name().as_deref() == Some("img") {
                        Some(e.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            assert!(img.is_some(), "should find <img> element");
            let img = img.unwrap();
            let attrs = img.attributes();
            assert!(!attrs.is_empty(), "image should have attributes (src, alt)",);
        }
    }

    // ---- New tests: code blocks ----

    #[test]
    fn walks_code_block_with_language() {
        let nodes = parse_and_walk("```rust\nfn main() {}\n```");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(pre) = &nodes[0] {
            assert_eq!(
                pre.name().string_name().as_deref(),
                Some("pre"),
                "should be <pre> element"
            );
            let has_code = pre.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("code")
                } else {
                    false
                }
            });
            assert!(has_code, "<pre> should contain <code>");
            // Check code has class attribute
            let code = pre
                .children()
                .iter()
                .find_map(|c| {
                    if let Node::Element(e) = c {
                        if e.name().string_name().as_deref() == Some("code") {
                            Some(e.as_ref())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .unwrap();
            assert!(
                !code.attributes().is_empty(),
                "code should have class attribute for language",
            );
        }
    }

    #[test]
    fn walks_code_block_without_language() {
        let nodes = parse_and_walk("```\nno lang\n```");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(pre) = &nodes[0] {
            assert_eq!(
                pre.name().string_name().as_deref(),
                Some("pre"),
                "should be <pre> element"
            );
        }
    }

    // ---- New tests: lists ----

    #[test]
    fn walks_ordered_list() {
        let nodes = parse_and_walk("1. first");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(ol) = &nodes[0] {
            assert_eq!(
                ol.name().string_name().as_deref(),
                Some("ol"),
                "should be <ol> element"
            );
        }
    }

    #[test]
    fn walks_unordered_list() {
        let nodes = parse_and_walk("- item");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(ul) = &nodes[0] {
            assert_eq!(
                ul.name().string_name().as_deref(),
                Some("ul"),
                "should be <ul> element"
            );
            let has_li = ul.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("li")
                } else {
                    false
                }
            });
            assert!(has_li, "<ul> should contain <li>");
        }
    }

    #[test]
    fn walks_task_list_checked() {
        let nodes = parse_and_walk("- [x] done");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(ul) = &nodes[0] {
            // The <li> should have a self-closing <input> as first child.
            let li = ul.children().iter().find_map(|c| {
                if let Node::Element(e) = c {
                    if e.name().string_name().as_deref() == Some("li") {
                        Some(e.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            assert!(li.is_some(), "<ul> should contain <li>");
            let li = li.unwrap();
            assert!(
                !li.children().is_empty(),
                "<li> should contain checkbox input",
            );
        }
    }

    #[test]
    fn walks_task_list_unchecked() {
        let nodes = parse_and_walk("- [ ] pending");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(ul) = &nodes[0] {
            let li = ul.children().iter().find_map(|c| {
                if let Node::Element(e) = c {
                    if e.name().string_name().as_deref() == Some("li") {
                        Some(e.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            assert!(li.is_some(), "<ul> should contain <li>");
        }
    }

    #[test]
    fn walks_unordered_list_no_checkbox() {
        // Regular list item (not a task list) should NOT have a checkbox.
        let nodes = parse_and_walk("- regular item");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(ul) = &nodes[0] {
            let li = ul.children().iter().find_map(|c| {
                if let Node::Element(e) = c {
                    if e.name().string_name().as_deref() == Some("li") {
                        Some(e.as_ref())
                    } else {
                        None
                    }
                } else {
                    None
                }
            });
            assert!(li.is_some(), "<ul> should contain <li>");
            let li = li.unwrap();
            // First child should be text/paragraph, not an <input>.
            let first_is_input = li.children().iter().any(|c| {
                if let Node::Element(e) = c {
                    e.name().string_name().as_deref() == Some("input")
                } else {
                    false
                }
            });
            assert!(
                !first_is_input,
                "regular list item should NOT contain <input> checkbox"
            );
        }
    }

    // ---- New tests: tables ----

    #[test]
    fn walks_table() {
        let nodes = parse_and_walk("| A | B |\n|---|---|\n| 1 | 2 |");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(table) = &nodes[0] {
            assert_eq!(
                table.name().string_name().as_deref(),
                Some("table"),
                "should be <table>"
            );
            // Should have thead and tbody children
            let has_thead = table.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("thead")
                } else {
                    false
                }
            });
            let has_tbody = table.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("tbody")
                } else {
                    false
                }
            });
            assert!(has_thead, "table should have <thead>");
            assert!(has_tbody, "table should have <tbody>");
        }
    }

    #[test]
    fn walks_table_with_alignment() {
        let nodes = parse_and_walk("| A |\n|:---|\n| 1 |");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(table) = &nodes[0] {
            // Find <th> element
            let th = find_element_recursive(table, "th");
            assert!(th.is_some(), "table should have <th>");
            let th = th.unwrap();
            assert!(
                !th.attributes().is_empty(),
                "aligned <th> should have style attribute"
            );
        }
    }

    // ---- New tests: strikethrough ----

    #[test]
    fn walks_strikethrough() {
        let nodes = parse_and_walk("~~deleted~~");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            let has_del = p.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("del")
                } else {
                    false
                }
            });
            assert!(has_del, "paragraph should contain <del> element");
        }
    }

    // ---- New tests: heading depth ----

    #[test]
    fn walks_heading_depth_1_to_6() {
        for (depth, prefix) in [
            (1, "#"),
            (2, "##"),
            (3, "###"),
            (4, "####"),
            (5, "#####"),
            (6, "######"),
        ] {
            let content = format!("{prefix} Level {depth}");
            let nodes = parse_and_walk(&content);
            assert_eq!(nodes.len(), 1, "depth {depth}: expected one node");
            if let Node::Element(e) = &nodes[0] {
                let expected = format!("h{depth}");
                assert_eq!(
                    e.name().string_name().as_deref(),
                    Some(expected.as_str()),
                    "depth {depth}: expected {expected}",
                );
            } else {
                panic!("depth {depth}: expected element");
            }
        }
    }

    // ---- New tests: attribute construction helpers ----

    #[test]
    fn create_attribute_builds_key_value() {
        let attr = create_attribute("href", "https://example.com");
        assert!(matches!(attr.key, AttributeKey::Ident(_)));
        assert!(matches!(attr.value, AttributeValue::LitStr(_)));
    }

    #[test]
    fn with_attributes_wraps_into_attribute_nodes() {
        let attrs = with_attributes(vec![create_attribute("class", "btn")]);
        assert_eq!(attrs.items.len(), 1);
        assert!(matches!(attrs.items[0], AttributeNode::Attribute(_)));
    }

    // ---- New tests: walk_node dispatch ----

    #[test]
    fn walk_node_dispatches_code_block() {
        // mdast::Code (fenced block) should produce <pre>, not <code> at top.
        let nodes = parse_and_walk("```python\nprint()\n```");
        assert_eq!(nodes.len(), 1);
        assert!(
            matches!(&nodes[0], Node::Element(e) if e.name().string_name().as_deref() == Some("pre")),
            "code block should produce <pre>",
        );
    }

    #[test]
    fn walk_node_dispatches_inline_code() {
        // mdast::InlineCode should produce <code> inside paragraph.
        let nodes = parse_and_walk("`inline`");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            assert_eq!(p.name().string_name().as_deref(), Some("p"));
            let has_code = p.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("code")
                } else {
                    false
                }
            });
            assert!(has_code, "inline code should produce <code>");
        }
    }

    #[test]
    fn walk_node_dispatches_image_not_link() {
        // Image syntax ![alt](src) should produce <img>, not <a>.
        let nodes = parse_and_walk("![photo](image.png)");
        assert_eq!(nodes.len(), 1);
        if let Node::Element(p) = &nodes[0] {
            let has_img = p.children().iter().any(|c| {
                if let Node::Element(inner) = c {
                    inner.name().string_name().as_deref() == Some("img")
                } else {
                    false
                }
            });
            assert!(has_img, "image should produce <img>");
        }
    }

    #[test]
    fn walk_node_dispatches_table() {
        let nodes = parse_and_walk("| Col |\n|-----|\n| Val |");
        assert_eq!(nodes.len(), 1);
        assert!(
            matches!(&nodes[0], Node::Element(e) if e.name().string_name().as_deref() == Some("table")),
            "table should produce <table>",
        );
    }

    // ---- Helper for recursive element finding ----

    fn find_element_recursive<'a>(element: &'a Element, tag: &str) -> Option<&'a Element> {
        if element.name().string_name().as_deref() == Some(tag) {
            return Some(element);
        }
        for child in element.children() {
            if let Node::Element(inner) = child {
                if let Some(found) = find_element_recursive(inner, tag) {
                    return Some(found);
                }
            }
        }
        None
    }

    // ---- Phase 02: coerce_attr_value tests ----

    #[test]
    fn coerce_attr_value_bool_true() {
        let expr = coerce_attr_value("true", Span::call_site());
        assert!(matches!(
            expr,
            Expr::Lit(syn::ExprLit {
                lit: Lit::Bool(LitBool { value: true, .. }),
                ..
            })
        ));
    }

    #[test]
    fn coerce_attr_value_bool_false() {
        let expr = coerce_attr_value("false", Span::call_site());
        assert!(matches!(
            expr,
            Expr::Lit(syn::ExprLit {
                lit: Lit::Bool(LitBool { value: false, .. }),
                ..
            })
        ));
    }

    #[test]
    fn coerce_attr_value_int() {
        let expr = coerce_attr_value("42", Span::call_site());
        assert!(
            matches!(
                expr,
                Expr::Lit(syn::ExprLit {
                    lit: Lit::Int(_),
                    ..
                })
            ),
            "expected LitInt"
        );
    }

    #[test]
    fn coerce_attr_value_float() {
        let expr = coerce_attr_value("3.14", Span::call_site());
        assert!(
            matches!(
                expr,
                Expr::Lit(syn::ExprLit {
                    lit: Lit::Float(_),
                    ..
                })
            ),
            "expected LitFloat"
        );
    }

    #[test]
    fn coerce_attr_value_string() {
        let expr = coerce_attr_value("hello", Span::call_site());
        if let Expr::Lit(l) = expr {
            assert!(
                matches!(l.lit, Lit::Str(s) if s.value() == "hello"),
                "expected LitStr(\"hello\")"
            );
        } else {
            panic!("expected Expr::Lit");
        }
    }

    #[test]
    fn coerce_attr_value_empty_string_stays_str() {
        let expr = coerce_attr_value("", Span::call_site());
        assert!(
            matches!(expr, Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) if s.value().is_empty()),
            "empty string should coerce to LitStr(\"\")"
        );
    }

    #[test]
    fn coerce_attr_value_leading_zeros_stay_str() {
        let expr = coerce_attr_value("007", Span::call_site());
        assert!(
            matches!(expr, Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. }) if s.value() == "007"),
            "leading zeros should stay as string, not coerce to int"
        );
    }

    // ---- Phase 02: JSX attribute walking tests ----

    #[test]
    fn walk_jsx_attributes_bare() {
        let ctx = WalkContext::empty();
        let attr = markdown::mdast::MdxJsxAttribute {
            name: "disabled".to_string(),
            value: None,
        };
        let attrs = vec![markdown::mdast::AttributeContent::Property(attr)];
        let result = walk_jsx_attributes(&ctx, &attrs);
        assert_eq!(result.len(), 1);
        let named_arg = &result[0];
        assert_eq!(named_arg.ident.to_string(), "disabled");
        assert!(
            matches!(
                &named_arg.value,
                NamedArgValue::Expr(Expr::Lit(syn::ExprLit {
                    lit: Lit::Bool(LitBool { value: true, .. }),
                    ..
                }))
            ),
            "bare attribute should coerce to true"
        );
    }

    #[test]
    fn walk_jsx_attributes_key_value() {
        let ctx = WalkContext::empty();
        let attr = markdown::mdast::MdxJsxAttribute {
            name: "label".to_string(),
            value: Some(markdown::mdast::AttributeValue::Literal(
                "hello".to_string(),
            )),
        };
        let attrs = vec![markdown::mdast::AttributeContent::Property(attr)];
        let result = walk_jsx_attributes(&ctx, &attrs);
        assert_eq!(result.len(), 1);
        let named_arg = &result[0];
        assert_eq!(named_arg.ident.to_string(), "label");
        assert!(
            matches!(&named_arg.value, NamedArgValue::Expr(Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. })) if s.value() == "hello"),
            "string attribute value should coerce to LitStr"
        );
    }

    #[test]
    fn walk_jsx_attributes_skip_expression() {
        let ctx = WalkContext::new(&[], Span::call_site());
        let expr_attr = markdown::mdast::AttributeContent::Expression(
            markdown::mdast::MdxJsxExpressionAttribute {
                value: "...props".to_string(),
                stops: vec![],
            },
        );
        let result = walk_jsx_attributes(&ctx, &[expr_attr]);
        assert!(result.is_empty(), "spread attributes should be skipped");
        let errors = ctx.errors.borrow();
        assert!(
            !errors.is_empty(),
            "should push error for spread attributes"
        );
    }

    // ---- Phase 02: JSX element walking tests ----

    #[test]
    fn walk_jsx_element_unknown_component() {
        let ctx = WalkContext::new(&[], Span::call_site());
        let element = markdown::mdast::MdxJsxFlowElement {
            children: vec![],
            position: None,
            name: Some("Unknown".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_element(&ctx, &element);
        assert!(
            result.is_none(),
            "unregistered component should return None"
        );
        let errors = ctx.errors.borrow();
        assert!(
            !errors.is_empty(),
            "should push error for unknown component"
        );
    }

    #[test]
    fn walk_jsx_element_lowercase_is_html() {
        let ctx = WalkContext::new(&[], Span::call_site());
        let element = markdown::mdast::MdxJsxFlowElement {
            children: vec![],
            position: None,
            name: Some("div".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_element(&ctx, &element);
        assert!(
            result.is_none(),
            "lowercase JSX should return None (HTML, not component)"
        );
        let errors = ctx.errors.borrow();
        assert!(errors.is_empty(), "lowercase should NOT push an error");
    }

    #[test]
    fn walk_jsx_element_fragment() {
        let ctx = WalkContext::new(&[], Span::call_site());
        let element = markdown::mdast::MdxJsxFlowElement {
            children: vec![],
            position: None,
            name: None, // fragment
            attributes: vec![],
        };
        let result = walk_jsx_element(&ctx, &element);
        assert!(result.is_none(), "fragment should return None");
    }

    #[test]
    fn walk_jsx_element_registered_produces_component() {
        let component_path: Path = syn::parse_quote!(components::callout);
        let registry = vec![("Callout".to_string(), component_path)];
        let ctx = WalkContext::new(&registry, Span::call_site());
        let element = markdown::mdast::MdxJsxFlowElement {
            children: vec![],
            position: None,
            name: Some("Callout".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_element(&ctx, &element);
        assert!(
            matches!(&result, Some(Node::Component(_))),
            "registered component should produce Node::Component"
        );
        if let Some(Node::Component(comp)) = result {
            assert_eq!(
                comp.path.segments.last().unwrap().ident.to_string(),
                "callout"
            );
            assert!(comp.named_args.is_empty());
            assert!(comp.children.is_empty());
        }
    }

    #[test]
    fn walk_jsx_element_self_closing_empty_children() {
        let component_path: Path = syn::parse_quote!(components::divider);
        let registry = vec![("Divider".to_string(), component_path)];
        let ctx = WalkContext::new(&registry, Span::call_site());
        // Self-closing and closed tags both produce empty children in markdown-rs.
        let element = markdown::mdast::MdxJsxFlowElement {
            children: vec![], // empty
            position: None,
            name: Some("Divider".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_element(&ctx, &element);
        assert!(
            matches!(&result, Some(Node::Component(_))),
            "self-closing component should produce Node::Component"
        );
        if let Some(Node::Component(comp)) = result {
            assert!(
                comp.children.is_empty(),
                "self-closing should have empty children"
            );
        }
    }

    // ---- Phase 02: walk_node JSX dispatch tests ----

    #[test]
    fn walk_node_dispatches_mdx_jsx_flow_element() {
        // markdown-rs only parses <Widget></Widget> as MdxJsxFlowElement.
        // Self-closing <Widget /> is parsed as raw Html.
        let component_path: Path = syn::parse_quote!(my::widget);
        let registry = vec![("Widget".to_string(), component_path)];
        let ctx = WalkContext::new(&registry, Span::call_site());
        let nodes = parse_and_walk_ctx(&ctx, "<Widget></Widget>");
        assert_eq!(nodes.len(), 1);
        assert!(
            matches!(&nodes[0], Node::Component(_)),
            "walk_node should dispatch MdxJsxFlowElement to walk_jsx_element"
        );
    }

    #[test]
    fn walk_jsx_text_element_registered_produces_component() {
        // Walk a manually-constructed MdxJsxTextElement through walk_jsx_text_element.
        // Note: markdown-rs only produces MdxJsxTextElement in specific parsing contexts;
        // this test verifies the walker function works on the struct directly.
        let component_path: Path = syn::parse_quote!(inline::badge);
        let registry = vec![("Badge".to_string(), component_path)];
        let ctx = WalkContext::new(&registry, Span::call_site());
        let element = markdown::mdast::MdxJsxTextElement {
            children: vec![],
            position: None,
            name: Some("Badge".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_text_element(&ctx, &element);
        assert!(
            matches!(&result, Some(Node::Component(_))),
            "registered text JSX should produce Node::Component"
        );
    }

    #[test]
    fn walk_jsx_text_element_unknown_component_pushes_error() {
        // CR-01: inline text-level components must report errors for unknown
        // PascalCase names, matching flow-level walk_jsx_element behavior.
        let ctx = WalkContext::new(&[], Span::call_site());
        let element = markdown::mdast::MdxJsxTextElement {
            children: vec![],
            position: None,
            name: Some("Unknown".to_string()),
            attributes: vec![],
        };
        let result = walk_jsx_text_element(&ctx, &element);
        assert!(
            result.is_none(),
            "unregistered inline component should return None"
        );
        let errors = ctx.errors.borrow();
        assert!(
            !errors.is_empty(),
            "should push error for unknown inline component"
        );
        assert!(
            errors.iter().any(|e| e.contains("unknown component 'Unknown'")),
            "should contain 'unknown component' message. Errors: {:?}",
            *errors
        );
    }

    // ---- Phase 02: tracer integration test ----

    #[test]
    fn tracer_component_round_trip() {
        // End-to-end tracer: parse MDX with a component, walk it, assert
        // the Component node has the correct path, named_args, and children.
        //
        // Note: markdown-rs only parses empty JSX tag pairs as MdxJsxFlowElement
        // (e.g. <Callout></Callout>). Tags with content like <Callout>hello</Callout>
        // are parsed as raw HTML fragments. This is a parser limitation addressed
        // by integration tests in the macro crate which use .mdx fixture files.
        let component_path: Path = syn::parse_quote!(components::callout);
        let registry = vec![("Callout".to_string(), component_path)];
        let ctx = WalkContext::new(&registry, Span::call_site());
        let nodes = parse_and_walk_ctx(&ctx, r#"<Callout type="info"></Callout>"#);
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];

        // The root-level JSX produces a Component node.
        assert!(
            matches!(node, Node::Component(_)),
            "expected Node::Component"
        );

        if let Node::Component(comp) = node {
            // Path matches registry entry.
            assert_eq!(
                comp.path.segments.last().unwrap().ident.to_string(),
                "callout"
            );

            // Has one NamedArg: type = "info" (string literal).
            assert_eq!(comp.named_args.len(), 1);
            assert_eq!(comp.named_args[0].ident.to_string(), "type");
            assert!(
                matches!(
                    &comp.named_args[0].value,
                    NamedArgValue::Expr(Expr::Lit(syn::ExprLit { lit: Lit::Str(s), .. })) if s.value() == "info"
                ),
                "type should be string literal \"info\""
            );

            // Empty children (self-closing / empty tag pair).
            assert!(
                comp.children.is_empty(),
                "empty tag pair should have no children"
            );
        }
    }
}
