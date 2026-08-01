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
        .page(home)
        .page(about)
        .build()
}

// --- Pages ------------------------------------------------------------------

#[page("/")]
async fn home(_cx: &Cx) -> Result {
    compile_mdx!("pages/home.mdx")
}

#[page("/about")]
async fn about(_cx: &Cx) -> Result {
    compile_mdx!("pages/about.mdx")
}
