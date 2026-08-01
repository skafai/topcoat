#![doc = include_str!("../docs/module.md")]
#![cfg_attr(docsrs, feature(doc_cfg))]

mod frontmatter;

pub use frontmatter::Frontmatter;
pub use topcoat_mdx_macro::{compile_mdx, mdx_page, mdx_pages};

/// A component registration entry for the MDX component inventory.
///
/// Submitted by `mdx_components!` when the `discover` feature is enabled,
/// allowing runtime discovery of component registrations.
#[derive(Debug, Clone)]
pub struct MdxComponentMapping {
    /// The MDX tag name (e.g., `"Callout"`, `"Divider"`).
    pub tag: &'static str,
    /// The Rust component path as a string (e.g., `"components::callout"`).
    /// Stored as `&'static str` because `syn::Path` is not `Send`.
    pub path: &'static str,
}

/// An index entry for a single `.mdx` or `.md` page discovered by `mdx_pages!`.
///
/// Used to build structured indexes (blog listings, sitemaps, tag pages) from
/// MDX frontmatter and file path metadata at compile time.
#[derive(Debug, Clone)]
pub struct MdxIndexEntry {
    /// The kebab-cased route slug derived from the file path stem.
    pub slug: &'static str,
    /// The `title` field from frontmatter, if present.
    pub title: Option<&'static str>,
    /// The `date` field from frontmatter, if present.
    pub date: Option<&'static str>,
    /// The `excerpt` field from frontmatter, if present.
    pub excerpt: Option<&'static str>,
    /// The `tags` field from frontmatter as a slice of strings, empty if absent.
    pub tags: &'static [&'static str],
}

/// An HTML element override entry for the MDX override inventory.
///
/// Submitted by override declarations when the `discover` feature is enabled,
/// allowing runtime discovery of element-to-component substitutions.
#[derive(Debug, Clone)]
pub struct MdxOverrideMapping {
    /// The HTML tag name (e.g., `"a"`, `"h1"`, `"pre"`).
    pub tag: &'static str,
    /// The Rust component path as a string.
    pub path: &'static str,
}

#[cfg(feature = "discover")]
inventory::collect!(MdxComponentMapping);

#[cfg(feature = "discover")]
inventory::collect!(MdxOverrideMapping);

#[doc = include_str!("../macro/docs/mdx_components.md")]
#[macro_export]
macro_rules! mdx_components {
    ($($name:ident => $path:path),* $(,)?) => {{
        // Submit each mapping to the global inventory when discover is enabled.
        $crate::__mdx_submit_components!($($name => $path),*);

        // Produce the braced block for explicit use.
        $crate::__mdx_components_block!($($name => $path),*)
    }};
}

#[doc(hidden)]
#[macro_export]
macro_rules! __mdx_submit_components {
    () => {};
    ($($name:ident => $path:path),* $(,)?) => {
        $(
            #[cfg(feature = "discover")]
            $crate::internal::inventory::submit! {
                $crate::MdxComponentMapping {
                    tag: stringify!($name),
                    path: stringify!($path),
                }
            }
        )*
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __mdx_components_block {
    ($($name:ident => $path:path),* $(,)?) => {
        { $($name => $path),* }
    };
}
