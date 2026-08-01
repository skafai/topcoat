#![doc = include_str!("../docs/module.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use topcoat_mdx_macro::{compile_mdx, mdx_page, mdx_pages};

/// An index entry for a single `.mdx` or `.md` page discovered by `mdx_pages!`.
///
/// Used to build structured indexes (blog listings, sitemaps, tag pages) from
/// MDX frontmatter and file path metadata at compile time.
#[derive(Debug, Clone)]
pub struct MdxIndexEntry {
    /// The kebab-cased route slug derived from the file path stem.
    pub slug: &'static str,
    /// The full route path including any prefix and subdirectory structure
    /// (e.g. `"/blog/updates/roadmap"`). Use this for generating links.
    pub path: &'static str,
    /// The `title` field from frontmatter, if present.
    pub title: Option<&'static str>,
    /// The `date` field from frontmatter, if present.
    pub date: Option<&'static str>,
    /// The `excerpt` field from frontmatter, if present.
    pub excerpt: Option<&'static str>,
    /// The `tags` field from frontmatter as a slice of strings, empty if absent.
    pub tags: &'static [&'static str],
}

#[doc = include_str!("../macro/docs/mdx_components.md")]
#[macro_export]
macro_rules! mdx_components {
    ($($name:ident => $path:path),* $(,)?) => {
        { $($name => $path),* }
    };
}
