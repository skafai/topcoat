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
use syn::{
    Ident, LitStr, Path as SynPath, Token,
    parse::{Parse, ParseStream},
    punctuated::Punctuated,
};
use topcoat_core_grammar::paths::{
    topcoat_context, topcoat_error, topcoat_inventory, topcoat_mdx, topcoat_router, topcoat_view,
};
use topcoat_mdx_grammar::{
    parse::get_parse_options,
    walker::{
        FrontmatterFormat, extract_frontmatter, find_excerpt_split, walk_excerpt_to_writer,
        walk_to_writer,
    },
};
use topcoat_view_grammar::view::ViewWriter;

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
        wrapper: Option<SynPath>,
        lit_str: LitStr,
    },
    TwoArgsWithOverrides {
        components: Vec<(String, SynPath)>,
        overrides: Vec<(&'static str, SynPath)>,
        wrapper: Option<SynPath>,
        lit_str: LitStr,
    },
    OneArg {
        lit_str: LitStr,
    },
}

/// A single `"tag" => Path` pair in the overrides braced block.
struct OverridePair {
    tag: LitStr,
    path: SynPath,
}

impl Parse for OverridePair {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let tag: LitStr = input.parse()?;
        let _: Token![=>] = input.parse()?;
        let path: SynPath = input.parse()?;
        Ok(Self { tag, path })
    }
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
        // Pattern 1: { Ident => Path, ... } [, overrides = { "tag" => Path, ... }]
        // [, wrapper = Path], "path.mdx" — direct braced block
        if input.peek(syn::token::Brace) {
            let content;
            syn::braced!(content in input);
            let components = parse_component_braces(&content)?;
            let overrides = parse_optional_overrides(input)?;
            let wrapper = parse_optional_wrapper(input)?;
            input.parse::<Token![,]>()?;
            let lit_str: LitStr = input.parse()?;
            return if overrides.is_empty() {
                Ok(Self::TwoArgs {
                    components,
                    wrapper,
                    lit_str,
                })
            } else {
                Ok(Self::TwoArgsWithOverrides {
                    components,
                    overrides,
                    wrapper,
                    lit_str,
                })
            };
        }

        // Pattern 2: mdx_components!{ Ident => Path, ... } [, overrides = { "tag" => Path, ... }]
        // [, wrapper = Path], "path.mdx" — mdx_components! macro_rules! invocation.
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
                let overrides = parse_optional_overrides(input)?;
                let wrapper = parse_optional_wrapper(input)?;
                input.parse::<Token![,]>()?;
                let lit_str: LitStr = input.parse()?;
                return if overrides.is_empty() {
                    Ok(Self::TwoArgs {
                        components,
                        wrapper,
                        lit_str,
                    })
                } else {
                    Ok(Self::TwoArgsWithOverrides {
                        components,
                        overrides,
                        wrapper,
                        lit_str,
                    })
                };
            }
        }

        // Pattern 3: "path.mdx" — backward compatible one-arg form
        let lit_str: LitStr = input.parse()?;
        Ok(Self::OneArg { lit_str })
    }
}

/// Parses an optional `wrapper = Path` from a `ParseStream`.
/// Returns `None` if no wrapper keyword is found.
fn parse_optional_wrapper(input: ParseStream) -> syn::Result<Option<SynPath>> {
    let fork = input.fork();
    if !fork.peek(Token![,]) {
        return Ok(None);
    }
    let _: Token![,] = fork.parse()?;
    if !fork.peek(Ident) {
        return Ok(None);
    }
    let maybe_kw: Ident = fork.parse()?;
    if maybe_kw != "wrapper" || !fork.peek(Token![=]) {
        return Ok(None);
    }
    // Consume from the actual stream.
    input.parse::<Token![,]>()?;
    let _kw: Ident = input.parse()?;
    input.parse::<Token![=]>()?;
    let path: SynPath = input.parse()?;
    Ok(Some(path))
}

/// Parses an optional `overrides = { "tag" => Path, ... }` from a `ParseStream`.
/// Returns an empty vector if no overrides keyword is found.
fn parse_optional_overrides(input: ParseStream) -> syn::Result<Vec<(&'static str, SynPath)>> {
    let fork = input.fork();
    if !fork.peek(Token![,]) {
        return Ok(Vec::new());
    }
    let _: Token![,] = fork.parse()?;
    if !fork.peek(Ident) {
        return Ok(Vec::new());
    }
    let maybe_kw: Ident = fork.parse()?;
    if maybe_kw != "overrides" || !fork.peek(Token![=]) {
        return Ok(Vec::new());
    }
    // Consume from the actual stream.
    input.parse::<Token![,]>()?;
    let _kw: Ident = input.parse()?;
    input.parse::<Token![=]>()?;
    let content;
    syn::braced!(content in input);
    let pairs = Punctuated::<OverridePair, Token![,]>::parse_terminated(&content)?;
    Ok(pairs
        .into_iter()
        .map(|p| {
            // Intentional leak: override tag names (e.g. "a", "img", "pre")
            // are small and the WalkContext requires &'static str. Leaking
            // avoids complex lifetime gymnastics at proc-macro expand time.
            // This is acceptable because override tags are declared per-
            // invocation and their total heap cost is negligible.
            (
                String::leak(p.tag.value()) as &'static str,
                p.path,
            )
        })
        .collect())
}

// ---------------------------------------------------------------------------
// mdx_page! input parsing
// ---------------------------------------------------------------------------

/// Input for `mdx_page!`: (`route_path`, `file_path`, [frontmatter = Type], [overrides = {...}],
/// [components = {...}], [wrapper = Path])
struct MdxPageInput {
    route_path: LitStr,
    file_path: LitStr,
    frontmatter_type: Option<syn::Type>,
    overrides: Option<Vec<(&'static str, SynPath)>>,
    components: Option<Vec<(String, SynPath)>>,
    wrapper: Option<SynPath>,
}

impl Parse for MdxPageInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let route_path: LitStr = input.parse()?;
        input.parse::<Token![,]>()?;
        let file_path: LitStr = input.parse()?;

        let mut frontmatter_type = None;
        let mut overrides = None;
        let mut components = None;
        let mut wrapper = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let kw: Ident = input.parse()?;
            if kw == "frontmatter" {
                input.parse::<Token![=]>()?;
                frontmatter_type = Some(input.parse()?);
            } else if kw == "overrides" {
                input.parse::<Token![=]>()?;
                let content;
                syn::braced!(content in input);
                let pairs = Punctuated::<OverridePair, Token![,]>::parse_terminated(&content)?;
                overrides = Some(
                    pairs
                        .into_iter()
                        .map(|p| {
                            // Intentional leak: override tag names are small and
                            // WalkContext requires &'static str.
                            (
                                String::leak(p.tag.value()) as &'static str,
                                p.path,
                            )
                        })
                        .collect(),
                );
            } else if kw == "components" {
                input.parse::<Token![=]>()?;
                let content;
                syn::braced!(content in input);
                components = Some(parse_component_braces(&content)?);
            } else if kw == "wrapper" {
                input.parse::<Token![=]>()?;
                wrapper = Some(input.parse()?);
            } else {
                return Err(syn::Error::new(
                    kw.span(),
                    "expected `frontmatter = Type`, `overrides = { ... }`, `components = { ... }`, or `wrapper = Path`, found something else",
                ));
            }
        }

        Ok(Self {
            route_path,
            file_path,
            frontmatter_type,
            overrides,
            components,
            wrapper,
        })
    }
}

// ---------------------------------------------------------------------------
// mdx_pages! input parsing
// ---------------------------------------------------------------------------

/// Input for `mdx_pages!`: (`directory_path`, prefix = "/optional/prefix",
/// frontmatter = Type, components = {...}, overrides = {...}, wrapper = Path)
struct MdxPagesInput {
    directory_path: LitStr,
    prefix: Option<LitStr>,
    frontmatter_type: Option<syn::Type>,
    components: Option<Vec<(String, SynPath)>>,
    overrides: Option<Vec<(&'static str, SynPath)>>,
    wrapper: Option<SynPath>,
}

impl Parse for MdxPagesInput {
    fn parse(input: ParseStream) -> syn::Result<Self> {
        let directory_path: LitStr = input.parse()?;

        let mut prefix = None;
        let mut frontmatter_type = None;
        let mut components = None;
        let mut overrides = None;
        let mut wrapper = None;

        while input.peek(Token![,]) {
            input.parse::<Token![,]>()?;
            let kw: Ident = input.parse()?;
            if kw == "prefix" {
                input.parse::<Token![=]>()?;
                prefix = Some(input.parse()?);
            } else if kw == "frontmatter" {
                input.parse::<Token![=]>()?;
                frontmatter_type = Some(input.parse()?);
            } else if kw == "components" {
                input.parse::<Token![=]>()?;
                let content;
                syn::braced!(content in input);
                components = Some(parse_component_braces(&content)?);
            } else if kw == "overrides" {
                input.parse::<Token![=]>()?;
                let content;
                syn::braced!(content in input);
                let pairs = Punctuated::<OverridePair, Token![,]>::parse_terminated(&content)?;
                overrides = Some(
                    pairs
                        .into_iter()
                        .map(|p| {
                            // Intentional leak: override tag names are small and
                            // WalkContext requires &'static str.
                            (
                                String::leak(p.tag.value()) as &'static str,
                                p.path,
                            )
                        })
                        .collect(),
                );
            } else if kw == "wrapper" {
                input.parse::<Token![=]>()?;
                wrapper = Some(input.parse()?);
            } else {
                return Err(syn::Error::new(
                    kw.span(),
                    "expected `prefix = \"/path\"`, `frontmatter = Type`, `components = { ... }`, `overrides = { ... }`, or `wrapper = Path`",
                ));
            }
        }

        Ok(Self {
            directory_path,
            prefix,
            frontmatter_type,
            components,
            overrides,
            wrapper,
        })
    }
}

// ---------------------------------------------------------------------------
// Common compile logic shared by compile_mdx! and mdx_page!
// ---------------------------------------------------------------------------

/// Result of compiling an MDX file: frontmatter content (if any) and view tokens.
///
/// The frontmatter can be YAML or TOML format; the format is tracked so
/// [`mdx_page!`] can dispatch to the correct deserializer. When `has_wrapper`
/// is true, the caller must wrap `view_tokens` in a component invocation.
struct CompiledMdxResult {
    /// Raw frontmatter content and its format (YAML or TOML).
    frontmatter_content: Option<(String, FrontmatterFormat)>,
    /// View tokens from the walker. When a wrapper was requested, these are
    /// produced by `ViewWriter::new_nested()` (plain View expression, no
    /// async wrapper).
    view_tokens: proc_macro2::TokenStream,
    /// Whether a wrapper component was requested.
    has_wrapper: bool,
    /// The wrapper component path (set when `has_wrapper` is true).
    wrapper_path: Option<SynPath>,
    /// Excerpt tokens from the walker when `<!-- more -->` is present.
    /// Contains the view tokens for the content before the excerpt marker,
    /// produced by a separate ViewWriter.
    #[allow(dead_code)]
    excerpt_tokens: Option<proc_macro2::TokenStream>,
}

/// Shared inner logic: parse markdown content, extract frontmatter, walk mdast.
///
/// Used by both [`compile_mdx_file`] (`compile_mdx!`, `mdx_page!`) and
/// [`generate_page_registration`] (`mdx_pages!`). The `label` parameter controls
/// the prefix in error messages. The `overrides` parameter registers HTML
/// element → component substitutions (e.g., `"a" => custom_link`). When
/// `wrapper` is `Some`, uses `ViewWriter::new_nested()` so the output tokens
/// are suitable for a component `child:` prop.
fn parse_and_walk_mdx(
    components: &[(String, SynPath)],
    overrides: &[(&'static str, SynPath)],
    wrapper: Option<&SynPath>,
    content: &str,
    label: &str,
    span: Span,
) -> Result<CompiledMdxResult, syn::Error> {
    // Parse with markdown-rs.
    let options = get_parse_options();
    let root = markdown::to_mdast(content, &options)
        .map_err(|e| syn::Error::new(span, format!("{label} parse error: {e}")))?;

    // Extract frontmatter from root node.
    let frontmatter_content = extract_frontmatter(&root);

    // Build override registry from the borrowed slice into owned storage
    // that lives for the WalkContext lifetime.
    let owned_overrides: Vec<(&'static str, SynPath)> = overrides
        .iter()
        .map(|(tag, path)| (*tag, path.clone()))
        .collect();
    let ctx = topcoat_mdx_grammar::walker::WalkContext::new(components, &owned_overrides, span);

    // Determine excerpt split index from the root children (post-frontmatter).
    let (excerpt_split, post_fm_children) = if let markdown::mdast::Node::Root(ref r) = root {
        let start_idx = usize::from(frontmatter_content.is_some());
        let post_fm: &[markdown::mdast::Node] = &r.children[start_idx..];
        let split = find_excerpt_split(post_fm);
        (split, post_fm)
    } else {
        (None, &[] as &[markdown::mdast::Node])
    };

    // Walk mdast into ViewWriter(s), skipping the frontmatter node.
    // Use new_nested() when a wrapper is specified so the tokens are suitable
    // for a component `child:` prop (no async wrapper).
    let mut writer = if wrapper.is_some() {
        ViewWriter::new_nested()
    } else {
        ViewWriter::new()
    };

    // Two-writer approach: if an excerpt split point exists, walk excerpt
    // children into a separate writer and body children into the main writer.
    // Excerpt children are walked through `walk_excerpt_to_writer` which
    // strips `<!-- more -->` from text content so the marker does not appear
    // as visible text in rendered output (CR-01).
    let excerpt_tokens = if let Some(split_idx) = excerpt_split {
        let mut excerpt_writer = ViewWriter::new_nested();
        for child in &post_fm_children[..split_idx] {
            walk_excerpt_to_writer(&ctx, child, &mut excerpt_writer);
        }
        for child in &post_fm_children[split_idx..] {
            walk_to_writer(&ctx, child, &mut writer);
        }
        Some(excerpt_writer.into_token_stream())
    } else {
        for child in post_fm_children {
            walk_to_writer(&ctx, child, &mut writer);
        }
        None
    };

    // Drain walker error buffer into syn::Error diagnostics.
    let errors: Vec<String> = ctx.errors.borrow_mut().drain(..).collect();
    if !errors.is_empty() {
        let mut combined_err = syn::Error::new(span, errors[0].clone());
        for err in &errors[1..] {
            combined_err.combine(syn::Error::new(span, err.clone()));
        }
        return Err(combined_err);
    }

    let inner_tokens = writer.into_token_stream();

    Ok(CompiledMdxResult {
        frontmatter_content,
        view_tokens: inner_tokens,
        has_wrapper: wrapper.is_some(),
        wrapper_path: wrapper.cloned(),
        excerpt_tokens,
    })
}

/// Shared logic: resolve path, read file, parse, extract frontmatter, walk.
/// When `wrapper` is `Some`, emits a component invocation wrapping the view tokens.
fn compile_mdx_file(
    components: &[(String, SynPath)],
    overrides: &[(&'static str, SynPath)],
    wrapper: Option<&SynPath>,
    path_str: &str,
    span: Span,
) -> Result<CompiledMdxResult, syn::Error> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR").unwrap_or_else(|_| ".".to_owned());
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

    parse_and_walk_mdx(
        components,
        overrides,
        wrapper,
        &content,
        "compile_mdx!",
        span,
    )
}

// ---------------------------------------------------------------------------
// compile_mdx! proc-macro
// ---------------------------------------------------------------------------

/// Compiles a `.mdx` or `.md` file into a Topcoat `view!` AST.
///
/// # Arguments
///
/// * `path` - A string literal pointing to the `.mdx` or `.md` file, relative to
///   `CARGO_MANIFEST_DIR`.
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
/// # Panics
/// Panics if `has_wrapper` is true but `wrapper_path` is none — this indicates
/// a bug in `compile_mdx_file`, which must always set `wrapper_path` when
/// `has_wrapper` is true.
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
        let const_name = Ident::new(
            &format!("__MDX_FRONTMATTER_{}", file_stem.to_uppercase()),
            lit_str.span(),
        );
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

/// Registers a `.mdx` or `.md` file as a page route with optional frontmatter.
///
/// # Arguments
///
/// * `route_path` - The URL path for this page (e.g. `"/blog/hello"`).
/// * `file_path` - Path to the `.mdx` or `.md` file, relative to `CARGO_MANIFEST_DIR`.
/// * `frontmatter = Type` (optional) - The Rust type to deserialize the YAML or TOML frontmatter
///   into.
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

/// Derives a route path for a discovered `.mdx` or `.md` file.
///
/// Given the scan directory, the resolved file path, and an optional prefix,
/// computes the route path: applies the prefix, then appends the relative
/// directory structure and kebab-cased filename stem.
fn derive_route_path(scan_dir: &Path, file_path: &Path, prefix: Option<&str>) -> String {
    let relative = file_path
        .strip_prefix(scan_dir)
        .unwrap_or(file_path)
        .to_string_lossy();

    // Remove .mdx or .md extension.
    let mut route = relative.into_owned();
    if let Some(ext) = std::path::Path::new(&route)
        .extension()
        .and_then(|e| e.to_str())
    {
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
        let kebab_dir: Vec<String> = dir.split('/').map(str::to_kebab_case).collect();
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
/// Mirrors the logic in `mdx_page!` but supports frontmatter type, components,
/// overrides, and wrapper arguments from `mdx_pages!`.
fn generate_page_registration(
    file_path: &Path,
    route_path: &str,
    components: &[(String, SynPath)],
    overrides: &[(&'static str, SynPath)],
    wrapper: Option<&SynPath>,
    frontmatter_type: Option<&syn::Type>,
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

    let result = parse_and_walk_mdx(components, overrides, wrapper, &content, "mdx_pages!", span)?;

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

    let view_tokens = &result.view_tokens;

    // Frontmatter const (when frontmatter_type is provided and content has frontmatter).
    let fm_const_and_insert = if let (Some((content, format)), Some(fm_type)) =
        (&result.frontmatter_content, frontmatter_type)
    {
        let fm_const_name = Ident::new(&format!("__MDX_PAGES_FRONTMATTER_{file_stem}"), span);

        let deserialized: serde_value::Value = if matches!(format, FrontmatterFormat::Yaml) {
            serde_saphyr::from_str(content).unwrap_or_else(|e| {
                panic!("mdx_pages! failed to deserialize frontmatter YAML: {e}")
            })
        } else {
            toml::from_str(content).unwrap_or_else(|e| {
                panic!("mdx_pages! failed to deserialize frontmatter TOML: {e}")
            })
        };

        let expr = value_to_expr(&deserialized, Some(fm_type), span)?;
        quote! {
            #[allow(clippy::approx_constant)]
            const #fm_const_name: #fm_type = #expr;
        }
    } else {
        quote! {}
    };

    let fm_insert = if result.frontmatter_content.is_some() && frontmatter_type.is_some() {
        let fm_const_name = Ident::new(&format!("__MDX_PAGES_FRONTMATTER_{file_stem}"), span);
        quote! {
            #topcoat_router::request::extensions(__cx).insert(#fm_const_name.clone());
        }
    } else {
        quote! {}
    };

    // Apply wrapper if requested.
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

    Ok(quote! {
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
/// * `directory_path` - A string literal pointing to a directory, relative to `CARGO_MANIFEST_DIR`.
///   All `.mdx` and `.md` files within this directory are scanned.
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
///
/// The macro also emits a const index array `MDX_INDEX_<DIR>` and an accessor function
/// `mdx_index_<dir>()` returning `&[MdxIndexEntry]` for content indexing purposes.
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
        .filter_map(|entry| {
            let route_path = derive_route_path(
                &canonical_scan_dir,
                &entry.file_path,
                prefix.as_deref(),
            );
            match generate_page_registration(
                &entry.file_path,
                &route_path,
                &components,
                &overrides,
                input.wrapper.as_ref(),
                input.frontmatter_type.as_ref(),
                span,
            ) {
                Ok(ts) => Some(ts),
                Err(e) => Some(e.to_compile_error()),
            }
        })
        .collect();

    // Build index entries from scanned pages.
    let index_entries = build_index(&page_entries, span);

    // Derive a stable identifier from the directory path for the index name.
    let index_suffix = dir_str
        .replace(std::path::MAIN_SEPARATOR, "_")
        .replace('/', "_")
        .replace('-', "_")
        .to_uppercase();

    let index_const_name = Ident::new(
        &format!("MDX_INDEX_{index_suffix}"),
        span,
    );
    let index_fn_name = Ident::new(
        &format!("mdx_index_{}", index_suffix.to_lowercase()),
        span,
    );

    // Combine route results into a single TokenStream.
    let route_tokens = route_results.into_iter().collect::<proc_macro2::TokenStream>();

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

/// A scanned page entry: file path, parsed content, and frontmatter data.
struct MdxPageEntry {
    file_path: std::path::PathBuf,
}

/// Scans a directory for `.mdx` and `.md` files, returning valid entries.
fn scan_directory(
    canonical_scan_dir: &Path,
    canonical_manifest: &Path,
    _span: Span,
) -> Vec<MdxPageEntry> {
    let mut entries = Vec::new();

    for entry in ignore::Walk::new(canonical_scan_dir) {
        let Ok(entry) = entry else {
            continue;
        };

        let file_path = entry.path().to_path_buf();

        // Only process .mdx and .md files.
        let is_target = file_path
            .extension()
            .and_then(|s| s.to_str())
            .is_some_and(|ext| ext.eq_ignore_ascii_case("mdx") || ext.eq_ignore_ascii_case("md"));
        if !is_target {
            continue;
        }

        // Security: verify resolved path stays within manifest directory (T-03-04).
        let resolved_path = file_path.canonicalize().unwrap_or_else(|_| file_path.clone());
        if !resolved_path.starts_with(canonical_manifest) {
            continue;
        }

        entries.push(MdxPageEntry { file_path });
    }

    entries
}

/// Builds index entries from scanned pages.
///
/// For each page, extracts title/date/tags/excerpt from frontmatter
/// using generic serde_value::Value parsing.
fn build_index(
    entries: &[MdxPageEntry],
    _span: Span,
) -> Vec<proc_macro2::TokenStream> {
    let mut index_items = Vec::new();

    for entry in entries {
        let resolved = entry.file_path.canonicalize().unwrap_or_else(|_| {
            entry.file_path.clone()
        });

        let content = match std::fs::read_to_string(&resolved) {
            Ok(c) => c,
            Err(_) => continue,
        };

        // Parse the markdown content.
        let options = get_parse_options();
        let root = match markdown::to_mdast(&content, &options) {
            Ok(r) => r,
            Err(_) => continue,
        };

        // Extract frontmatter data.
        let frontmatter = extract_frontmatter(&root);

        // Derive slug from file stem using kebab-case.
        let slug = resolved
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("page")
            .to_kebab_case();
        // Intentional leak: slugs are small and the index array requires
        // &'static str. Leaking avoids complex lifetime management.
        let slug_str: &'static str = Box::leak(slug.into_boxed_str());

        // Extract known fields from frontmatter using generic deserialization.
        let (title_expr, date_expr, excerpt_expr, tags_expr) =
            if let Some((fm_content, format)) = &frontmatter {
                let deserialized: serde_value::Value =
                    if matches!(format, FrontmatterFormat::Yaml) {
                        serde_saphyr::from_str(fm_content).unwrap_or_else(|e| {
                            panic!("mdx_pages! failed to deserialize frontmatter YAML: {e}")
                        })
                    } else {
                        toml::from_str(fm_content).unwrap_or_else(|e| {
                            panic!("mdx_pages! failed to deserialize frontmatter TOML: {e}")
                        })
                    };

                let title = extract_string_field(&deserialized, "title");
                let date = extract_string_field(&deserialized, "date");
                let excerpt = extract_string_field(&deserialized, "excerpt");
                let tags = extract_tags_field(&deserialized);

                (
                    title.map(|s| quote! { Some(#s) }).unwrap_or(quote! { None }),
                    date.map(|s| quote! { Some(#s) }).unwrap_or(quote! { None }),
                    excerpt.map(|s| quote! { Some(#s) }).unwrap_or(quote! { None }),
                    tags,
                )
            } else {
                (
                    quote! { None },
                    quote! { None },
                    quote! { None },
                    quote! { &[] },
                )
            };

        index_items.push(quote! {
            #topcoat_mdx::MdxIndexEntry {
                slug: #slug_str,
                title: #title_expr,
                date: #date_expr,
                excerpt: #excerpt_expr,
                tags: #tags_expr,
            }
        });
    }

    index_items
}

/// Extract a string field from a serde_value::Value Map.
fn extract_string_field(value: &serde_value::Value, field: &str) -> Option<LitStr> {
    if let serde_value::Value::Map(entries) = value {
        if let Some(inner) = entries.get(&serde_value::Value::String(field.to_string())) {
            if let serde_value::Value::String(s) = inner {
                return Some(LitStr::new(s, Span::call_site()));
            }
        }
    }
    None
}

/// Extract a tags field (sequence of strings) from a serde_value::Value Map.
fn extract_tags_field(value: &serde_value::Value) -> proc_macro2::TokenStream {
    if let serde_value::Value::Map(entries) = value {
        if let Some(serde_value::Value::Seq(items)) =
            entries.get(&serde_value::Value::String("tags".to_string()))
        {
            let tag_lits: Vec<LitStr> = items
                .iter()
                .filter_map(|v| {
                    if let serde_value::Value::String(s) = v {
                        Some(LitStr::new(s, Span::call_site()))
                    } else {
                        None
                    }
                })
                .collect();

            if !tag_lits.is_empty() {
                return quote! { &#(#tag_lits),* };
            }
        }
    }
    quote! { &[] }
}


// ---------------------------------------------------------------------------
// serde_value::Value -> syn::Expr conversion
// ---------------------------------------------------------------------------

/// Converts a `serde_value::Value` into a `syn::Expr` that constructs the
/// equivalent Rust value at compile time.
///
/// If `root_type` is provided and the value is a `Map`, it is used as the
/// struct path for the generated `ExprStruct`.  This is needed because the
/// caller emits the expression directly (not prefixed with a type).
fn value_to_expr(
    value: &serde_value::Value,
    root_type: Option<&syn::Type>,
    span: Span,
) -> Result<syn::Expr, syn::Error> {
    match value {
        serde_value::Value::Bool(b) => Ok(syn::parse_quote! { #b }),
        serde_value::Value::I8(n) => Ok(make_lit_int(&format!("{n}i8"), span)),
        serde_value::Value::I16(n) => Ok(make_lit_int(&format!("{n}i16"), span)),
        serde_value::Value::I32(n) => Ok(make_lit_int(&format!("{n}i32"), span)),
        serde_value::Value::I64(n) => Ok(make_lit_int(&format!("{n}i64"), span)),
        serde_value::Value::U8(n) => Ok(make_lit_int(&format!("{n}u8"), span)),
        serde_value::Value::U16(n) => Ok(make_lit_int(&format!("{n}u16"), span)),
        serde_value::Value::U32(n) => Ok(make_lit_int(&format!("{n}u32"), span)),
        serde_value::Value::U64(n) => Ok(make_lit_int(&format!("{n}u64"), span)),
        serde_value::Value::F32(n) => Ok(make_lit_float(&format!("{n:?}f32"), span)),
        serde_value::Value::F64(n) => Ok(make_lit_float(&format!("{n:?}f64"), span)),
        serde_value::Value::Char(c) => Ok(syn::parse_quote! { #c }),
        serde_value::Value::String(s) => Ok(syn::parse_quote! { #s }),
        serde_value::Value::Unit => Ok(syn::parse_quote! { () }),
        serde_value::Value::Option(None) => Ok(syn::parse_quote! { None }),
        serde_value::Value::Option(Some(inner)) => {
            let inner_expr = value_to_expr(inner, None, span)?;
            Ok(syn::parse_quote! { Some(#inner_expr) })
        }
        serde_value::Value::Newtype(inner) => value_to_expr(inner, None, span),
        serde_value::Value::Seq(items) => {
            let exprs: Result<Vec<syn::Expr>, syn::Error> =
                items.iter().map(|v| value_to_expr(v, None, span)).collect();
            let expr_list = exprs?;
            Ok(syn::parse_quote! { vec![#(#expr_list),*] })
        }
        serde_value::Value::Map(entries) => {
            // Convert map entries to struct-like field initializers.
            // When `root_type` is `Some`, the top-level Map is rendered as a
            // typed struct literal (e.g. `BlogMeta { title, date }`).
            // Nested Maps (recursive calls with `root_type = None`) fall back
            // to a placeholder `_ { ... }` path, which is only valid inside
            // `parse_quote!` — the expression won't compile as standalone Rust.
            // This is acceptable because frontmatter deserialization always
            // passes the root type, and nested maps are rendered as field
            // values (vecs, strings, etc.) rather than struct literals.
            let mut named_fields = Vec::new();
            for (key, val) in entries {
                let serde_value::Value::String(field_name) = key else {
                    return Err(syn::Error::new(
                        span,
                        format!("mdx_page! frontmatter map key is not a string: {key:?}"),
                    ));
                };
                let field_ident = syn::Ident::new(field_name, span);
                let field_expr = value_to_expr(val, None, span)?;
                named_fields.push(syn::FieldValue {
                    attrs: vec![],
                    member: syn::Member::Named(field_ident),
                    colon_token: Some(syn::token::Colon::default()),
                    expr: field_expr,
                });
            }
            // Use the provided root type as the struct path. If not given,
            // fall back to a placeholder so the expression still parses.
            let path = match root_type {
                Some(syn::Type::Path(tp)) => tp.path.clone(),
                _ => syn::Path::from(syn::Ident::new("_", span)),
            };
            Ok(syn::Expr::Struct(syn::ExprStruct {
                attrs: vec![],
                qself: None,
                path,
                brace_token: syn::token::Brace::default(),
                dot2_token: None,
                rest: None,
                fields: named_fields.into_iter().collect(),
            }))
        }
        serde_value::Value::Bytes(b) => {
            // Bytes in frontmatter are unusual; encode as a vec of u8 values.
            let bytes: Vec<syn::Expr> = b
                .iter()
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

#[cfg(test)]
#[allow(clippy::approx_constant)]
mod tests {
    use super::*;

    fn v2e(value: &serde_value::Value) -> Result<syn::Expr, syn::Error> {
        value_to_expr(value, None, Span::call_site())
    }

    #[test]
    fn value_to_expr_unit() {
        let expr = v2e(&serde_value::Value::Unit).unwrap();
        assert!(matches!(expr, syn::Expr::Tuple(tuple) if tuple.elems.is_empty()));
    }

    #[test]
    fn value_to_expr_bool_true() {
        let expr = v2e(&serde_value::Value::Bool(true)).unwrap();
        assert!(
            matches!(expr, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) if b.value())
        );
    }

    #[test]
    fn value_to_expr_bool_false() {
        let expr = v2e(&serde_value::Value::Bool(false)).unwrap();
        assert!(
            matches!(expr, syn::Expr::Lit(syn::ExprLit { lit: syn::Lit::Bool(b), .. }) if !b.value())
        );
    }

    #[test]
    fn value_to_expr_i32() {
        let expr = v2e(&serde_value::Value::I32(42)).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("42"), "should contain 42, got {s}");
    }

    #[test]
    fn value_to_expr_u64() {
        let expr = v2e(&serde_value::Value::U64(1000)).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("1000"), "should contain 1000, got {s}");
    }

    #[test]
    fn value_to_expr_f64_precision() {
        let expr = v2e(&serde_value::Value::F64(3.141_592_65)).unwrap();
        let s = quote! { #expr }.to_string();
        // Should preserve precision, not truncate to "3.1"
        assert!(s.contains("3.14"), "should preserve precision, got {s}");
    }

    #[test]
    fn value_to_expr_f32_precision() {
        let expr = v2e(&serde_value::Value::F32(2.718)).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("2.718"), "should preserve precision, got {s}");
    }

    #[test]
    fn value_to_expr_string() {
        let expr = v2e(&serde_value::Value::String("hello".to_string())).unwrap();
        assert!(matches!(
            expr,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Str(_),
                ..
            })
        ));
    }

    #[test]
    fn value_to_expr_char() {
        let expr = v2e(&serde_value::Value::Char('X')).unwrap();
        assert!(matches!(
            expr,
            syn::Expr::Lit(syn::ExprLit {
                lit: syn::Lit::Char(_),
                ..
            })
        ));
    }

    #[test]
    fn value_to_expr_option_none() {
        let expr = v2e(&serde_value::Value::Option(None)).unwrap();
        assert!(
            matches!(expr, syn::Expr::Path(p) if p.path.segments.last().unwrap().ident == "None")
        );
    }

    #[test]
    fn value_to_expr_option_some() {
        let inner = Box::new(serde_value::Value::String("inner".to_string()));
        let expr = v2e(&serde_value::Value::Option(Some(inner))).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("Some"), "should contain Some");
        assert!(s.contains("inner"), "should contain inner value");
    }

    #[test]
    fn value_to_expr_seq() {
        let items = vec![
            serde_value::Value::I32(1),
            serde_value::Value::I32(2),
            serde_value::Value::I32(3),
        ];
        let expr = v2e(&serde_value::Value::Seq(items)).unwrap();
        // Seq produces a macro invocation (vec![...]).
        assert!(
            matches!(expr, syn::Expr::Macro(_)),
            "should produce a macro invocation"
        );
    }

    #[test]
    fn value_to_expr_empty_map() {
        let entries: std::collections::BTreeMap<serde_value::Value, serde_value::Value> =
            std::collections::BTreeMap::new();
        let expr = v2e(&serde_value::Value::Map(entries)).unwrap();
        // Empty map produces an ExprStruct with placeholder path.
        assert!(
            matches!(expr, syn::Expr::Struct(_)),
            "should produce a struct expression"
        );
    }

    #[test]
    fn value_to_expr_map_with_fields() {
        use std::collections::BTreeMap;
        let mut entries = BTreeMap::new();
        entries.insert(
            serde_value::Value::String("name".to_string()),
            serde_value::Value::String("test".to_string()),
        );
        entries.insert(
            serde_value::Value::String("count".to_string()),
            serde_value::Value::I32(5),
        );
        let expr = v2e(&serde_value::Value::Map(entries)).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("name"), "should contain name field, got {s}");
        assert!(s.contains("count"), "should contain count field, got {s}");
    }

    #[test]
    fn value_to_expr_bytes() {
        let bytes = vec![72, 101, 108, 108, 111];
        let expr = v2e(&serde_value::Value::Bytes(bytes)).unwrap();
        // Bytes produces a macro invocation (vec![...]).
        assert!(
            matches!(expr, syn::Expr::Macro(_)),
            "should produce a macro invocation"
        );
    }

    #[test]
    fn value_to_expr_nested_option() {
        let inner = Box::new(serde_value::Value::Option(Some(Box::new(
            serde_value::Value::I32(42),
        ))));
        let expr = v2e(&serde_value::Value::Option(Some(inner))).unwrap();
        let s = quote! { #expr }.to_string();
        assert!(s.contains("Some"), "should contain Some");
        assert!(s.contains("42"), "should contain nested value");
    }
}
