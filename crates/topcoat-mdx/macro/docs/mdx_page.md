The `mdx_page!` macro compiles a single `.mdx` or `.md` file and registers it as a page route. The macro reads the file at compile time, parses it with `markdown-rs`, walks the syntax tree into `view!` AST nodes, and submits the route to the inventory so that `Router::builder().discover()` picks it up.

```rust,ignore
use serde::Deserialize;
use topcoat::{mdx::mdx_page, router::Router, router::RouterBuilderDiscoverExt};

#[derive(Deserialize)]
struct BlogMeta {
    title: String,
    date: String,
}

mdx_page!("/blog/hello", "content/hello.mdx", frontmatter = BlogMeta);

let router = Router::builder().discover().build();
```

# Syntax

```text
mdx_page!(route_path, file_path [, frontmatter = Type] [, components = {...}] [, overrides = {...}] [, wrapper = Path])
```

The `route_path` and `file_path` arguments are required string literals. All remaining arguments are optional and may appear in any order.

## Route path

A string literal specifying the URL path for this page:

```rust,ignore
mdx_page!("/blog/hello", "content/hello.mdx");
```

## File path

A string literal pointing to the `.mdx` or `.md` file, relative to `CARGO_MANIFEST_DIR`:

```rust,ignore
mdx_page!("/about", "pages/about.mdx");
```

## Frontmatter

Pass `frontmatter = Type` to deserialize the file frontmatter at compile time into a Rust struct:

```rust,ignore
#[derive(serde::Deserialize)]
struct BlogMeta {
    title: String,
    date: String,
}

mdx_page!("/blog/hello", "content/hello.mdx", frontmatter = BlogMeta);
```

The macro reads YAML or TOML frontmatter from the file, deserializes it into a `serde_value::Value`, and then converts it into a `const` of the specified type. The frontmatter is stored in the request extensions on each handler invocation. Read it with the `Frontmatter<T>` extractor:

```rust,ignore
#[page("/blog/hello")]
async fn hello_page(
    cx: &Cx,
    meta: Frontmatter<BlogMeta>,
) -> topcoat::Result {
    view! { cx => <h1>(meta.title)</h1> }
}
```

## Components

Pass `components = {...}` to supply an inline component registry:

```rust,ignore
mdx_page!(
    "/blog/hello",
    "content/hello.mdx",
    components = {
        Callout => components::callout,
        Divider => components::divider,
    }
);
```

Alternatively, use `mdx_components!{...}` or rely on the global inventory when the `discover` feature is enabled. See [`compile_mdx!`][] for the different registry forms.

## Overrides

Pass `overrides = { ... }` to replace HTML elements with components:

```rust,ignore
mdx_page!(
    "/blog/hello",
    "content/hello.mdx",
    overrides = {
        "a" => components::custom_link,
        "h1" => components::heading,
    }
);
```

The following HTML elements can be overridden: `a`, `h1` through `h6`, `img`, `pre`, and `hr`. When a link element is overridden, URL safety checks run before the override component is invoked.

## Wrapper

Pass `wrapper = Path` to wrap the compiled content in a layout component:

```rust,ignore
mdx_page!(
    "/blog/hello",
    "content/hello.mdx",
    wrapper = components::blog_layout
);
```

The wrapper receives the compiled content as a `child` prop.

# Features

The following features are available when compiling the page.

## Heading IDs

Each heading element receives an `id` attribute generated from its text content. Duplicate headings get `-1`, `-2` suffixes. When combined with an `h1` through `h6` override, the component receives the `id` attribute as input.

## Reference links

Reference-style links and images are resolved from definition declarations in the document. Unknown references produce a compile error.

## Footnotes

Footnote definitions are collected and rendered as a numbered section at the end of the document. References become superscript links with backlinks.

## Code block meta

Fenced code block meta strings are parsed and attached as `data-*` attributes on the `<pre>` element: `data-lang`, `data-lines`, `data-title`, and `data-emphasis`.

## Excerpt extraction

When the file contains `<!-- more -->` as a text node, content before that marker is treated as the excerpt. Use `mdx_pages!` to access excerpt data through its content indexer.

# File Extensions

Files with the `.mdx` extension support embedded component tags. Files with the `.md` extension are parsed as plain markdown with no component support. Both extensions are accepted.

[`compile_mdx!`]: macro.compile_mdx.html
