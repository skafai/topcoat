#![doc = include_str!("../docs/module.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

pub use topcoat_mdx_macro::{compile_mdx, mdx_page, mdx_pages};

/// An index entry for a single `.mdx` or `.md` page discovered by `mdx_pages!`.
///
/// Used to build structured indexes (blog listings, sitemaps, tag pages) from
/// MDX frontmatter and file path metadata at compile time.
///
/// The named fields cover the frontmatter every page tends to carry. Pages
/// that declare more reach the consumer through [`frontmatter_raw`], which
/// holds the whole block for deserializing into a type of your own.
///
/// Entries are built by `mdx_pages!`. Constructing one by hand is possible but
/// not the intended use, so a new field is a breaking change for any code that
/// does.
///
/// [`frontmatter_raw`]: Self::frontmatter_raw
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
    /// The whole frontmatter block, with the `---` or `+++` delimiters already
    /// stripped. Empty when the page carries no frontmatter.
    ///
    /// Deserialize it into your own type to read fields beyond the named ones,
    /// picking the deserializer from [`frontmatter_format`]:
    ///
    /// ```no_run
    /// # use topcoat_mdx::{MdxFrontmatterFormat, MdxIndexEntry};
    /// # #[derive(serde::Deserialize)]
    /// # struct PostMeta { subtitle: String }
    /// # fn parse(entry: &MdxIndexEntry) -> Option<PostMeta> {
    /// match entry.frontmatter_format {
    ///     MdxFrontmatterFormat::Yaml => serde_saphyr::from_str(entry.frontmatter_raw).ok(),
    ///     MdxFrontmatterFormat::Toml => toml::from_str(entry.frontmatter_raw).ok(),
    ///     MdxFrontmatterFormat::None => None,
    /// }
    /// # }
    /// ```
    ///
    /// [`frontmatter_format`]: Self::frontmatter_format
    pub frontmatter_raw: &'static str,
    /// The syntax [`frontmatter_raw`] is written in.
    ///
    /// The delimiters are stripped during parsing, so the syntax cannot be
    /// recovered from the string alone. Read this instead of guessing.
    ///
    /// [`frontmatter_raw`]: Self::frontmatter_raw
    pub frontmatter_format: MdxFrontmatterFormat,
    /// Whitespace-separated words in the page body, counted when the page was
    /// compiled and excluding the frontmatter block.
    ///
    /// Code blocks and component markup count toward the total, matching what
    /// reading-time tooling reports for a markdown file. Turn it into an
    /// estimate with a rate of your choosing:
    ///
    /// ```
    /// # let word_count = 400_usize;
    /// let minutes = word_count.div_ceil(200);
    /// # assert_eq!(minutes, 2);
    /// ```
    pub word_count: usize,
}

/// The syntax a page's frontmatter block is written in.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MdxFrontmatterFormat {
    /// The page carries no frontmatter.
    None,
    /// YAML frontmatter, written between `---` delimiters.
    Yaml,
    /// TOML frontmatter, written between `+++` delimiters.
    Toml,
}

#[doc = include_str!("../macro/docs/mdx_components.md")]
#[macro_export]
macro_rules! mdx_components {
    ($($name:ident => $path:path),* $(,)?) => {
        { $($name => $path),* }
    };
}
