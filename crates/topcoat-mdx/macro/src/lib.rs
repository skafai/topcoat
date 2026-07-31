//! Proc-macro crate for `topcoat-mdx`.
//!
//! Provides the `compile_mdx!` macro that reads `.mdx` or `.md` files at compile time,
//! parses them with `markdown-rs`, walks the mdast into `view!` AST nodes,
//! and emits tokens. Also provides `mdx_page!` for registering `.mdx` or `.md` files
//! as page routes with frontmatter support.

#![cfg_attr(docsrs, feature(doc_cfg))]

use std::path::Path;

use heck::ToKebabCase;
use proc_macro::TokenStream;
use proc_macro2::Span;
use quote::quote;
use syn::parse::{Parse, ParseStream};
use syn::punctuated::Punctuated;
use syn::{Ident, LitStr, Path as SynPath, Token};
use topcoat_mdx_grammar::{
    parse::get_parse_options,
    walker::{extract_frontmatter, walk_to_writer},
};
use topcoat_view_grammar::view::ViewWriter;
use topcoat_core_grammar::paths::{
    topcoat_context, topcoat_error, topcoat_inventory, topcoat_router, topcoat_view,
};

// ---------------------------------------------------------------------------
// compile_mdx! input parsing
// ---------------------------------------------------------------------------

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
        if input.peek(Ident) {
            let fork = input.fork();
            let maybe_ident: Ident = fork.parse()?;
            if fork.peek(Token![!])
                && fork.peek2(syn::token::Brace)
                && maybe_ident == "mdx_components"
            {
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

// ---------------------------------------------------------------------------
// mdx_page! input parsing
// ---------------------------------------------------------------------------

/// Input for `mdx_page!`: (`route_path`, `file_path`, [frontmatter = Type])
struct MdxPageInput {
    route_path: LitStr,
    file_path: LitStr,
    frontmatter_type: Option<syn::Type>,
}

impl Parse for MdxPageInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let route_path: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let file_path: LitStr = input.parse()?;

        let mut frontmatter_type = None;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            // Parse `frontmatter = Type`
            let kw: Ident = input.parse()?;
            if kw != "frontmatter" {
                return Err(syn::Error::new(
                    kw.span(),
                    "expected `frontmatter = Type`, found something else",
                ));
            }
            input.parse::<Token![=]>()?;
            frontmatter_type = Some(input.parse()?);
        }

        Ok(Self {
            route_path,
            file_path,
            frontmatter_type,
        })
    }
}

// ---------------------------------------------------------------------------
// mdx_pages! input parsing
// ---------------------------------------------------------------------------

/// Input for `mdx_pages!`: (`directory_path`, prefix = "/optional/prefix")
struct MdxPagesInput {
    directory_path: LitStr,
    prefix: Option<LitStr>,
}

impl Parse for MdxPagesInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let directory_path: LitStr = input.parse()?;

        let mut prefix = None;
        if input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            // Parse `prefix = "/some/path"`
            let kw: Ident = input.parse()?;
            if kw != "prefix" {
                return Err(syn::Error::new(
                    kw.span(),
                    "expected `prefix = \"/path\"`, found something else",
                ));
            }
            input.parse::<Token![=]>()?;
            prefix = Some(input.parse()?);
        }

        Ok(Self {
            directory_path,
            prefix,
        })
    }
}

// ---------------------------------------------------------------------------
// Common compile logic shared by compile_mdx! and mdx_page!
// ---------------------------------------------------------------------------

/// Result of compiling an MDX file: frontmatter YAML (if any) and view tokens.
struct CompiledMdxResult {
    frontmatter_yaml: Option<String>,
    view_tokens: proc_macro2::TokenStream,
}

/// Shared inner logic: parse markdown content, extract frontmatter, walk mdast.
///
/// Used by both `compile_mdx_file` (compile_mdx!, mdx_page!) and
/// `generate_page_registration` (mdx_pages!). The `label` parameter controls
/// the prefix in error messages.
fn parse_and_walk_mdx(
    components: &[(String, SynPath)],
    content: &str,
    label: &str,
    span: Span,
) -> Result<CompiledMdxResult, syn::Error> {
    // Parse with markdown-rs.
    let options = get_parse_options();
    let root = markdown::to_mdast(content, &options).map_err(|e| {
        syn::Error::new(span, format!("{label} parse error: {e}"))
    })?;

    // Extract frontmatter from root node.
    let frontmatter_yaml = extract_frontmatter(&root);

    // Build WalkContext with component registry and error buffer.
    let ctx = topcoat_mdx_grammar::walker::WalkContext::new(components, span);

    // Walk mdast into ViewWriter, skipping the frontmatter node.
    let mut writer = ViewWriter::new();
    if let markdown::mdast::Node::Root(r) = root {
        let start_idx = usize::from(frontmatter_yaml.is_some());
        for child in r.children.iter().skip(start_idx) {
            walk_to_writer(&ctx, child, &mut writer);
        }
    }

    // Drain walker error buffer into syn::Error diagnostics.
    let errors: Vec<String> = ctx.errors.borrow_mut().drain(..).collect();
    if !errors.is_empty() {
        let mut combined_err = syn::Error::new(span, errors[0].clone());
        for err in &errors[1..] {
            combined_err.combine(syn::Error::new(span, err.clone()));
        }
        return Err(combined_err);
    }

    Ok(CompiledMdxResult {
        frontmatter_yaml,
        view_tokens: writer.into_token_stream(),
    })
}

/// Shared logic: resolve path, read file, parse, extract frontmatter, walk.
fn compile_mdx_file(
    components: &[(String, SynPath)],
    path_str: &str,
    span: Span,
) -> Result<CompiledMdxResult, syn::Error> {
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let resolved = Path::new(&manifest_dir).join(path_str);

    // Security: verify resolved path stays within manifest directory (T-01-01).
    let canonical = resolved.canonicalize().map_err(|e| {
        syn::Error::new(
            span,
            format!("compile_mdx! cannot resolve path '{path_str}': {e}"),
        )
    })?;
    let canonical_manifest = std::path::Path::new(&manifest_dir)
        .canonicalize()
        .map_err(|e| {
            syn::Error::new(
                span,
                format!(
                    "compile_mdx! cannot canonicalize CARGO_MANIFEST_DIR '{manifest_dir}': {e}"
                ),
            )
        })?;

    if !canonical.starts_with(&canonical_manifest) {
        return Err(syn::Error::new(
            span,
            format!("compile_mdx! path '{path_str}' escapes CARGO_MANIFEST_DIR (T-01-01)"),
        ));
    }

    let content = std::fs::read_to_string(&canonical).map_err(|e| {
        syn::Error::new(span, format!("compile_mdx! cannot read '{path_str}': {e}"))
    })?;

    parse_and_walk_mdx(components, &content, "compile_mdx!", span)
}

// ---------------------------------------------------------------------------
// compile_mdx! proc-macro
// ---------------------------------------------------------------------------

/// Compiles a `.mdx` or `.md` file into a Topcoat `view!` AST.
///
/// # Arguments
///
/// * `path` - A string literal pointing to the `.mdx` or `.md` file, relative to `CARGO_MANIFEST_DIR`.
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

    let result = match compile_mdx_file(&components, &path_str, lit_str.span()) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

    let view_tokens = &result.view_tokens;

    // If frontmatter exists, emit it as a const alongside the view tokens.
    // Wrap in a block so the const is scoped and the block evaluates to the view.
    if let Some(yaml) = result.frontmatter_yaml {
        // Derive a unique const name from the file stem (uppercased per Rust conventions).
        let file_stem = Path::new(&path_str)
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("MDX");
        let const_name = Ident::new(
            &format!("__MDX_FRONTMATTER_{}", file_stem.to_uppercase()),
            lit_str.span(),
        );
        let yaml_lit = LitStr::new(&yaml, lit_str.span());

        quote! {
            {
                const #const_name: &str = #yaml_lit;
                #view_tokens
            }
        }
        .into()
    } else {
        quote! { #view_tokens }.into()
    }
}

// ---------------------------------------------------------------------------
// mdx_page! proc-macro
// ---------------------------------------------------------------------------

/// Registers a `.mdx` or `.md` file as a page route with optional frontmatter.
///
/// # Arguments
///
/// * `route_path` - The URL path for this page (e.g. `"/blog/hello"`).
/// * `file_path` - Path to the `.mdx` or `.md` file, relative to `CARGO_MANIFEST_DIR`.
/// * `frontmatter = Type` (optional) - The Rust type to deserialize the YAML or TOML frontmatter into.
///
/// # Examples
///
/// ```ignore
/// use serde::Deserialize;
///
/// #[derive(Deserialize)]
/// struct BlogMeta {
///     title: String,
///     date: String,
/// }
///
/// // Register with frontmatter:
/// mdx_page!("/blog/hello", "content/hello.mdx", frontmatter = BlogMeta);
///
/// // Register without frontmatter:
/// mdx_page!("/about", "content/about.mdx");
/// ```
#[proc_macro]
pub fn mdx_page(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<MdxPageInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            return syn::Error::new(
                Span::call_site(),
                format!(
                    "mdx_page! expects: route_path, file_path [, frontmatter = Type]: {e}"
                ),
            )
            .to_compile_error()
            .into();
        }
    };

    let route_path = &input.route_path;
    let file_path = &input.file_path;
    let path_str = file_path.value();

    let result = match compile_mdx_file(&[], &path_str, file_path.span()) {
        Ok(r) => r,
        Err(e) => return e.to_compile_error().into(),
    };

    let view_tokens = &result.view_tokens;

    // Generate unique identifiers from file stem.
    let file_stem = Path::new(&path_str)
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page");
    let render_fn_name = Ident::new(&format!("__mdx_render_{file_stem}"), file_path.span());
    let unit_name = Ident::new(&format!("__mdx_page_{file_stem}"), file_path.span());

    // Frontmatter const + extension insertion.
    let fm_const_and_insert = if let (Some(yaml), Some(fm_type)) =
        (&result.frontmatter_yaml, &input.frontmatter_type)
    {
        let fm_const_name = Ident::new(
            &format!("__MDX_PAGE_FRONTMATTER_{file_stem}"),
            file_path.span(),
        );

        // Deserialize YAML at compile time into serde_value::Value, then
        // convert to a syn::Expr of the target type.
        let deserialized = match serde_saphyr::from_str::<serde_value::Value>(yaml) {
            Ok(v) => v,
            Err(e) => {
                return syn::Error::new(
                    file_path.span(),
                    format!("mdx_page! failed to deserialize frontmatter YAML: {e}"),
                )
                .to_compile_error()
                .into();
            }
        };

        match value_to_expr(&deserialized, file_path.span()) {
            Ok(expr) => {
                // `expr` is a brace block `{ field: val, ... }` for Map values.
                // Prefix with the type name to form a valid struct literal.
                quote! {
                    const #fm_const_name: #fm_type = #fm_type #expr;
                }
            }
            Err(e) => {
                return e.to_compile_error().into();
            }
        }
    } else {
        quote! {}
    };

    let fm_insert = if result.frontmatter_yaml.is_some()
        && input.frontmatter_type.is_some()
    {
        let fm_const_name = Ident::new(
            &format!("__MDX_PAGE_FRONTMATTER_{file_stem}"),
            file_path.span(),
        );
        quote! {
            #topcoat_router::request::extensions(cx).insert(#fm_const_name.clone());
        }
    } else {
        quote! {}
    };

    // Emit the page registration.
    quote! {
        const _: () = {
            #fm_const_and_insert

            fn #render_fn_name(
                cx: &#topcoat_context::Cx,
                body: #topcoat_router::Body,
            ) -> ::std::pin::Pin<
                Box<dyn ::core::future::Future<Output = #topcoat_error::Result<#topcoat_view::View>> + Send>
            > {
                ::std::boxed::Box::pin(async move {
                    #fm_insert
                    Ok(#view_tokens?)
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

/// Derives a route path for a discovered `.mdx` or `.md` file.
///
/// Given the scan directory, the resolved file path, and an optional prefix,
/// computes the route path: applies the prefix, then appends the relative
/// directory structure and kebab-cased filename stem.
fn derive_route_path(
    scan_dir: &Path,
    file_path: &Path,
    prefix: Option<&str>,
) -> String {
    let relative = file_path
        .strip_prefix(scan_dir)
        .unwrap_or(file_path)
        .to_string_lossy();

    // Remove .mdx or .md extension.
    let mut route = relative.into_owned();
    if let Some(ext) = std::path::Path::new(&route).extension().and_then(|e| e.to_str()) {
        if ext.eq_ignore_ascii_case("mdx") {
            route.truncate(route.len() - 4);
        } else if ext.eq_ignore_ascii_case("md") {
            route.truncate(route.len() - 2);
        }
    }

    // Kebab-case the filename stem (last path component).
    let parts: Vec<&str> = route.rsplitn(2, '/').collect();
    let (dir_part, stem) = if parts.len() == 2 {
        (Some(parts[1]), parts[0])
    } else {
        (None, parts[0])
    };
    let kebab_stem = stem.to_kebab_case();

    let mut path_parts: Vec<String> = Vec::new();
    if let Some(dir) = dir_part {
        let kebab_dir: Vec<String> = dir.split('/').map(|seg| seg.to_kebab_case()).collect();
        path_parts.push(kebab_dir.join("/"));
    }
    path_parts.push(kebab_stem);

    let relative_route = path_parts.join("/");

    match prefix {
        Some(p) => format!("{}/{}", p.trim_end_matches('/'), relative_route),
        None => format!("/{relative_route}"),
    }
}

/// Generates page registration tokens for a single `.mdx` or `.md` file.
///
/// Mirrors the logic in `mdx_page!` but without frontmatter type arguments
/// (since `mdx_pages!` does not carry per-file type declarations).
fn generate_page_registration(
    file_path: &Path,
    route_path: &str,
    span: Span,
) -> Result<proc_macro2::TokenStream, syn::Error> {
    let path_display = file_path.to_string_lossy();
    let resolved = file_path.canonicalize().map_err(|e| {
        syn::Error::new(
            span,
            format!("mdx_pages! cannot resolve path '{path_display}': {e}"),
        )
    })?;

    let content = std::fs::read_to_string(&resolved).map_err(|e| {
        syn::Error::new(
            span,
            format!("mdx_pages! cannot read '{path_display}': {e}"),
        )
    })?;

    let CompiledMdxResult { view_tokens, .. } =
        parse_and_walk_mdx(&[], &content, &format!("mdx_pages!"), span)?;

    // Generate unique identifiers from file stem.
    // Use snake_case for identifiers (valid Rust) but the route path
    // (passed as argument) may use kebab-case.
    let file_stem = resolved
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("page")
        .to_kebab_case()
        .replace('-', "_");
    let render_fn_name = Ident::new(&format!("__mdx_pages_render_{file_stem}"), span);
    let unit_name = Ident::new(&format!("__mdx_pages_{file_stem}"), span);
    let route_path_lit = LitStr::new(route_path, span);

    Ok(quote! {
        const _: () = {
            fn #render_fn_name(
                cx: &#topcoat_context::Cx,
                body: #topcoat_router::Body,
            ) -> ::std::pin::Pin<
                Box<dyn ::core::future::Future<Output = #topcoat_error::Result<#topcoat_view::View>> + Send>
            > {
                ::std::boxed::Box::pin(async move {
                    Ok(#view_tokens?)
                })
            }

            #[allow(non_camel_case_types)]
            struct #unit_name;

            const ERASED: #topcoat_router::PageFn = #topcoat_router::PageFn::const_new(
                #topcoat_router::OwnedMethods::One(#topcoat_router::Method::GET),
                ::std::borrow::Cow::Borrowed(#topcoat_router::Path::new(#route_path_lit)),
                #render_fn_name,
            );

            impl ::core::convert::From<#unit_name> for #topcoat_router::PageFn {
                fn from(_: #unit_name) -> Self {
                    ERASED
                }
            }

            #topcoat_inventory::submit!(ERASED);
        };
    })
}

/// Auto-discovers `.mdx` and `.md` files in a directory and registers each as a page route.
///
/// # Arguments
///
/// * `directory_path` - A string literal pointing to a directory, relative to
///   `CARGO_MANIFEST_DIR`. All `.mdx` and `.md` files within this directory are scanned.
/// * `prefix = "/path"` (optional) - A route path prefix prepended to each derived route.
///
/// # Examples
///
/// ```ignore
/// // Register all .mdx and .md files under content/blog/ with /blog prefix:
/// mdx_pages!("content/blog", prefix = "/blog");
///
/// // Register without prefix:
/// mdx_pages!("pages");
/// ```
///
/// Route paths are derived from the file structure:
/// - `content/blog/hello-world.mdx` -> `/blog/hello-world`
/// - `content/blog/nested/post.mdx` -> `/blog/nested/post`
#[proc_macro]
pub fn mdx_pages(tokens: TokenStream) -> TokenStream {
    let input = match syn::parse::<MdxPagesInput>(tokens) {
        Ok(i) => i,
        Err(e) => {
            return syn::Error::new(
                Span::call_site(),
                format!(
                    "mdx_pages! expects: directory_path [, prefix = \"/path\"]: {e}"
                ),
            )
            .to_compile_error()
            .into();
        }
    };

    let dir_str = input.directory_path.value();
    let span = input.directory_path.span();
    let manifest_dir =
        std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
    let scan_dir = Path::new(&manifest_dir).join(&dir_str);

    // Validate scan directory exists.
    if !scan_dir.is_dir() {
        return syn::Error::new(
            span,
            format!("mdx_pages! directory '{dir_str}' does not exist (resolved: {})", scan_dir.display()),
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

    let prefix = input.prefix.as_ref().map(syn::LitStr::value);

    // Use ignore::Walk to find all .mdx files, respecting .gitignore.
    let mut results = Vec::new();
    for entry in ignore::Walk::new(&canonical_scan_dir) {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                // Skip entries that cannot be read (e.g., permission errors).
                // These are non-fatal — just log and continue.
                continue;
            }
        };

        let file_path = entry.path();

        // Only process .mdx and .md files.
        let is_target = file_path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mdx") || ext.eq_ignore_ascii_case("md"));
        if !is_target {
            continue;
        }

        // Security: verify resolved path stays within manifest directory (T-03-04).
        if !file_path.starts_with(&canonical_manifest) {
            results.push(
                syn::Error::new(
                    span,
                    format!(
                        "mdx_pages! file '{}' escapes CARGO_MANIFEST_DIR (T-03-04)",
                        file_path.display()
                    ),
                )
                .to_compile_error(),
            );
            continue;
        }

        // Derive the route path.
        let route_path = derive_route_path(&canonical_scan_dir, file_path, prefix.as_deref());

        // Generate registration tokens for this file.
        match generate_page_registration(file_path, &route_path, span) {
            Ok(ts) => results.push(ts),
            Err(e) => results.push(e.to_compile_error()),
        }
    }

    quote! {
        #(#results)*
    }
    .into()
}

// ---------------------------------------------------------------------------
// serde_value::Value -> syn::Expr conversion
// ---------------------------------------------------------------------------

/// Converts a `serde_value::Value` into a `syn::Expr` that constructs the
/// equivalent Rust value at compile time.
fn value_to_expr(value: &serde_value::Value, span: Span) -> Result<syn::Expr, syn::Error> {
    match value {
        serde_value::Value::Bool(b) => {
            Ok(syn::parse_quote! { #b })
        }
        serde_value::Value::I8(n) => {
            Ok(make_lit_int(&format!("{n}i8"), span))
        }
        serde_value::Value::I16(n) => {
            Ok(make_lit_int(&format!("{n}i16"), span))
        }
        serde_value::Value::I32(n) => {
            Ok(make_lit_int(&format!("{n}i32"), span))
        }
        serde_value::Value::I64(n) => {
            Ok(make_lit_int(&format!("{n}i64"), span))
        }
        serde_value::Value::U8(n) => {
            Ok(make_lit_int(&format!("{n}u8"), span))
        }
        serde_value::Value::U16(n) => {
            Ok(make_lit_int(&format!("{n}u16"), span))
        }
        serde_value::Value::U32(n) => {
            Ok(make_lit_int(&format!("{n}u32"), span))
        }
        serde_value::Value::U64(n) => {
            Ok(make_lit_int(&format!("{n}u64"), span))
        }
        serde_value::Value::F32(n) => {
            Ok(make_lit_float(&format!("{n:?}f32"), span))
        }
        serde_value::Value::F64(n) => {
            Ok(make_lit_float(&format!("{n:?}f64"), span))
        }
        serde_value::Value::Char(c) => {
            Ok(syn::parse_quote! { #c })
        }
        serde_value::Value::String(s) => {
            Ok(syn::parse_quote! { #s })
        }
        serde_value::Value::Unit => {
            Ok(syn::parse_quote! { () })
        }
        serde_value::Value::Option(None) => {
            Ok(syn::parse_quote! { None })
        }
        serde_value::Value::Option(Some(inner)) => {
            let inner_expr = value_to_expr(inner, span)?;
            Ok(syn::parse_quote! { Some(#inner_expr) })
        }
        serde_value::Value::Newtype(inner) => {
            value_to_expr(inner, span)
        }
        serde_value::Value::Seq(items) => {
            let exprs: Result<Vec<syn::Expr>, syn::Error> =
                items.iter().map(|v| value_to_expr(v, span)).collect();
            let expr_list = exprs?;
            Ok(syn::parse_quote! { vec![#(#expr_list),*] })
        }
        serde_value::Value::Map(entries) => {
            // Convert map entries to struct-like field initializers.
            let mut fields = Vec::new();
            for (key, val) in entries {
                let serde_value::Value::String(field_name) = key else {
                    return Err(syn::Error::new(
                        span,
                        format!("mdx_page! frontmatter map key is not a string: {key:?}"),
                    ));
                };
                let field_ident = syn::Ident::new(field_name, span);
                let field_expr = value_to_expr(val, span)?;
                fields.push(quote! { #field_ident: #field_expr });
            }
            Ok(syn::parse_quote! { { #(#fields),* } })
        }
        serde_value::Value::Bytes(b) => {
            // Bytes in frontmatter are unusual; encode as a vec of u8 values.
            let bytes: Vec<syn::Expr> = b.iter()
                .map(|v| make_lit_int(&format!("{v}u8"), span))
                .collect();
            Ok(syn::parse_quote! { vec![#(#bytes),*] })
        }
    }
}

/// Create a `syn::Expr` from an integer literal with a type suffix.
fn make_lit_int(repr: &str, _span: Span) -> syn::Expr {
    let lit: syn::LitInt = syn::parse_str(repr).expect("valid integer literal");
    syn::parse_quote! { #lit }
}

/// Create a `syn::Expr` from a float literal with a type suffix.
fn make_lit_float(repr: &str, _span: Span) -> syn::Expr {
    let lit: syn::LitFloat = syn::parse_str(repr).expect("valid float literal");
    syn::parse_quote! { #lit }
}
