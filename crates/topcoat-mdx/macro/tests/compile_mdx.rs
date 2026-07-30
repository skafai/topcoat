use topcoat::compile_mdx;
use topcoat::context::CxTestBuilder;

// ---- Tracer fixture (from Plan 01) ----

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

// ---- CommonMark fixture ----

#[tokio::test]
async fn commonmark_compiles() {
    // Verify the macro expands and compiles without errors.
    let _view = compile_mdx!("tests/fixtures/commonmark.mdx");
}

#[tokio::test]
async fn commonmark_renders() {
    let view = compile_mdx!("tests/fixtures/commonmark.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);

    // Heading levels 1-6
    assert!(html.contains("<h1>"), "should have <h1>. Got:\n{html}");
    assert!(html.contains("<h2>"), "should have <h2>");
    assert!(html.contains("<h3>"), "should have <h3>");
    assert!(html.contains("<h4>"), "should have <h4>");
    assert!(html.contains("<h5>"), "should have <h5>");
    assert!(html.contains("<h6>"), "should have <h6>");

    // Paragraph
    assert!(html.contains("<p>"), "should have <p>");

    // Inline formatting
    assert!(html.contains("<strong>"), "should have <strong>");
    assert!(html.contains("<em>"), "should have <em>");

    // Link with href attribute
    assert!(
        html.contains("<a href="),
        "should have <a href=>. Got:\n{html}"
    );

    // Image with src and alt attributes
    assert!(
        html.contains("<img src="),
        "should have <img src=>. Got:\n{html}"
    );
    assert!(
        html.contains("alt="),
        "should have alt= attribute. Got:\n{html}"
    );

    // Code block: <pre><code
    assert!(
        html.contains("<pre><code"),
        "should have <pre><code. Got:\n{html}"
    );

    // Blockquote
    assert!(
        html.contains("<blockquote>"),
        "should have <blockquote>. Got:\n{html}"
    );

    // Lists
    assert!(html.contains("<ul>"), "should have <ul>");
    assert!(html.contains("<li>"), "should have <li>");
    assert!(html.contains("<ol>"), "should have <ol>");

    // Thematic break
    assert!(html.contains("<hr>"), "should have <hr>");

    // Hard break
    assert!(html.contains("<br>"), "should have <br>");

    // Inline code
    assert!(html.contains("<code>"), "should have inline <code>");
}

// ---- GFM fixture ----

#[tokio::test]
async fn gfm_compiles() {
    let _view = compile_mdx!("tests/fixtures/gfm.mdx");
}

#[tokio::test]
async fn gfm_renders() {
    let view = compile_mdx!("tests/fixtures/gfm.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);

    // Table structure
    assert!(
        html.contains("<table>"),
        "should have <table>. Got:\n{html}"
    );
    assert!(html.contains("<thead>"), "should have <thead>");
    assert!(html.contains("<tbody>"), "should have <tbody>");
    assert!(html.contains("<tr>"), "should have <tr>");
    assert!(html.contains("<th>"), "should have <th>");
    assert!(html.contains("<td>"), "should have <td>");

    // Table alignment (style="text-align: ...")
    assert!(
        html.contains("text-align:"),
        "should have text-align: style. Got:\n{html}"
    );

    // Strikethrough
    assert!(
        html.contains("<del>"),
        "should have <del> for strikethrough. Got:\n{html}"
    );

    // Task list: checkbox input
    assert!(
        html.contains(r#"type="checkbox""#),
        "should have type=\"checkbox\". Got:\n{html}"
    );
}

// ---- Raw HTML fixture ----

#[tokio::test]
async fn raw_html_compiles() {
    let _view = compile_mdx!("tests/fixtures/raw_html.mdx");
}

#[tokio::test]
async fn raw_html_renders_unescaped() {
    let view = compile_mdx!("tests/fixtures/raw_html.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);

    // Raw HTML block should appear verbatim (MDX-05).
    assert!(
        html.contains(r#"<div class="test">Raw HTML block</div>"#),
        "raw HTML block should pass through unescaped. Got:\n{html}"
    );

    // Another raw HTML block (table) should appear verbatim.
    assert!(
        html.contains(r#"<table class="raw-table">"#),
        "raw HTML table should pass through unescaped. Got:\n{html}"
    );

    // Verify NOT escaped as &lt;div&gt; or similar.
    assert!(
        !html.contains("&lt;div"),
        "raw HTML should NOT be escaped as &lt;div&gt;. Got:\n{html}"
    );
    assert!(
        !html.contains("&lt;table"),
        "raw HTML should NOT be escaped as &lt;table&gt;. Got:\n{html}"
    );
}
