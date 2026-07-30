//! Proc-macro crate for `topcoat-mdx`.
//!
//! Provides the `compile_mdx!` macro that reads `.mdx` files at compile time,
//! parses them with `markdown-rs`, walks the mdast into `view!` AST nodes,
//! and emits tokens.

#![cfg_attr(docsrs, feature(doc_cfg))]

use proc_macro::TokenStream;

/// Compiles a `.mdx` file into a Topcoat `view!` AST.
///
/// # Arguments
///
/// * `path` - A string literal pointing to the `.mdx` file, relative to
///   `CARGO_MANIFEST_DIR`.
///
/// # Example
///
/// ```ignore
/// #[page("/blog/post")]
/// async fn post_page(cx: Cx) -> impl IntoResponse {
///     view! { cx => compile_mdx!("content/post.mdx") }
/// }
/// ```
#[proc_macro]
pub fn compile_mdx(_tokens: TokenStream) -> TokenStream {
    // Stub implementation — replaced in plan 01-01 task 3.
    TokenStream::new()
}
