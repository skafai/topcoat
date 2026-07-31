//! Proc-macro crate for `topcoat-mdx`.
//!
//! Provides the `compile_mdx!` macro that reads `.mdx` files at compile time,
//! parses them with `markdown-rs`, walks the mdast into `view!` AST nodes,
//! and emits tokens.

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::path::Path;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, LitStr, Path as SynPath, Token};
use topcoat_mdx_grammar::{parse::get_parse_options, walker::walk_to_writer};
use topcoat_view_grammar::view::ViewWriter;

/// A single `Ident => Path` pair in the component registry braced block.
struct CompPair {
    name: Ident,
    path: SynPath,
}

impl Parse for CompPair {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let name: Ident = input.parse()?;
        let _: Token![=>] = input.parse()?;
        let path: SynPath = input.parse()?;
        Ok(Self { name, path })
    }
}

/// Input for `compile_mdx!`: either two-arg (registry + path) or one-arg (path).
enum CompileMdxInput {
    TwoArgs {
        components: Vec<(String, SynPath)>,
        lit_str: LitStr,
    },
    OneArg {
        lit_str: LitStr,
    },
}

/// Parses a braced block of `CompPair`s from a `ParseStream`.
fn parse_component_braces(content: ParseStream) -> syn::Result<Vec<(String, SynPath)>> {
    let pairs = Punctuated::<CompPair, Token![,]>::parse_terminated(content)?;
    Ok(pairs
        .into_iter()
        .map(|p| (p.name.to_string(), p.path))
        .collect())
}

impl Parse for CompileMdxInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        // Pattern 1: { Ident => Path, ... }, "path.mdx" — direct braced block
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            let components = parse_component_braces(&content)?;
            input.parse::<Token![,]>()?;
            let lit_str: LitStr = input.parse()?;
            return Ok(Self::TwoArgs { components, lit_str });
        }

        // Pattern 2: mdx_components!{ Ident => Path, ... }, "path.mdx"
        // — mdx_components! macro_rules! invocation.
        // The proc-macro receives the RAW tokens before mdx_components! is
        // expanded, so we parse the invocation ourselves.
        if input.peek(Ident) {
            let fork = input.fork();
            let maybe_ident: Ident = fork.parse()?;
            if fork.peek(Token![!])
                && fork.peek2(syn::token::Brace)
                && maybe_ident == "mdx_components"
            {
                // Commit the parse from the fork.
                let _macro_name: Ident = input.parse()?;
                let _bang: Token![!] = input.parse()?;
                let content;
                syn::braced!(content in input);
                let components = parse_component_braces(&content)?;
                input.parse::<Token![,]>()?;
                let lit_str: LitStr = input.parse()?;
                return Ok(Self::TwoArgs { components, lit_str });
            }
        }

        // Pattern 3: "path.mdx" — backward compatible one-arg form
        let lit_str: LitStr = input.parse()?;
        Ok(Self::OneArg { lit_str })
    }
}

/// Compiles a `.mdx` file into a Topcoat `view!` AST.
///
/// # Arguments
///
/// * `path` - A string literal pointing to the `.mdx` file, relative to `CARGO_MANIFEST_DIR`.
/// * `components` (optional) - A component registry declared via `mdx_components!{...}`.
///
/// # Examples
///
/// Without component registry (backward-compatible):
///
/// ```ignore
/// #[page("/blog/post")]
/// async fn post_page(cx: Cx) -> impl IntoResponse {
///     view! { cx => compile_mdx!("content/post.mdx") }
/// }
/// ```
///
/// With component registry (recommended):
///
/// ```ignore
/// #[page("/blog/post")]
/// async fn post_page(cx: Cx) -> impl IntoResponse {
///     view! { cx => compile_mdx!(
///         mdx_components! {
///             Callout => components::callout,
///             Divider => components::divider,
///         },
///         "content/post.mdx"
///     ) }
/// }
/// ```
///
/// The `mdx_components!` helper is the documented form, but a bare braced
/// block is also accepted:
///
/// ```ignore
/// compile_mdx!({
///     Callout => components::callout,
/// }, "content/post.mdx")
/// ```
#[proc_macro]
pub fn compile_mdx(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<CompileMdxInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            let msg = format!("compile_mdx! expects a string literal path, optionally preceded by a component registry: {e}");
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };

    let (components, lit_str) = match input {
        CompileMdxInput::TwoArgs { components, lit_str } => (components, lit_str),
        CompileMdxInput::OneArg { lit_str } => (Vec::new(), lit_str),
    };

    let path_str = lit_str.value();

    // Resolve path relative to CARGO_MANIFEST_DIR (T-01-01 mitigation).
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let resolved = Path::new(&manifest_dir).join(&path_str);

    // Security: verify resolved path stays within manifest directory (T-01-01).
    let canonical = match resolved.canonicalize() {
        Ok(p) => p,
        Err(e) => {
            let msg = format!("compile_mdx! cannot resolve path '{path_str}': {e}");
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };
    let canonical_manifest = match std::path::Path::new(&manifest_dir).canonicalize() {
        Ok(p) => p,
        Err(e) => {
            let msg = format!(
                "compile_mdx! cannot canonicalize CARGO_MANIFEST_DIR '{manifest_dir}': {e}"
            );
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };

    if !canonical.starts_with(&canonical_manifest) {
        let msg = format!(
            "compile_mdx! path '{path_str}' escapes CARGO_MANIFEST_DIR (T-01-01)"
        );
        return syn::Error::new(Span::call_site(), msg)
            .to_compile_error()
            .into();
    }

    // Read file contents.
    let content = match std::fs::read_to_string(&canonical) {
        Ok(c) => c,
        Err(e) => {
            let msg = format!("compile_mdx! cannot read '{path_str}': {e}");
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };

    // Parse with markdown-rs.
    let options = get_parse_options();
    let root = match markdown::to_mdast(&content, &options) {
        Ok(r) => r,
        Err(e) => {
            // OQ-3: convert parser message to syn::Error (T-01-04 mitigation).
            let msg = format!("compile_mdx! parse error in '{path_str}': {e}");
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };

    // Build WalkContext with component registry and error buffer.
    let ctx = topcoat_mdx_grammar::walker::WalkContext::new(&components, lit_str.span());

    // Walk mdast into ViewWriter.
    let mut writer = ViewWriter::new();
    if let markdown::mdast::Node::Root(r) = root {
        for child in &r.children {
            walk_to_writer(&ctx, child, &mut writer);
        }
    }

    // Drain walker error buffer into syn::Error diagnostics (D-04, L-02, L-04).
    let errors: Vec<String> = ctx.errors.borrow_mut().drain(..).collect();
    if !errors.is_empty() {
        let mut combined_err = syn::Error::new_spanned(&lit_str, errors[0].clone());
        for err in &errors[1..] {
            combined_err.combine(syn::Error::new_spanned(&lit_str, err.clone()));
        }
        return combined_err.to_compile_error().into();
    }

    // Emit tokens.
    let token_stream = writer.into_token_stream();
    quote! { #token_stream }.into()
}
