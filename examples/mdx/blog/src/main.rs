use topcoat::{
    Result,
    context::Cx,
    router::{layout, module_router, page, RouterBuilderDiscoverExt},
    view::view,
};

// --- Server ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    topcoat::start(router()).await.unwrap();
}

// --- Router ------------------------------------------------------------------

pub fn router() -> topcoat::router::Router {
    // `module_router!` discovers `#[page]` functions from Rust modules.
    // `.discover()` picks up the `PageFn` entries that `mdx_pages!` registers
    // via `inventory::submit!` for the individual blog posts.
    module_router!().discover().build()
}

// --- Layout ------------------------------------------------------------------

#[layout]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        cx =>
        <html>
            <head>
                <title>"MDX Blog"</title>
                topcoat::dev::script()
            </head>
            <body>
                <nav>
                    <a href="/">"Home"</a>
                    " | "
                    <a href="/blog">"Blog"</a>
                </nav>
                <hr />
                (slot?)
            </body>
        </html>
    }
}

// --- Home page ---------------------------------------------------------------

#[page]
async fn home(cx: &Cx) -> Result {
    view! {
        cx =>
        <h1>"Welcome"</h1>
        <p>"Check out the " <a href="/blog">"blog"</a> "."</p>
    }
}

// --- Blog module -------------------------------------------------------------

mod blog {
    use super::*;

    use topcoat::mdx::mdx_pages;

    mdx_pages!("posts", prefix = "/blog");

    #[page]
    async fn index(cx: &Cx) -> Result {
        let posts = mdx_index_posts();
        view! {
            cx =>
            <h1>"Blog Posts"</h1>
            <ul>
                for post in posts {
                    <li>
                        <a href=(format!("/blog/{slug}", slug = post.slug))>
                            (post.title.unwrap_or(post.slug))
                        </a>
                        " — "
                        (post.date.unwrap_or("undated"))
                    </li>
                }
            </ul>
        }
    }
}
