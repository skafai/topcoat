//! Node-specific walkers for mdast node types.
//!
//! Each function handles one mdast node type: links, images, code blocks,
//! lists, tables, strikethrough, etc. They construct `Node` values using
//! the helpers in `helpers.rs` and check for overrides via `jsx.rs`.

use topcoat_view_grammar::{
    attributes::Attributes,
    view::{Node, Nodes},
};

use super::helpers::{
    create_attribute, create_attribute_bool, html_element, normal_element_with_attrs,
    self_closing_element, text_node, void_element_with_attrs, with_attributes,
};
use super::jsx::{build_override_component, try_find_override_path};
use super::WalkContext;

// ---------------------------------------------------------------------------
// Node-specific walkers
// ---------------------------------------------------------------------------

/// Checks if a URL uses a dangerous protocol (XSS mitigation, T-01-01).
/// Blocks `javascript:`, `vbscript:`, and ALL `data:` URIs (including
/// `data:image/svg+xml` which can execute JS via SVG event handlers).
pub(crate) fn is_safe_url(url: &str) -> bool {
    let cleaned: String = url.chars().filter(|c| *c != '\u{0000}').collect();
    let trimmed = cleaned.trim().to_ascii_lowercase();
    !trimmed.starts_with("javascript:")
        && !trimmed.starts_with("vbscript:")
        && !trimmed.starts_with("data:")
}

/// Walks a link node: `<a href="url" title="...">...</a>`.
/// Strips dangerous URL schemes (javascript:, vbscript:, data:)
/// to prevent XSS — renders link text as a `<span>` without href.
pub(crate) fn walk_link(ctx: &WalkContext, link: &markdown::mdast::Link) -> Node {
    if !is_safe_url(&link.url) {
        // Strip the href to prevent XSS; render link text only.
        // T-03.1-01: is_safe_url() check runs BEFORE try_apply_override()
        // so dangerous URLs never route through the override component.
        let children = super::walk_nodes(ctx, &link.children);
        return html_element("span", children);
    }
    let mut attrs = Vec::with_capacity(2);
    attrs.push(create_attribute("href", &link.url));
    if let Some(title) = &link.title {
        attrs.push(create_attribute("title", title));
    }
    let attributes = with_attributes(attrs);
    let children = super::walk_nodes(ctx, &link.children);
    // Check for override AFTER is_safe_url() passes (XSS protection preserved).
    if let Some(path) = try_find_override_path(ctx, "a") {
        return build_override_component(path, &attributes, children, ctx.span);
    }
    Node::Element(Box::new(normal_element_with_attrs(
        "a", attributes, children,
    )))
}

/// Walks an image node: `<img src="url" alt="alt" title="...">`.
/// Strips dangerous URL schemes (javascript:, vbscript:, data:)
/// to prevent XSS — renders alt text only without src.
pub(crate) fn walk_image(ctx: &WalkContext, image: &markdown::mdast::Image) -> Node {
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
    // Check for override before constructing the <img> void element.
    if let Some(path) = try_find_override_path(ctx, "img") {
        return build_override_component(path, &attributes, Nodes::new(), ctx.span);
    }
    Node::Element(Box::new(void_element_with_attrs("img", attributes)))
}

/// Walks a fenced code block: `<pre><code class="language-{lang}">...</code></pre>`.
pub(crate) fn walk_code_block(ctx: &WalkContext, code: &markdown::mdast::Code) -> Node {
    let mut attrs = Vec::new();
    if let Some(ref lang) = code.lang {
        attrs.push(create_attribute("class", &format!("language-{lang}")));
    }
    let code_attrs = with_attributes(attrs);
    let code_children = Nodes::from(vec![text_node(&code.value)]);
    let code_el = normal_element_with_attrs("code", code_attrs, code_children);
    let pre_children = Nodes::from(vec![Node::Element(Box::new(code_el))]);
    // Check for override at the <pre> level (outermost element).
    if let Some(path) = try_find_override_path(ctx, "pre") {
        return build_override_component(path, &Attributes::default(), pre_children, ctx.span);
    }
    html_element("pre", pre_children)
}

/// Walks a list: `<ul>` or `<ol>` with `<li>` children.
pub(crate) fn walk_list(ctx: &WalkContext, list: &markdown::mdast::List) -> Node {
    let tag = if list.ordered { "ol" } else { "ul" };
    let mut children = Vec::new();
    for node in &list.children {
        match node {
            markdown::mdast::Node::ListItem(item) => {
                children.push(walk_list_item(ctx, item));
            }
            other => children.extend(super::walk_node(ctx, other)),
        }
    }
    html_element(tag, Nodes::from(children))
}

/// Walks a list item: `<li>` with optional leading checkbox for task lists.
pub(crate) fn walk_list_item(ctx: &WalkContext, item: &markdown::mdast::ListItem) -> Node {
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
    children.extend(super::walk_nodes(ctx, &item.children).into_vec());
    html_element("li", Nodes::from(children))
}

/// Walks a table: `<table><thead>...</thead><tbody>...</tbody></table>`.
pub(crate) fn walk_table(ctx: &WalkContext, table: &markdown::mdast::Table) -> Node {
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
pub(crate) fn walk_table_cell_inner(
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
    let children = super::walk_nodes(ctx, &cell.children);
    let attributes = with_attributes(attrs);
    Node::Element(Box::new(normal_element_with_attrs(
        tag, attributes, children,
    )))
}

/// Walks a delete (strikethrough) node: `<del>...</del>`.
pub(crate) fn walk_delete(ctx: &WalkContext, delete: &markdown::mdast::Delete) -> Node {
    let children = super::walk_nodes(ctx, &delete.children);
    html_element("del", children)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::parse::get_parse_options;
    use topcoat_view_grammar::view::Element as ViewElement;

    fn parse_and_walk(content: &str) -> Nodes {
        parse_and_walk_ctx(&WalkContext::empty(), content)
    }

    fn parse_and_walk_ctx(ctx: &WalkContext, content: &str) -> Nodes {
        let options = get_parse_options();
        let root = markdown::to_mdast(content, &options).unwrap();
        match root {
            markdown::mdast::Node::Root(r) => super::super::walk_nodes(ctx, &r.children),
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
    fn walks_thematic_break() {
        // Use "***" instead of "---" to avoid ambiguity with frontmatter
        // (which is enabled in parse options). Stars cannot be parsed as
        // frontmatter, making this test unambiguous.
        let nodes = parse_and_walk("\n***\n");
        assert_eq!(nodes.len(), 1);
        let node = &nodes[0];
        assert!(
            matches!(node, Node::Element(e) if matches!(e.as_ref(), ViewElement::Void { .. })),
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
                    matches!(e.as_ref(), ViewElement::Void { .. })
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
    fn is_safe_url_blocks_null_byte_bypass() {
        // Null bytes before the scheme would bypass trim_start().
        assert!(!is_safe_url("\x00javascript:alert(1)"));
        assert!(!is_safe_url("java\x00script:alert(1)"));
        assert!(!is_safe_url("\x00\x00vbscript:msgBox(1)"));
        assert!(!is_safe_url("\x00data:text/html,<script>"));
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
                    matches!(inner.as_ref(), ViewElement::Void { .. })
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
            assert!(!attrs.is_empty(), "image should have attributes (src, alt)");
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

    fn find_element_recursive<'a>(element: &'a ViewElement, tag: &str) -> Option<&'a ViewElement> {
        if element.name().string_name().as_deref() == Some(tag) {
            return Some(element);
        }
        for child in element.children() {
            if let Node::Element(inner) = child
                && let Some(found) = find_element_recursive(inner, tag)
            {
                return Some(found);
            }
        }
        None
    }
}
