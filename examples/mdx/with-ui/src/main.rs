mod components;

use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::fontsource::fontsource_font,
    router::{Router, RouterBuilderDiscoverExt, layout, page},
    tailwind,
    view::view,
    mdx::compile_mdx,
};

// --- Server ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let router = Router::builder()
        .assets(AssetBundle::load().unwrap())
        .discover()
        .build();
    topcoat::start(router).await.unwrap();
}

// --- Layout ------------------------------------------------------------------

#[layout("/")]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        cx =>
        <!DOCTYPE html>
        <html>
            <head>
                <title>"MDX with UI"</title>
                topcoat::dev::script()
                topcoat::font::link(font: fontsource_font!(GEIST, host: Asset))
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
            </head>
            <body>
                <nav class="border-b px-6 py-3">
                    <a href="/" class="font-semibold">"MDX + UI"</a>
                </nav>
                <main class="mx-auto max-w-3xl px-6 py-8">
                    (slot?)
                </main>
            </body>
        </html>
    }
}

// --- Pages -------------------------------------------------------------------

#[page]
async fn home(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! {
            Card => components::card::card,
            Card_Header => components::card::card_header,
            Card_Title => components::card::card_title,
            Card_Description => components::card::card_description,
            Card_Content => components::card::card_content,
            Card_Footer => components::card::card_footer,
            Badge => components::badge::badge,
            Button => components::button::button,
        },
        "pages/index.mdx"
    )
}
