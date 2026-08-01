use topcoat_mdx_macro::mdx_pages;

// ---- Index emission test ----

// mdx_pages! emits MDX_INDEX_TESTS_FIXTURES_PAGES const and
// mdx_index_tests_fixtures_pages() accessor function.
// The pages/ fixture contains:
//   - hello-world.mdx (has frontmatter: title, date)
//   - about.mdx (no frontmatter)
//   - MyPost.mdx (no frontmatter)
//   - plain-markdown.md (no frontmatter)
//   - nested/deep-page.mdx (in subdirectory)
mod index_test {
    use topcoat_mdx_macro::mdx_pages;

    mdx_pages!("tests/fixtures/pages", prefix = "/index-test");

    #[test]
    fn mdx_index_emitted() {
        let index = mdx_index_tests_fixtures_pages();
        // We expect at least the top-level files: about, hello-world, MyPost, plain-markdown
        assert!(!index.is_empty(), "MDX_INDEX should not be empty");
    }

    #[test]
    fn mdx_index_entry_has_slug() {
        let index = mdx_index_tests_fixtures_pages();
        let hello = index
            .iter()
            .find(|e| e.slug == "hello-world")
            .expect("hello-world entry should exist");
        // hello-world.mdx has frontmatter with title "Hello World"
        assert_eq!(hello.title, Some("Hello World"));
    }

    #[test]
    fn mdx_index_entry_with_frontmatter() {
        let index = mdx_index_tests_fixtures_pages();
        let hello = index
            .iter()
            .find(|e| e.slug == "hello-world")
            .expect("hello-world entry should exist");
        assert_eq!(hello.date, Some("2024-06-15"));
        // hello-world.mdx has no tags in frontmatter
        assert!(hello.tags.is_empty());
    }

    #[test]
    fn mdx_index_entry_without_frontmatter() {
        let index = mdx_index_tests_fixtures_pages();
        let about = index
            .iter()
            .find(|e| e.slug == "about")
            .expect("about entry should exist");
        assert!(about.title.is_none());
        assert!(about.date.is_none());
        assert!(about.excerpt.is_none());
        assert!(about.tags.is_empty());
    }

    #[test]
    fn mdx_index_kebab_case_slug() {
        let index = mdx_index_tests_fixtures_pages();
        // MyPost.mdx should derive slug "my-post" via kebab-case
        let my_post = index
            .iter()
            .find(|e| e.slug == "my-post")
            .expect("my-post entry should exist for MyPost.mdx");
        assert!(my_post.title.is_none());
    }

    #[test]
    fn mdx_index_md_file_slug() {
        let index = mdx_index_tests_fixtures_pages();
        // plain-markdown.md should derive slug "plain-markdown"
        let plain = index
            .iter()
            .find(|e| e.slug == "plain-markdown")
            .expect("plain-markdown entry should exist");
        assert!(plain.title.is_none());
    }
}

// ---- Index entry type test ----

mod type_test {
    use topcoat::mdx::MdxIndexEntry;

    static TEST_TAGS: &[&'static str] = &["tag1"];

    #[test]
    fn mdx_index_entry_fields() {
        // Verify MdxIndexEntry has the expected fields.
        let entry = MdxIndexEntry {
            slug: "test",
            title: Some("Test Title"),
            date: Some("2024-01-01"),
            excerpt: Some("Test excerpt"),
            tags: TEST_TAGS,
        };
        assert_eq!(entry.slug, "test");
        assert_eq!(entry.title, Some("Test Title"));
        assert_eq!(entry.date, Some("2024-01-01"));
        assert_eq!(entry.excerpt, Some("Test excerpt"));
        assert_eq!(entry.tags, &["tag1"]);
    }

    #[test]
    fn mdx_index_entry_empty_optional_fields() {
        static EMPTY_TAGS: &[&'static str] = &[];
        let entry = MdxIndexEntry {
            slug: "minimal",
            title: None,
            date: None,
            excerpt: None,
            tags: EMPTY_TAGS,
        };
        assert!(entry.title.is_none());
        assert!(entry.date.is_none());
        assert!(entry.excerpt.is_none());
        assert!(entry.tags.is_empty());
    }
}
