use topcoat::{context::CxTestBuilder, mdx::compile_mdx};

#[tokio::test]
async fn compile_mdx_with_frontmatter_compiles() {
    // compile_mdx! on a frontmatter file emits a YAML const + view tokens.
    let view = compile_mdx!("tests/fixtures/frontmatter_basic.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    // The frontmatter should NOT appear in the rendered HTML.
    assert!(
        !html.contains("---"),
        "frontmatter should not render as content"
    );
    // But the body content should render.
    assert!(html.contains("Hello from MDX"), "body should render");
}

// ---- Backward compatibility: compile_mdx! still works ----

#[tokio::test]
async fn compile_mdx_backward_compat_one_arg() {
    // Verify one-arg compile_mdx! still compiles with plain markdown fixture.
    let view = compile_mdx!("tests/fixtures/tracer.mdx").expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    assert!(html.contains("Tracer Test"), "tracer content should render");
}

// ---- No frontmatter files ----

#[tokio::test]
async fn compile_mdx_without_frontmatter() {
    // Verify compile_mdx! on a no-frontmatter file compiles (no YAML const emitted).
    let view = compile_mdx!("tests/fixtures/frontmatter_empty.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    assert!(
        html.contains("No Frontmatter"),
        "plain content should render"
    );
}

// ---- Complex frontmatter ----

#[tokio::test]
async fn compile_mdx_complex_frontmatter() {
    // Verify compile_mdx! on a complex frontmatter file compiles.
    let view = compile_mdx!("tests/fixtures/frontmatter_complex.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    assert!(
        html.contains("Complex Frontmatter Test"),
        "body should render"
    );
    assert!(
        !html.contains("---"),
        "frontmatter should not render as content"
    );
}

// ---- .md file with frontmatter ----

#[tokio::test]
async fn compile_mdx_handles_md_extension_with_frontmatter() {
    let view =
        compile_mdx!("tests/fixtures/frontmatter_md.md").expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    assert!(html.contains("<h1 "), "body should render");
    assert!(
        !html.contains("---"),
        "frontmatter should not render as content"
    );
}
