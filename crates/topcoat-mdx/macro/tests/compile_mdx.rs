use topcoat::{compile_mdx, context::CxTestBuilder};

// ---- Tracer fixture (from Plan 01) ----

#[tokio::test]
async fn tracer_compiles() {
    // Verify the macro expands and compiles without errors.
    let _view = compile_mdx!("tests/fixtures/tracer.mdx");
}

#[tokio::test]
async fn tracer_renders() {
    // Combined test: verifies the tracer fixture compiles, renders raw HTML
    // unescaped, and includes mixed content (markdown + raw HTML).
    // (Consolidated from tracer_compiles + tracer_raw_html_passthrough +
    // partially overlapping raw_html_renders_unescaped per IN-01.)
    let view = compile_mdx!("tests/fixtures/tracer.mdx").expect("view should render successfully");
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
    let view =
        compile_mdx!("tests/fixtures/commonmark.mdx").expect("view should render successfully");
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

    // Link with correct href value (WR-05: value-level assertion)
    assert!(
        html.contains(r#"href="https://example.com""#),
        "should have correct href value. Got:\n{html}"
    );

    // Image with src and alt attribute values (WR-05: value-level assertion)
    assert!(
        html.contains(r#"src="photo.png""#),
        "should have correct image src value. Got:\n{html}"
    );
    assert!(
        html.contains(r#"alt="Image alt""#),
        "should have correct alt value. Got:\n{html}"
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
    let view = compile_mdx!("tests/fixtures/gfm.mdx").expect("view should render successfully");
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

    // Table alignment values (WR-05: value-level assertions)
    assert!(
        html.contains("text-align: left"),
        "should have left alignment. Got:\n{html}"
    );
    assert!(
        html.contains("text-align: right"),
        "should have right alignment. Got:\n{html}"
    );
    assert!(
        html.contains("text-align: center"),
        "should have center alignment. Got:\n{html}"
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
    let view =
        compile_mdx!("tests/fixtures/raw_html.mdx").expect("view should render successfully");
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
