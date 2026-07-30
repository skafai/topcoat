use topcoat::compile_mdx;
use topcoat::context::CxTestBuilder;

#[tokio::test]
async fn tracer_compiles() {
    // Verify the macro expands and compiles without errors.
    let _view = compile_mdx!("tests/fixtures/tracer.mdx");
}

#[tokio::test]
async fn tracer_raw_html_passthrough() {
    // Verify raw HTML passes through unescaped in the rendered output.
    // compile_mdx! emits `async { Ok(view) }.await`, so the result is
    // `Result<View, Error>` — no extra .await needed.
    let view = compile_mdx!("tests/fixtures/tracer.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);

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
