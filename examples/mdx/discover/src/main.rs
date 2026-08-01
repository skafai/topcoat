use topcoat::{
    Result,
    context::Cx,
    router::{Router, RouterBuilderDiscoverExt, layout, page},
    view::{view, View, component},
    mdx::compile_mdx,
};

// --- Highlight component ---

#[component]
pub async fn highlight(#[default] child: View) -> Result {
    view! {
        <span class="bg-yellow-200 px-1 rounded">(child)</span>
    }
}

// Component is registered via inventory automatically when used with
// `compile_mdx!(mdx_components! { Highlight => highlight }, ...)` below.

// --- Server ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    topcoat::start(Router::builder().discover().build()).await.unwrap();
}

// --- Layout ------------------------------------------------------------------

#[layout]
async fn root_layout(cx: &Cx, slot: Result) -> Result {
    view! {
        cx =>
        <!DOCTYPE html>
        <html>
            <head>
                <title>"MDX Discover"</title>
                topcoat::dev::script()
            </head>
            <body>
                <nav class="border-b px-6 py-3">
                    <a href="/" class="font-semibold">"Discover"</a>
                    " | "
                    <a href="/features">"Features"</a>
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
        mdx_components! { Highlight => highlight },
        "pages/home.mdx"
    )
}

#[page("/features")]
async fn features(_cx: &Cx) -> Result {
    compile_mdx!(
        mdx_components! { Highlight => highlight },
        "pages/features.mdx"
    )
}
