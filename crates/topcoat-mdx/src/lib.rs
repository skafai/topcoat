//! Compiles `.mdx` files at build time into Topcoat `view!` AST nodes.
//!
//! Content authors write `.mdx` files with embedded Topcoat components;
//! the `compile_mdx!` macro reads the file, parses it with `markdown-rs`,
//! walks the mdast into `view!` AST nodes, and emits tokens.
//! Zero runtime parsing overhead.

#![cfg_attr(docsrs, feature(doc_cfg))]

extern crate self as topcoat_mdx;

#[cfg(feature = "macro")]
pub use topcoat_mdx_macro::compile_mdx;
