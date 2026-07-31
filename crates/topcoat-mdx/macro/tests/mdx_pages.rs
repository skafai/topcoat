use topcoat_mdx_macro::mdx_pages;

// ---- Basic mdx_pages! compilation ----

// mdx_pages! scans the fixtures/pages directory and registers each .mdx file.
mdx_pages!("tests/fixtures/pages", prefix = "/blog");

#[test]
fn mdx_pages_compiles() {
    // The fact that this test module compiles proves that mdx_pages!
    // successfully scanned the directory and generated valid registration code.
    // No runtime assertion needed — the macro emits const _: () = { ... } blocks
    // that are verified at compile time.
}
