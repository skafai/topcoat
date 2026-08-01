#![allow(clippy::approx_constant)]

use topcoat::{
    context::CxTestBuilder,
    mdx::compile_mdx,
};
use topcoat::view as topcoat_view_module;
use topcoat_view_module::{View, component, view};

type Result<T = View> = topcoat::Result<T>;

/// Helper to run `compile_mdx`! with a `__cx` binding so that component
/// code (`__view(__cx, ...)`) compiles correctly.
macro_rules! compile_mdx_with_cx {
    ( $cx:expr => $( $arg:tt )* ) => {{
        let __cx = &$cx;
        compile_mdx!($( $arg )*)
    }};
}

// ---------------------------------------------------------------------------
// Mock components for override integration tests
// ---------------------------------------------------------------------------

mod mock {
    use super::*;

    /// Custom link component that accepts `href` prop (matching HTML <a> attr).
    #[component]
    pub async fn custom_link(href: &'static str, #[default] child: View) -> Result {
        view! {
            <a class="custom-link" href=(href)>
                (child)
            </a>
        }
    }
}

// ---------------------------------------------------------------------------
// Override integration tests
// ---------------------------------------------------------------------------

mod overrides_link {
    use super::*;

    /// Verify that `compile_mdx!` with an `overrides` arg compiles.
    #[tokio::test]
    async fn compiles_with_overrides() {
        let cx = CxTestBuilder::new().build();
        let _view = compile_mdx_with_cx!(cx =>
            mdx_components!{},
            overrides = { "a" => mock::custom_link },
            "tests/fixtures/overrides_link.mdx"
        );
    }

    /// Verify that links render through the override component.
    #[tokio::test]
    async fn renders_link_through_override() {
        let cx = CxTestBuilder::new().build();
        let view = compile_mdx_with_cx!(cx =>
            mdx_components!{},
            overrides = { "a" => mock::custom_link },
            "tests/fixtures/overrides_link.mdx"
        ).expect("view should render successfully");
        let html = view.render(&cx);

        assert!(
            html.contains("custom-link"),
            "link should render through override component. Got:\n{html}"
        );
    }
}

mod overrides_xss_safety {
    use super::*;

    /// Verify that javascript: URLs are NOT routed through the override
    /// component even when one is registered — XSS protection (T-03.1-01).
    #[tokio::test]
    async fn dangerous_url_not_overridden() {
        let cx = CxTestBuilder::new().build();
        let view = compile_mdx_with_cx!(cx =>
            mdx_components!{},
            overrides = { "a" => mock::custom_link },
            "tests/fixtures/overrides_xss.mdx"
        ).expect("view should render safely");
        let html = view.render(&cx);

        assert!(
            !html.contains("custom-link"),
            "javascript: URL should NOT route through override. Got:\n{html}"
        );
    }
}
