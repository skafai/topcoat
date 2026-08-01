mod components;

use topcoat::{
    Result,
    asset::{AssetBundle, RouterBuilderAssetExt},
    context::Cx,
    font::fontsource::fontsource_font,
    router::{layout, module_router, page, RouterBuilderDiscoverExt},
    tailwind,
    view::view,
    mdx::mdx_pages,
};

// --- Server ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    let router = module_router!()
        .discover()
        .assets(AssetBundle::load().unwrap())
        .build();
    topcoat::start(router).await.unwrap();
}

// --- Layout ------------------------------------------------------------------

#[layout]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        cx =>
        <!DOCTYPE html>
        <html>
            <head>
                <title>"MDX Blog"</title>
                topcoat::dev::script()
                topcoat::font::link(font: fontsource_font!(GEIST, host: Asset))
                <link rel="stylesheet" href=(tailwind::stylesheet!())>
            </head>
            <body class="font-sans">
                <nav class="border-b px-6 py-3">
                    <a href="/" class="font-semibold">"MDX Blog"</a>
                    " | "
                    <a href="/blog" class="ml-4">"Blog"</a>
                </nav>
                <main class="mx-auto max-w-3xl px-6 py-8">
                    (slot?)
                </main>
            </body>
        </html>
    }
}

// --- Home page ----------------------------------------------------------------

#[page]
async fn home(_cx: &Cx) -> Result {
    use components::badge::{BadgeVariant, badge};

    view! {
        <h1 class="text-3xl font-bold tracking-tight">"MDX + UI Blog"</h1>
        <p class="mt-3 text-muted-foreground">
            "A blog with "
            badge(variant: BadgeVariant::Secondary, "UI pages")
            " and "
            badge(variant: BadgeVariant::Secondary, "MDX posts")
            "."
        </p>
        <p class="mt-6">
            "The page layout, navigation, and blog index are built with Topcoat UI components. "
            "The actual post content is written in "
            <a class="text-primary underline" href="/blog">
                <code>"MDX"</code>
            </a>
            " and embedded at render time."
        </p>
    }
}

// --- Blog -------------------------------------------------------------------

mod blog {
    use super::*;

    // mdx_pages! registers posts via inventory at /<slug> by default.
    // The prefix pushes them under /blog/ to sit beside the index page.
    mdx_pages!("posts", prefix = "/blog");

    #[page]
    async fn index(cx: &Cx) -> Result {
        use components::badge::{BadgeVariant, badge};
        use components::card::{card, card_description, card_header, card_title};

        let posts = mdx_index_posts();

        view! {
            cx =>
            <h1 class="mb-8 text-3xl font-bold tracking-tight">"Blog Posts"</h1>
            <div class="flex flex-col gap-6">
                for post in posts {
                    <a href=(format!("/blog/{slug}", slug = post.slug))>
                        card(
                            card_header(
                                card_title(
                                    (post.title.unwrap_or(post.slug))
                                    " "
                                    badge(variant: BadgeVariant::Outline, (post.date.unwrap_or("undated")))
                                )
                                card_description(
                                    (post.excerpt.unwrap_or("No excerpt."))
                                )
                            )
                        )
                    </a>
                }
            </div>
        }
    }
}
