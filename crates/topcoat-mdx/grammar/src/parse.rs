//! Parser configuration for `markdown-rs`.
//!
//! Provides `get_parse_options()` which returns a `ParseOptions` value with
//! GFM, MDX JSX, and frontmatter constructs enabled.

use markdown::{Constructs, ParseOptions};

/// Returns the default parse options for MDX compilation.
///
/// Enables GFM extensions (tables, strikethrough, task lists, autolinks),
/// MDX JSX flow and text support, and YAML frontmatter.
#[must_use]
pub fn get_parse_options() -> ParseOptions {
    let mut constructs = Constructs::gfm();
    constructs.mdx_jsx_flow = true;
    constructs.mdx_jsx_text = true;
    constructs.frontmatter = true;
    ParseOptions {
        constructs,
        ..ParseOptions::default()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_options_enable_gfm_table() {
        let opts = get_parse_options();
        assert!(opts.constructs.gfm_table);
    }

    #[test]
    fn parse_options_enable_gfm_strikethrough() {
        let opts = get_parse_options();
        assert!(opts.constructs.gfm_strikethrough);
    }

    #[test]
    fn parse_options_enable_mdx_jsx() {
        let opts = get_parse_options();
        assert!(opts.constructs.mdx_jsx_flow);
        assert!(opts.constructs.mdx_jsx_text);
    }

    #[test]
    fn parse_options_enable_frontmatter() {
        let opts = get_parse_options();
        assert!(opts.constructs.frontmatter);
    }
}
