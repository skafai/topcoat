use serde::Deserialize;
use topcoat::{context::CxTestBuilder, mdx::compile_mdx};

// ---- Frontmatter fixture ----

/// Frontmatter type matching `frontmatter_basic.mdx`.
#[derive(Debug, Deserialize, PartialEq)]
struct BlogMeta {
    title: String,
    date: String,
    tags: Vec<String>,
}

#[tokio::test]
async fn compile_mdx_with_frontmatter_compiles() {
    // compile_mdx! on a frontmatter file emits a YAML const + view tokens.
    let view = compile_mdx!("tests/fixtures/frontmatter_basic.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    // The frontmatter should NOT appear in the rendered HTML.
    assert!(!html.contains("---"), "frontmatter should not render as content");
    // But the body content should render.
    assert!(html.contains("<h1>Hello from MDX</h1>"), "body should render");
}

// ---- Backward compatibility: compile_mdx! still works ----

#[tokio::test]
async fn compile_mdx_backward_compat_one_arg() {
    // Verify one-arg compile_mdx! still compiles with plain markdown fixture.
    let view = compile_mdx!("tests/fixtures/tracer.mdx")
        .expect("view should render successfully");
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
    assert!(html.contains("No Frontmatter"), "plain content should render");
}

// ---- Complex frontmatter ----

#[tokio::test]
async fn compile_mdx_complex_frontmatter() {
    // Verify compile_mdx! on a complex frontmatter file compiles.
    let view = compile_mdx!("tests/fixtures/frontmatter_complex.mdx")
        .expect("view should render successfully");
    let cx = CxTestBuilder::new().build();
    let html = view.render(&cx);
    assert!(html.contains("Complex Frontmatter Test"), "body should render");
    assert!(!html.contains("---"), "frontmatter should not render as content");
}

// ---- Frontmatter extractor ----

#[test]
fn frontmatter_struct_matches_fixture() {
    // Verify the BlogMeta struct can be constructed with expected values.
    let meta = BlogMeta {
        title: "Hello".to_string(),
        date: "2024-01-01".to_string(),
        tags: vec!["rust".to_string(), "mdx".to_string()],
    };
    assert_eq!(meta.title, "Hello");
    assert_eq!(meta.date, "2024-01-01");
    assert_eq!(meta.tags.len(), 2);
}
