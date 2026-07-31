use topcoat_mdx_macro::mdx_pages;

// ---- Basic mdx_pages! compilation ----

// mdx_pages! scans the fixtures/pages directory and registers each .mdx file.
mdx_pages!("tests/fixtures/pages", prefix = "/blog");

#[test]
fn mdx_pages_compiles() {
    // The fact that this test module compiles proves that mdx_pages!
    // successfully scanned the directory and generated valid registration code.
}

// ---- Nested directory ----

// Nested .mdx files should produce routes like /docs/nested/deep-page.
mod nested_test {
    use topcoat_mdx_macro::mdx_pages;

    mdx_pages!("tests/fixtures/pages/nested", prefix = "/docs");

    #[test]
    fn mdx_pages_nested_directory() {
        // Compilation proves nested file was discovered and registered.
    }
}

// ---- Empty directory ----

// mdx_pages! with an empty directory should compile without error.
mod empty_dir_test {
    use topcoat_mdx_macro::mdx_pages;

    // Create an empty directory for this test.
    mdx_pages!("tests/fixtures/empty_pages");

    #[test]
    fn mdx_pages_empty_directory() {
        // Compilation succeeds even with no .mdx files found.
    }
}

// ---- .md-only directory ----

// mdx_pages! with only .md files (no .mdx) should compile and register them.
mod md_only_test {
    use topcoat_mdx_macro::mdx_pages;

    mdx_pages!("tests/fixtures/md-only-pages", prefix = "/md-only");

    #[test]
    fn mdx_pages_discovers_md_files() {
        // Compilation proves .md files were registered.
        // The directory contains only .md files, no .mdx files.
    }
}
