mod components;

use topcoat::{
    Result,
    context::Cx,
    mdx::compile_mdx,
    router::{layout, module_router, page, Router},
    view::view,
};

// --- Server -----------------------------------------------------------------

#[tokio::main]
async fn main() {
    topcoat::start(router()).await.unwrap();
}

// --- Router -----------------------------------------------------------------

fn router() -> Router {
    module_router!().build()
}

// --- Layout -----------------------------------------------------------------

#[layout]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        cx =>
        <html>
            <head>
                <title>"MDX Docs"</title>
                topcoat::dev::script()
            </head>
            <body>
                <nav>
                    <a href="/footnotes">"Footnotes"</a> " | "
                    <a href="/references">"Reference Links"</a> " | "
                    <a href="/overrides">"Overrides"</a> " | "
                    <a href="/wrappers">"Wrappers"</a> " | "
                    <a href="/excerpts">"Excerpts"</a> " | "
                    <a href="/code-blocks">"Code Blocks"</a> " | "
                    <a href="/heading-ids">"Heading IDs"</a>
                </nav>
                <hr />
                (slot?)
            </body>
        </html>
    }
}

// --- Pages ------------------------------------------------------------------

#[page]
async fn home(_cx: &Cx) -> Result {
    view! {
        _cx =>
        <h1>"MDX Features"</h1>
        <p>"This example demonstrates advanced MDX features in Topcoat."</p>
        <ul>
            <li><a href="/footnotes">"Footnotes"</a></li>
            <li><a href="/references">"Reference Links"</a></li>
            <li><a href="/overrides">"Element Overrides"</a></li>
            <li><a href="/wrappers">"Content Wrappers"</a></li>
            <li><a href="/excerpts">"Excerpts"</a></li>
            <li><a href="/code-blocks">"Code Block Meta"</a></li>
            <li><a href="/heading-ids">"Heading IDs"</a></li>
        </ul>
    }
}

#[page("/footnotes")]
async fn footnotes(_cx: &Cx) -> Result {
    compile_mdx!("pages/footnotes.mdx")
}

#[page("/references")]
async fn references(_cx: &Cx) -> Result {
    compile_mdx!("pages/references.mdx")
}

#[page("/overrides")]
async fn overrides(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components!{},
        overrides = { "a" => components::branded_link },
        "pages/overrides.mdx"
    )
}

#[page("/wrappers")]
async fn wrappers(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components!{},
        wrapper = components::page_wrapper,
        "pages/wrappers.mdx"
    )
}

#[page("/excerpts")]
async fn excerpts(_cx: &Cx) -> Result {
    compile_mdx!("pages/excerpts.mdx")
}

#[page("/code-blocks")]
async fn code_blocks(_cx: &Cx) -> Result {
    compile_mdx!("pages/code-blocks.mdx")
}

#[page("/heading-ids")]
async fn heading_ids(_cx: &Cx) -> Result {
    compile_mdx!("pages/heading-ids.mdx")
}
