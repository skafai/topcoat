//! `Frontmatter<T>` request extractor for MDX page frontmatter.
//!
//! Frontmatter values are deserialized at compile time by `mdx_page!` and
//! stored in the request extensions. This extractor reads them at zero
//! runtime cost.

use std::ops::Deref;

use anyhow::anyhow;
use topcoat_core::{context::Cx, error::Result};
use topcoat_router::{Body, error::internal_server_error, request::{extensions, FromRequest}};

/// Zero-cost request extractor for MDX page frontmatter.
///
/// Frontmatter is deserialized at compile time by `mdx_page!` into a `const`,
/// then cloned into the request extensions at the start of each handler
/// invocation. This extractor reads the value from the extensions.
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
/// #[page("/blog/hello")]
/// async fn hello_page(
///     cx: &Cx,
///     Frontmatter(meta): Frontmatter<BlogMeta>,
/// ) -> Result {
///     view! { cx => <h1>(meta.title)</h1> }
/// }
/// ```
#[derive(Debug, Clone)]
#[must_use]
pub struct Frontmatter<T>(pub T);

impl<T> Deref for Frontmatter<T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> FromRequest for Frontmatter<T>
where
    T: Clone + Send + Sync + 'static,
{
    async fn from_request(cx: &Cx, _body: Body) -> Result<Self> {
        let value: &T = extensions(cx).get::<T>().ok_or_else(|| {
            internal_server_error(anyhow!("Frontmatter not registered for this page"))
        })?;
        Ok(Frontmatter(value.clone()))
    }
}
