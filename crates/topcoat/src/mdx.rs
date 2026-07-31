//! MDX compilation and page routing.
//!
//! Re-exports from `topcoat-mdx` and `topcoat-mdx-macro`: `compile_mdx!`,
//! `mdx_page!`, `mdx_pages!`, `mdx_components!`, and the `Frontmatter<T>` extractor.

#![cfg_attr(docsrs, feature(doc_cfg))]

pub use topcoat_mdx::Frontmatter;
pub use topcoat_mdx::mdx_components;
pub use topcoat_mdx_macro::compile_mdx;
pub use topcoat_mdx_macro::mdx_page;
pub use topcoat_mdx_macro::mdx_pages;
