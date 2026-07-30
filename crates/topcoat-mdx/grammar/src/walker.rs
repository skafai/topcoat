//! mdast-to-view AST walker.
//!
//! Transforms `markdown-rs` mdast nodes into Topcoat `view!` AST types
//! (`Node`, `Element`, `Nodes`), enabling markdown content to be rendered
//! through the same code generation pipeline as handwritten templates.

use proc_macro2::Span;
use syn::{parse_quote, LitStr};
use topcoat_view_grammar::attributes::Attributes;
use topcoat_view_grammar::view::*;
use topcoat_view_grammar::view::hir::{LowerView, ViewBuilder};

use crate::parse::get_parse_options;

/// Walks an mdast node tree into a Topcoat `view!` `View`.
///
/// Parses the MDX content using `markdown-rs` with GFM + MDX + frontmatter
/// enabled, then walks the resulting mdast into a `View` value ready for
/// token emission via `ToTokens`.
pub fn mdx_to_view(mdx_content: &str) -> View {
    let options = get_parse_options();
    let root = markdown::to_mdast(mdx_content, &options)
        .expect("markdown parse failed — handled by compile_mdx! macro");

    let nodes = match root {
        markdown::mdast::Node::Root(r) => walk_nodes(&r.children),
        _ => Nodes::new(),
    };
    View { cx: None, nodes }
}

/// Walks a slice of mdast nodes into a `Nodes` collection.
pub fn walk_nodes(mdast_nodes: &[markdown::mdast::Node]) -> Nodes {
    let mut nodes = Vec::new();
    for node in mdast_nodes {
        nodes.extend(walk_node(node));
    }
    nodes.into()
}

/// Walks a single mdast node into zero or more view `Node`s.
pub fn walk_node(node: &markdown::mdast::Node) -> Vec<Node> {
    match node {
        markdown::mdast::Node::Root(r) => walk_nodes(&r.children).into_vec(),
        markdown::mdast::Node::Paragraph(p) => {
            vec![html_element("p", walk_nodes(&p.children))]
        }
        markdown::mdast::Node::Heading(h) => {
            let tag = format!("h{}", h.depth);
            vec![html_element(&tag, walk_nodes(&h.children))]
        }
        markdown::mdast::Node::Text(t) => {
            vec![text_node(&t.value)]
        }
        markdown::mdast::Node::Emphasis(e) => {
            vec![html_element("em", walk_nodes(&e.children))]
        }
        markdown::mdast::Node::Strong(s) => {
            vec![html_element("strong", walk_nodes(&s.children))]
        }
        markdown::mdast::Node::InlineCode(c) => {
            vec![html_element("code", Nodes::from(vec![text_node(&c.value)]))]
        }
        markdown::mdast::Node::Blockquote(b) => {
            vec![html_element("blockquote", walk_nodes(&b.children))]
        }
        markdown::mdast::Node::ThematicBreak(_) => {
            vec![Node::Element(Box::new(void_element("hr")))]
        }
        markdown::mdast::Node::Break(_) => {
            vec![Node::Element(Box::new(void_element("br")))]
        }
        // Default: skip unrecognized nodes for now
        _ => Vec::new(),
    }
}

/// Walks an mdast node directly into a `ViewBuilder`.
///
/// This is the key function for raw HTML passthrough (D-03): `mdast::Html`
/// nodes are written via `str_unescaped()`, while `mdast::Text` nodes
/// go through `text()` for proper escaping.
pub fn walk_to_writer(node: &markdown::mdast::Node, builder: &mut ViewBuilder) {
    match node {
        markdown::mdast::Node::Html(h) => {
            // Raw HTML passthrough — trusted author content, build-time only.
            builder.str_unescaped(&h.value);
        }
        markdown::mdast::Node::Text(t) => {
            // Text content — escaped for HtmlContext::Text.
            builder.text(&t.value);
        }
        _ => {
            // For all other node types, construct view nodes and write them.
            let view_nodes = walk_node(node);
            for vn in view_nodes {
                vn.lower(builder);
            }
        }
    }
}

/// Constructs a normal HTML element with opening and closing tags.
fn html_element(tag: &str, children: Nodes) -> Node {
    let opening_name = make_element_name(tag);
    let opening = OpeningTag {
        lt: parse_quote!(<),
        name: opening_name,
        attributes: Attributes::default(),
        gt: parse_quote!(>),
    };
    let closing_name = make_element_name(tag);
    let closing = ClosingTag {
        lt: parse_quote!(<),
        slash: parse_quote!(/),
        name: closing_name,
        gt: parse_quote!(>),
    };
    Node::Element(Box::new(Element::Normal {
        opening_tag: opening,
        children,
        closing_tag: closing,
    }))
}

/// Constructs an ElementName from a tag name string.
fn make_element_name(tag: &str) -> ElementName {
    ElementName::Ident(HtmlIdent {
        first: syn::Ident::new(tag, Span::call_site()),
        rest: vec![],
    })
}

/// Constructs a void HTML element (no closing tag, no children).
fn void_element(tag: &str) -> Element {
    Element::Void {
        tag: OpeningTag {
            lt: parse_quote!(<),
            name: make_element_name(tag),
            attributes: Attributes::default(),
            gt: parse_quote!(>),
        },
    }
}

/// Constructs a `Node::Text` from a string.
fn text_node(content: &str) -> Node {
    Node::Text(LitStr::new(content, Span::call_site()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use quote::quote;

    fn parse_and_walk(content: &str) -> Nodes {
        let options = get_parse_options();
        let root = markdown::to_mdast(content, &options).unwrap();
        match root {
            markdown::mdast::Node::Root(r) => walk_nodes(&r.children),
            _ => unreachable!(),
        }
    }

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
        let options = get_parse_options();
        let root =
            markdown::to_mdast(r#"<div class="raw">Raw</div>"#, &options).unwrap();
        let mut builder = ViewBuilder::new();
        if let markdown::mdast::Node::Root(r) = root {
            for child in &r.children {
                walk_to_writer(child, &mut builder);
            }
        }
        let tokens = builder.finish().emit_root();
        let token_str = quote! { #tokens }.to_string();
        // The raw HTML should appear verbatim (not escaped as &lt;div&gt;).
        // Token stream string literals escape internal quotes as \".
        assert!(token_str.contains("<div"), "should contain raw <div, got: {token_str}");
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
        let nodes = parse_and_walk("---");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if matches!(e.as_ref(), Element::Void { .. })),
            "expected void element for thematic break",
        );
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
        assert!(nodes.len() >= 1);
    }

    #[test]
    fn mdx_to_view_produces_view() {
        let view = mdx_to_view("# Test");
        assert!(view.cx.is_none());
        assert!(!view.nodes.is_empty());
    }
}
