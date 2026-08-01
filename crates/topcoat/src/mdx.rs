#![doc = include_str!("../docs/mdx.md")]

#![cfg_attr(docsrs, feature(doc_cfg))]

pub use topcoat_mdx::{
    Frontmatter, MdxComponentMapping, MdxIndexEntry, MdxOverrideMapping, mdx_components,
};
pub use topcoat_mdx_macro::{compile_mdx, mdx_page, mdx_pages};
