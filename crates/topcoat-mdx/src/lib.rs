//! Compiles `.mdx` files at build time into Topcoat `view!` AST nodes.
//!
//! Content authors write `.mdx` files with embedded Topcoat components;
//! the `compile_mdx!` macro reads the file, parses it with `markdown-rs`,
//! walks the mdast into `view!` AST nodes, and emits tokens.
//! Zero runtime parsing overhead.

#![cfg_attr(docsrs, feature(doc_cfg))]

mod frontmatter;

pub use frontmatter::Frontmatter;
pub use topcoat_mdx_macro::compile_mdx;
pub use topcoat_mdx_macro::mdx_page;
pub use topcoat_mdx_macro::mdx_pages;

/// Declares a component registry mapping MDX tag names to Rust component paths.
///
/// Produces a braced block of `Ident => Path` pairs that `compile_mdx!` parses
/// as the component registry argument (D-01).
///
/// ```ignore
/// mdx_components! {
///     Callout => components::callout,
///     Divider => components::divider,
/// }
/// ```
///
/// Trailing commas are supported. Paths may be qualified (e.g.
/// `crate::components::callout`).
#[macro_export]
macro_rules! mdx_components {
    ($($name:ident => $path:path),* $(,)?) => {
        $crate::__mdx_components_block!($($name => $path),*)
    };
}

#[doc(hidden)]
#[macro_export]
macro_rules! __mdx_components_block {
    ($($name:ident => $path:path),* $(,)?) => {
        { $($name => $path),* }
    };
}

// Note: mdx_components! produces { Ident => Path } braced blocks that are
// consumed by compile_mdx! as proc-macro tokens — they are not valid Rust
// expressions. Verification is via cargo check and the macro crate tests.
