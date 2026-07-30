//! Proc-macro crate for `topcoat-mdx`.
//!
//! Provides the `compile_mdx!` macro that reads `.mdx` files at compile time,
//! parses them with `markdown-rs`, walks the mdast into `view!` AST nodes,
//! and emits tokens.

#![cfg_attr(docsrs, feature(doc_cfg))]

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use std::path::Path;
use syn::LitStr;

use topcoat_mdx_grammar::parse::get_parse_options;
use topcoat_mdx_grammar::walker::walk_to_writer;
use topcoat_view_grammar::view::ViewWriter;

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
pub fn compile_mdx(tokens: TokenStream) -> TokenStream {
    let lit_str = match syn::parse::<LitStr>(tokens) {
        Ok(l) => l,
        Err(e) => {
            let msg = format!("compile_mdx! expects a string literal path: {e}");
            return syn::Error::new(Span::call_site(), msg).to_compile_error().into();
        }
    };

    let path_str = lit_str.value();

    // Resolve path relative to CARGO_MANIFEST_DIR (T-01-01 mitigation).
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let resolved = Path::new(&manifest_dir).join(&path_str);

    // Security: verify resolved path stays within manifest directory (T-01-01).
    let canonical = match resolved.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("compile_mdx! cannot resolve path '{}': {e}", path_str);
            return syn::Error::new(Span::call_site(), msg).to_compile_error().into();
        }
    };
    let canonical_manifest = std::path::Path::new(&manifest_dir)
        .canonicalize()
        .unwrap_or_else(|_| manifest_dir.into());

    if !canonical.starts_with(&canonical_manifest) {
        let msg = format!(
            "compile_mdx! path '{}' escapes CARGO_MANIFEST_DIR (T-01-01)",
            path_str
        );
        return syn::Error::new(Span::call_site(), msg).to_compile_error().into();
    }

    // Read file contents.
    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("compile_mdx! cannot read '{}': {e}", path_str);
            return syn::Error::new(Span::call_site(), msg).to_compile_error().into();
        }
    };

    // Parse with markdown-rs.
    let options = get_parse_options();
    let root = match markdown::to_mdast(&content, &options) {
        Ok(r) => r,
        Err(e) => {
            // OQ-3: convert parser message to syn::Error (T-01-04 mitigation).
            let msg = format!("compile_mdx! parse error in '{}': {e}", path_str);
            return syn::Error::new(Span::call_site(), msg).to_compile_error().into();
        }
    };

    // Walk mdast into ViewWriter.
    let mut writer = ViewWriter::new();
    if let markdown::mdast::Node::Root(r) = root {
        for child in &r.children {
            walk_to_writer(child, &mut writer);
        }
    }

    // Emit tokens.
    let token_stream = writer.into_token_stream();
    quote! { #token_stream }.into()
}
