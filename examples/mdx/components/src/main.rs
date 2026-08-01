mod components;

use topcoat::{
    Result,
    context::Cx,
    mdx::compile_mdx,
    router::{page, Router},
};

// --- Server -----------------------------------------------------------------

#[tokio::main]
async fn main() {
    topcoat::start(router()).await.unwrap();
}

// --- Router -----------------------------------------------------------------

fn router() -> Router {
    Router::builder()
        .page(callouts)
        .page(wrappers)
        .page(self_closing)
        .page(nested)
        .build()
}

// --- Pages ------------------------------------------------------------------

#[page("/callouts")]
async fn callouts(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! {
            Callout => components::callout,
            Wrapper => components::wrapper,
            Divider => components::divider,
        },
        "pages/callouts.mdx"
    )
}

#[page("/wrappers")]
async fn wrappers(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! {
            Callout => components::callout,
            Wrapper => components::wrapper,
            Divider => components::divider,
        },
        "pages/wrappers.mdx"
    )
}

#[page("/self-closing")]
async fn self_closing(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! {
            Callout => components::callout,
            Wrapper => components::wrapper,
            Divider => components::divider,
        },
        "pages/self-closing.mdx"
    )
}

#[page("/nested")]
async fn nested(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! {
            Callout => components::callout,
            Wrapper => components::wrapper,
            Divider => components::divider,
        },
        "pages/nested.mdx"
    )
}
