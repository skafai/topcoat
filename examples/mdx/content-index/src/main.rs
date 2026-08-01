use topcoat::{
    Result,
    context::Cx,
    mdx::{MdxIndexEntry, mdx_pages},
    router::{
        Router, RouterBuilderDiscoverExt, layout, page, path_param_segment, response::Response,
        route,
    },
    view::{component, view},
};

// --- Content -----------------------------------------------------------------

// Scanning a directory does two things: it registers a route per file, and it
// reads each file's frontmatter into a compile-time index. Nothing here touches
// the filesystem at runtime.
mdx_pages!("posts", prefix = "/posts");

/// The index in newest-first order. `date` is a string, and the frontmatter
/// uses ISO-8601, so a plain reverse sort is chronological.
fn posts_by_date() -> Vec<&'static MdxIndexEntry> {
    let mut posts: Vec<_> = mdx_index_posts().iter().collect();
    posts.sort_by(|a, b| b.date.unwrap_or_default().cmp(a.date.unwrap_or_default()));
    posts
}

/// Every tag used across the scanned files, deduplicated and sorted.
fn all_tags() -> Vec<&'static str> {
    let mut tags: Vec<&'static str> = mdx_index_posts()
        .iter()
        .flat_map(|post| post.tags.iter().copied())
        .collect();
    tags.sort_unstable();
    tags.dedup();
    tags
}

// --- Views -------------------------------------------------------------------

#[component]
async fn post_card(post: &'static MdxIndexEntry) -> Result {
    view! {
        <li class="border-b py-4">
            <a href=(post.path) class="text-lg font-medium">
                (post.title.unwrap_or(post.slug))
            </a>
            <p class="text-sm text-gray-500">(post.date.unwrap_or("undated"))</p>
            if let Some(excerpt) = post.excerpt {
                <p class="mt-1">(excerpt)</p>
            }
            <p class="mt-1 text-sm">
                for tag in post.tags {
                    <a href=(format!("/tags/{tag}")) class="mr-2">
                        "#"
                        (tag)
                    </a>
                }
            </p>
        </li>
    }
}

// --- Pages -------------------------------------------------------------------

#[page("/")]
async fn index() -> Result {
    let posts = posts_by_date();
    view! {
        <h1 class="text-2xl font-semibold">"All posts"</h1>
        <ul>
            for post in posts {
                post_card(post: post)
            }
        </ul>
    }
}

#[page("/tags/{tag}")]
async fn tag_page(cx: &Cx) -> Result {
    let tag = path_param_segment(cx, "tag");
    let posts: Vec<_> = mdx_index_posts()
        .iter()
        .filter(|post| post.tags.contains(&tag))
        .collect();

    view! {
        <h1 class="text-2xl font-semibold">
            "Tagged "
            (tag)
        </h1>
        if posts.is_empty() {
            <p>"No posts carry this tag."</p>
        } else {
            <ul>
                for post in posts {
                    post_card(post: post)
                }
            </ul>
        }
    }
}

// The index is a plain slice, so it feeds non-HTML responses just as well.
#[route(GET "/sitemap.xml")]
async fn sitemap() -> Result<Response> {
    use std::fmt::Write;

    let mut xml = String::from(r#"<?xml version="1.0" encoding="UTF-8"?>"#);
    xml.push_str(r#"<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">"#);
    for post in mdx_index_posts() {
        write!(xml, "<url><loc>{}</loc></url>", post.path).expect("writing to a String cannot fail");
    }
    xml.push_str("</urlset>");

    Ok(Response::builder()
        .header("content-type", "application/xml")
        .body(xml.into())
        .expect("sitemap response is well-formed"))
}

// --- Layout ------------------------------------------------------------------

#[layout("/")]
async fn root_layout(slot: Result) -> Result {
    view! {
        <!DOCTYPE html>
        <html>
            <head>
                <title>"MDX Content Index"</title>
                topcoat::dev::script()
            </head>
            <body>
                <nav class="border-b px-6 py-3">
                    <a href="/" class="font-semibold">"Posts"</a>
                    " | "
                    for tag in all_tags() {
                        <a href=(format!("/tags/{tag}")) class="mr-2">
                            "#"
                            (tag)
                        </a>
                    }
                    " | "
                    <a href="/sitemap.xml">"Sitemap"</a>
                </nav>
                <main class="mx-auto max-w-2xl px-6 py-8">(slot?)</main>
            </body>
        </html>
    }
}

// --- Server ------------------------------------------------------------------

#[tokio::main]
async fn main() {
    topcoat::start(Router::builder().discover().build())
        .await
        .unwrap();
}
