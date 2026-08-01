The `mdx_pages!` macro scans a directory for `.mdx` and `.md` files, compiles each one at build time into `view!` AST nodes, and registers a page route per file. It also emits a const index array and accessor function for content indexing purposes.

```rust,ignore
use topcoat::{mdx::mdx_pages, router::Router, router::RouterBuilderDiscoverExt};

mdx_pages!("content/blog", prefix = "/blog");

let router = Router::builder().discover().build();
```

`mdx_pages!` must be placed at module level — it generates consts, functions, and inventory registrations that cannot appear inside a function body.

# Combining with `module_router!`

`mdx_pages!` registers each page as a `PageFn` in the link-time inventory. When using `module_router!`, call `.discover()` on the returned builder so that these inventory items are picked up:

```rust,ignore
use topcoat::router::{module_router, RouterBuilderDiscoverExt};

pub fn router() -> Router {
    let builder: RouterBuilder = module_router!().into();
    builder.discover().build()
}
```

Without `.discover()`, the `#[page]`, `#[layout]`, and `#[route]` items in your module tree work fine — but any pages registered by `mdx_pages!` will not appear on the router.

# Syntax

```text
mdx_pages!(directory_path [, prefix = "/path"] [, frontmatter = Type] [, components = {...}] [, overrides = {...}] [, wrapper = Path])
```

The `directory_path` argument is a required string literal. All remaining arguments are optional and may appear in any order.

## Directory path

A string literal pointing to the directory, relative to `CARGO_MANIFEST_DIR`. All `.mdx` and `.md` files within this directory are scanned recursively. Files matching `.gitignore` patterns are excluded:

```rust,ignore
mdx_pages!("content/blog");
```

## Route prefix

Pass `prefix = "/path"` to prepend a route path segment to each derived route:

```rust,ignore
mdx_pages!("content/blog", prefix = "/blog");
```

This would register `content/blog/hello-world.mdx` at `/blog/hello-world`.

## Route derivation

Route paths are derived from file structure relative to the scan directory. File stems are converted to kebab-case. For example:

| File path | Derived route (no prefix) | Derived route (`prefix = "/blog"`) |
|---|---|---|
| `hello-world.mdx` | `/hello-world` | `/blog/hello-world` |
| `nested/post.mdx` | `/nested/post` | `/blog/nested/post` |

## Shared frontmatter

Pass `frontmatter = Type` to deserialize the frontmatter of every page in the directory using the same type:

```rust,ignore
#[derive(serde::Deserialize)]
struct BlogMeta {
    title: String,
    date: String,
}

mdx_pages!("content/blog", prefix = "/blog", frontmatter = BlogMeta);
```

All pages must use compatible frontmatter shapes. If different directories need different types, use separate `mdx_pages!` calls.

## Shared components

Pass `components = {...}` to supply a component registry that applies to all pages in the directory:

```rust,ignore
mdx_pages!(
    "content/blog",
    components = {
        Callout => components::callout,
    }
);
```

The registry applies to every page in the scan, so a component used by several files is declared once.

## Shared overrides

Pass `overrides = { ... }` to replace HTML elements with components across all pages:

```rust,ignore
mdx_pages!(
    "content/blog",
    overrides = { "a" => components::custom_link }
);
```

## Shared wrapper

Pass `wrapper = Path` to wrap all pages in the same layout component:

```rust,ignore
mdx_pages!("content/blog", wrapper = components::blog_layout);
```

# Content Indexer

The macro emits two artifacts for content indexing: a const array and an accessor function.

## Index const

A `&'static [MdxIndexEntry]` const named `MDX_INDEX_{DIR}` is emitted, where `{DIR}` is the directory path converted to uppercase with separators replaced by underscores. For example, scanning `"content/blog"` produces `MDX_INDEX_CONTENT_BLOG`.

Each `MdxIndexEntry` contains the following fields populated from frontmatter and file metadata:

- `slug`: the kebab-cased route slug derived from the file stem
- `title`: the `title` field from frontmatter, if present
- `date`: the `date` field from frontmatter, if present
- `excerpt`: the `excerpt` field from frontmatter, or the content before `<!-- more -->`
- `tags`: the `tags` field from frontmatter as a slice of strings, empty if absent

## Index accessor function

A function named `mdx_index_{dir}` is emitted, where `{dir}` is the lowercase directory path with separators replaced by underscores. For `"content/blog"`, the accessor is `mdx_index_content_blog()`:

```rust,ignore
use topcoat::{mdx::mdx_pages, Result, router::page, view::view};

mdx_pages!("content/blog", prefix = "/blog");

#[page]
async fn blog_index() -> Result {
    let entries = mdx_index_content_blog();
    view! {
        <ul>
            for entry in entries {
                <li>
                    <a href=(entry.path)>(entry.title.unwrap_or(entry.slug))</a>
                </li>
            }
        </ul>
    }
}
```

# File Extensions

Files with the `.mdx` extension support embedded component tags. Files with the `.md` extension are parsed as plain markdown. Both extensions are scanned.

[`compile_mdx!`]: macro.compile_mdx.html
[`MdxIndexEntry`]: struct.MdxIndexEntry.html
