use std::path::Path;

use proc_macro2::Span;
use syn::Path as SynPath;
use topcoat_mdx_grammar::{
    parse::get_parse_options,
    walker::{
        FrontmatterFormat, extract_frontmatter, find_excerpt_split, walk_excerpt_to_writer,
        walk_to_writer,
    },
};
use topcoat_view_grammar::view::ViewWriter;

// ---------------------------------------------------------------------------
// Common compile logic shared by compile_mdx! and mdx_page!
// ---------------------------------------------------------------------------

/// Result of compiling an MDX file: frontmatter content (if any) and view tokens.
///
/// The frontmatter can be YAML or TOML format; the format is tracked so
/// [`mdx_page!`] can dispatch to the correct deserializer. When `has_wrapper`
/// is true, the caller must wrap `view_tokens` in a component invocation.
pub(crate) struct CompiledMdxResult {
    /// Raw frontmatter content and its format (YAML or TOML).
    pub(crate) frontmatter_content: Option<(String, FrontmatterFormat)>,
    /// View tokens from the walker. When a wrapper was requested, these are
    /// produced by `ViewWriter::new_nested()` (plain View expression, no
    /// async wrapper).
    pub(crate) view_tokens: proc_macro2::TokenStream,
    /// Whether a wrapper component was requested.
    pub(crate) has_wrapper: bool,
    /// The wrapper component path (set when `has_wrapper` is true).
    pub(crate) wrapper_path: Option<SynPath>,
    /// Excerpt tokens from the walker when `<!-- more -->` is present.
    /// Contains the view tokens for the content before the excerpt marker,
    /// produced by a separate ViewWriter.
    ///
    /// TODO: Currently computed but not emitted to generated code. The
    /// caller (`compile_mdx!`, `mdx_page!`) only uses `view_tokens`.
    /// Excerpt tokens should be exposed as a separate const or component
    /// prop so consuming code can render the excerpt independently.
    #[allow(dead_code)]
    pub(crate) excerpt_tokens: Option<proc_macro2::TokenStream>,
}

/// Shared inner logic: parse markdown content, extract frontmatter, walk mdast.
///
/// Used by both [`compile_mdx_file`] (`compile_mdx!`, `mdx_page!`) and
/// [`generate_page_registration`] (`mdx_pages!`). The `label` parameter controls
/// the prefix in error messages. The `overrides` parameter registers HTML
/// element → component substitutions (e.g., `"a" => custom_link`). When
/// `wrapper` is `Some`, uses `ViewWriter::new_nested()` so the output tokens
/// are suitable for a component `child:` prop.
pub(crate) fn parse_and_walk_mdx(
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
pub(crate) fn compile_mdx_file(
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
