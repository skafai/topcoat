//! Proc-macro crate for `topcoat-mdx`.
//!
//! Provides the `compile_mdx!` macro that reads `.mdx` or `.md` files at compile time,
//! parses them with `markdown-rs`, walks the mdast into `view!` AST nodes,
//! and emits tokens. Also provides `mdx_page!` for registering `.mdx` or `.md` files
//! as page routes with frontmatter support.

#![cfg_attr(docsrs, feature(doc_cfg))]

mod compile;
mod convert;
mod input;
mod pages;
#[cfg(test)]
mod tests;

use std::path::Path;

use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::{Ident, LitStr, Path as SynPath};
use topcoat_core_grammar::paths::{
    topcoat_context, topcoat_error, topcoat_inventory, topcoat_mdx, topcoat_router, topcoat_view,
};
use topcoat_mdx_grammar::walker::FrontmatterFormat;

use crate::{
    compile::compile_mdx_file,
    convert::value_to_expr,
    input::{CompileMdxInput, MdxPageInput, MdxPagesInput},
    pages::{build_index, derive_route_path, generate_page_registration, scan_directory},
};

// ---------------------------------------------------------------------------
// compile_mdx! proc-macro
// ---------------------------------------------------------------------------

#[doc = include_str!("../docs/compile_mdx.md")]
#[proc_macro]
pub fn compile_mdx(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<CompileMdxInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            let msg = format!(
                "compile_mdx! expects a string literal path, optionally preceded by a component registry: {e}"
            );
            return syn::Error::new(Span::call_site(), msg)
                .to_compile_error()
                .into();
        }
    };

    let (components, wrapper, overrides, lit_str) = match input {
        CompileMdxInput::TwoArgs {
            components,
            wrapper,
            lit_str,
        } => (
            components,
            wrapper,
            Vec::<(&'static str, SynPath)>::new(),
            lit_str,
        ),
        CompileMdxInput::TwoArgsWithOverrides {
            components,
            overrides,
            wrapper,
            lit_str,
        } => (components, wrapper, overrides, lit_str),
        CompileMdxInput::OneArg { lit_str } => (Vec::new(), None, Vec::new(), lit_str),
    };

    let path_str = lit_str.value();

    let result = match compile_mdx_file(
        &components,
        &overrides,
        wrapper.as_ref(),
        &path_str,
        lit_str.span(),
    ) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

    let view_tokens = &result.view_tokens;

    // Build the final output tokens. When no wrapper is requested, emit exactly
    // what view_tokens contains (the original async { Ok(...) }.await pattern).
    // When a wrapper is requested, wrap the inner view tokens in a Component
    // render call using __cx from the enclosing scope.
    let final_tokens = if result.has_wrapper {
        let wrapper_path = result.wrapper_path.as_ref().unwrap();
        quote! {
            async {
                {
                    use #topcoat_view::Component;
                    let props = #wrapper_path::props_builder().child(#view_tokens).build();
                    Component::render(#wrapper_path::default(), __cx, props).await
                }
            }.await
        }
    } else {
        quote! { #view_tokens }
    };

    // If frontmatter exists, emit it as a const alongside the view tokens.
    // Wrap in a block so the const is scoped and the block evaluates to the view.
    if let Some((content, _format)) = result.frontmatter_content {
        // Derive a unique const name from the file stem (uppercased per Rust conventions).
        let file_stem = Path::new(&path_str)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("MDX")
            .replace('-', "_");
        let const_name = Ident::new(&format!("__MDX_FRONTMATTER_{file_stem}"), lit_str.span());
        let yaml_lit = LitStr::new(&content, lit_str.span());

        quote! {
            {
                const #const_name: &str = #yaml_lit;
                #final_tokens
            }
        }
        .into()
    } else {
        quote! { #final_tokens }.into()
    }
}

// ---------------------------------------------------------------------------
// mdx_page! proc-macro
// ---------------------------------------------------------------------------

#[doc = include_str!("../docs/mdx_page.md")]
#[proc_macro]
#[allow(clippy::too_many_lines, clippy::missing_panics_doc)]
pub fn mdx_page(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<MdxPageInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            return syn::Error::new(
                Span::call_site(),
                format!("mdx_page! expects: route_path, file_path [, frontmatter = Type]: {e}"),
            )
            .to_compile_error()
            .into();
        }
    };

    let route_path = &input.route_path;
    let file_path = &input.file_path;
    let path_str = file_path.value();

    let components: Vec<(String, SynPath)> = input.components.unwrap_or_default();
    let overrides: Vec<(&'static str, SynPath)> = input.overrides.unwrap_or_default();
    let result = match compile_mdx_file(
        &components,
        &overrides,
        input.wrapper.as_ref(),
        &path_str,
        file_path.span(),
    ) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

    let view_tokens = &result.view_tokens;

    // Apply wrapper if requested — emits Component::render() call using `cx`.
    let render_body = if result.has_wrapper {
        let wrapper_path = result.wrapper_path.as_ref().unwrap();
        quote! {
            {
                use #topcoat_view::Component;
                let props = #wrapper_path::props_builder().child(#view_tokens).build();
                Component::render(#wrapper_path::default(), __cx, props).await
            }
        }
    } else {
        quote! { Ok(#view_tokens?) }
    };

    // Generate unique identifiers from file stem.
    let file_stem = Path::new(&path_str)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page")
        .replace('-', "_");
    let render_fn_name = Ident::new(&format!("__mdx_render_{file_stem}"), file_path.span());
    let unit_name = Ident::new(&format!("__mdx_page_{file_stem}"), file_path.span());

    // Frontmatter const + extension insertion.
    let fm_const_and_insert = if let (Some((content, format)), Some(fm_type)) =
        (&result.frontmatter_content, &input.frontmatter_type)
    {
        let fm_const_name = Ident::new(
            &format!("__MDX_PAGE_FRONTMATTER_{file_stem}"),
            file_path.span(),
        );

        // Deserialize frontmatter at compile time into serde_value::Value,
        // dispatching on format (YAML via serde-saphyr, TOML via toml).
        let deserialized: serde_value::Value = if matches!(format, FrontmatterFormat::Yaml) {
            serde_saphyr::from_str(content)
                .unwrap_or_else(|e| panic!("mdx_page! failed to deserialize frontmatter YAML: {e}"))
        } else {
            toml::from_str(content)
                .unwrap_or_else(|e| panic!("mdx_page! failed to deserialize frontmatter TOML: {e}"))
        };

        match value_to_expr(&deserialized, Some(fm_type), file_path.span()) {
            Ok(expr) => {
                // `expr` is already a full struct literal (e.g. `BlogMeta { name, date }`)
                // for Map values since `root_type` was provided.
                quote! {
                    #[allow(clippy::approx_constant)]
                    const #fm_const_name: #fm_type = #expr;
                }
            }
            Err(e) => {
                return e.to_compile_error().into();
            }
        }
    } else {
        quote! {}
    };

    let fm_insert = if result.frontmatter_content.is_some() && input.frontmatter_type.is_some() {
        let fm_const_name = Ident::new(
            &format!("__MDX_PAGE_FRONTMATTER_{file_stem}"),
            file_path.span(),
        );
        quote! {
            #topcoat_router::request::extensions(__cx).insert(#fm_const_name.clone());
        }
    } else {
        quote! {}
    };

    // Emit the page registration.
    quote! {
        #[allow(clippy::needless_question_mark, clippy::approx_constant)]
        const _: () = {
            #fm_const_and_insert

            fn #render_fn_name(
                __cx: &#topcoat_context::Cx,
                body: #topcoat_router::Body,
            ) -> ::std::pin::Pin<
                Box<dyn ::core::future::Future<Output = #topcoat_error::Result<#topcoat_view::View>> + Send + '_>
            > {
                ::std::boxed::Box::pin(async move {
                    #fm_insert
                    #render_body
                })
            }

            #[allow(non_camel_case_types)]
            struct #unit_name;

            const ERASED: #topcoat_router::PageFn = #topcoat_router::PageFn::const_new(
                #topcoat_router::OwnedMethods::One(#topcoat_router::Method::GET),
                ::std::borrow::Cow::Borrowed(#topcoat_router::Path::new(#route_path)),
                #render_fn_name,
            );

            impl ::core::convert::From<#unit_name> for #topcoat_router::PageFn {
                fn from(_: #unit_name) -> Self {
                    ERASED
                }
            }

            #topcoat_inventory::submit!(ERASED);
        };
    }
    .into()
}

// ---------------------------------------------------------------------------
// mdx_pages! proc-macro
// ---------------------------------------------------------------------------

#[doc = include_str!("../docs/mdx_pages.md")]
#[proc_macro]
pub fn mdx_pages(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<MdxPagesInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            return syn::Error::new(
                Span::call_site(),
                format!("mdx_pages! expects: directory_path [, prefix = \"/path\"]: {e}"),
            )
            .to_compile_error()
            .into();
        }
    };

    let dir_str = input.directory_path.value();
    let span = input.directory_path.span();
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let scan_dir = Path::new(&manifest_dir).join(&dir_str);

    // Validate scan directory exists.
    if !scan_dir.is_dir() {
        return syn::Error::new(
            span,
            format!(
                "mdx_pages! directory '{dir_str}' does not exist (resolved: {})",
                scan_dir.display()
            ),
        )
        .to_compile_error()
        .into();
    }

    let canonical_scan_dir = scan_dir.canonicalize().map_err(|e| {
        syn::Error::new(
            span,
            format!("mdx_pages! cannot canonicalize directory '{dir_str}': {e}"),
        )
    });

    let canonical_scan_dir = match canonical_scan_dir {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    let canonical_manifest = std::path::Path::new(&manifest_dir)
        .canonicalize()
        .map_err(|e| {
            syn::Error::new(
                span,
                format!("mdx_pages! cannot canonicalize CARGO_MANIFEST_DIR '{manifest_dir}': {e}"),
            )
        });

    let canonical_manifest = match canonical_manifest {
        Ok(p) => p,
        Err(e) => return e.to_compile_error().into(),
    };

    // Security: verify scan directory stays within manifest directory before
    // enumeration (T-03.1-04). Per-file guards at line ~1106 catch escaping
    // entries, but rejecting the whole directory avoids unnecessary traversal,
    // prevents external file paths from leaking through diagnostics, and
    // matches compile_mdx_file which validates before reading.
    if !canonical_scan_dir.starts_with(&canonical_manifest) {
        return syn::Error::new(
            span,
            format!(
                "mdx_pages! scan directory '{dir_str}' resolves outside CARGO_MANIFEST_DIR (T-03.1-04)"
            ),
        )
        .to_compile_error()
        .into();
    }

    let prefix = input.prefix.as_ref().map(syn::LitStr::value);
    let components: Vec<(String, SynPath)> = input.components.unwrap_or_default();
    let overrides: Vec<(&'static str, SynPath)> = input.overrides.unwrap_or_default();

    // Scan directory for .mdx and .md files.
    let page_entries = scan_directory(&canonical_scan_dir, &canonical_manifest, span);

    // Generate route registrations.
    let route_results: Vec<proc_macro2::TokenStream> = page_entries
        .iter()
        .map(|entry| {
            let route_path =
                derive_route_path(&canonical_scan_dir, &entry.file_path, prefix.as_deref());
            match generate_page_registration(
                &entry.file_path,
                &route_path,
                &components,
                &overrides,
                input.wrapper.as_ref(),
                input.frontmatter_type.as_ref(),
                span,
            ) {
                Ok(ts) => ts,
                Err(e) => e.to_compile_error(),
            }
        })
        .collect();

    // Build index entries from scanned pages.
    let index_entries = build_index(&page_entries, span);

    // Derive a stable identifier from the directory path for the index name.
    let index_suffix = dir_str
        .replace([std::path::MAIN_SEPARATOR, '/', '-'], "_")
        .to_uppercase();

    let index_const_name = Ident::new(&format!("MDX_INDEX_{index_suffix}"), span);
    let index_fn_name = Ident::new(&format!("mdx_index_{}", index_suffix.to_lowercase()), span);

    // Combine route results into a single TokenStream.
    let route_tokens = route_results
        .into_iter()
        .collect::<proc_macro2::TokenStream>();

    // Build index const using the collected entries.
    let index_const_tokens = quote! {
        &[
            #(#index_entries),*
        ]
    };

    quote! {
        #route_tokens

        #[allow(clippy::approx_constant)]
        const #index_const_name: &'static [#topcoat_mdx::MdxIndexEntry] = #index_const_tokens;

        #[allow(clippy::approx_constant)]
        fn #index_fn_name() -> &'static [#topcoat_mdx::MdxIndexEntry] {
            #index_const_name
        }
    }
    .into()
}
