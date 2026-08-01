//! Helper functions for constructing view! AST elements and attributes.

use proc_macro2::Span;
use syn::{Ident, LitStr, parse_quote};
use topcoat_view_grammar::{
    attributes::{Attribute, AttributeKey, AttributeNode, AttributeValue, Attributes},
    view::{
        ClosingTag, Element, ElementName, HtmlIdent, Node, Nodes, OpeningTag, SelfClosingTag,
    },
};

// ---------------------------------------------------------------------------
// Helper functions for constructing elements and attributes
// ---------------------------------------------------------------------------

/// Constructs a `Node::Text` from a string.
pub(crate) fn text_node(content: &str) -> Node {
    Node::Text(LitStr::new(content, Span::call_site()))
}

/// Creates an `Ident` that can be a Rust keyword (e.g., "type", "for").
/// `syn::parse_str::<Ident>` uses `Ident::parse`, which rejects keywords.
/// The fallback uses `Ident::new` directly for keyword-safe identifiers.
pub(crate) fn make_ident(name: &str) -> Ident {
    syn::parse_str(name).unwrap_or_else(|_| Ident::new(name, Span::call_site()))
}

/// Constructs an `ElementName` from a tag name string.
pub(crate) fn make_element_name(tag: &str) -> ElementName {
    ElementName::Ident(HtmlIdent {
        first: make_ident(tag),
        rest: vec![],
    })
}

/// Constructs a normal HTML element with opening and closing tags, wrapped in Node.
pub(crate) fn html_element(tag: &str, children: Nodes) -> Node {
    let attributes = Attributes::default();
    Node::Element(Box::new(normal_element_with_attrs(
        tag, attributes, children,
    )))
}

/// Constructs a normal HTML element with custom attributes.
pub(crate) fn normal_element_with_attrs(
    tag: &str,
    attributes: Attributes,
    children: Nodes,
) -> Element {
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
pub(crate) fn void_element(tag: &str) -> Element {
    void_element_with_attrs(tag, Attributes::default())
}

/// Constructs a void HTML element with custom attributes.
pub(crate) fn void_element_with_attrs(tag: &str, attributes: Attributes) -> Element {
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
pub(crate) fn self_closing_element(tag: &str, attributes: Attributes) -> Element {
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
pub(crate) fn create_attribute(key: &str, value: &str) -> Attribute {
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
pub(crate) fn create_attribute_bool(key: &str) -> Attribute {
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
pub(crate) fn with_attributes(attrs: Vec<Attribute>) -> Attributes {
    Attributes {
        cx: None,
        items: attrs.into_iter().map(AttributeNode::Attribute).collect(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}
