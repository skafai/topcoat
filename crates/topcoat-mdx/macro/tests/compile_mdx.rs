use topcoat::compile_mdx;

#[test]
fn tracer_compiles() {
    // Verify the macro expands and compiles without errors.
    let _view = compile_mdx!("fixtures/tracer.mdx");
}

#[tokio::test]
async fn tracer_raw_html_passthrough() {
    // Verify raw HTML passes through unescaped in the rendered output.
    let view_result = compile_mdx!("fixtures/tracer.mdx").await;
    let view = view_result.expect("view should render successfully");
    let html = view.render().to_string();

    // The raw HTML should appear verbatim, not double-escaped.
    assert!(
        html.contains(r#"<div class="raw">Raw HTML</div>"#),
        "raw HTML should pass through unescaped. Got:\n{html}",
    );
    // Verify it's NOT escaped as &lt;div&gt;.
    assert!(
        !html.contains("&lt;div"),
        "raw HTML should NOT be escaped. Got:\n{html}",
    );
}
