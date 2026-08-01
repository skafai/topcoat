Topcoat MDX compiles `.mdx` and `.md` files at build time into `view!` AST nodes. Content authors write markdown with embedded Topcoat components, and the `compile_mdx!` macro reads the file, parses it with `markdown-rs`, walks the syntax tree into `view!` nodes, and emits tokens. There is zero runtime parsing overhead.

```rust,ignore
use topcoat::{mdx::{compile_mdx, mdx_components}, router::page, view::view};

#[page("/blog/hello")]
async fn hello_page() -> topcoat::Result {
    view! {
        compile_mdx!(
            mdx_components! {
                Callout => components::callout,
            },
            "content/hello.mdx"
        )
    }
}
```

# Setup

Enable the `mdx` feature on the `topcoat` facade crate:

```toml
topcoat = { version = "0.5.0", features = ["mdx"] }
```

The `compile_mdx!` macro resolves file paths relative to `CARGO_MANIFEST_DIR`. The path argument must be a string literal so the macro can read the file at compile time.

# MDX Syntax

The parser supports CommonMark and GFM extensions including tables, strikethrough, task lists, and autolinks. HTML passthrough is disabled so that only component tags are processed through the MDX JSX path.

See the [`compile_mdx!`][compile_mdx] reference for the full list of supported features: reference links, footnotes, heading IDs, code block meta strings, and excerpt extraction.

# Component Embedding

Use [`mdx_components!`][mdx_components] to declare a registry of component mappings. Each entry pairs an identifier with a Rust component path. When the parser encounters a matching tag in an `.mdx` file, it renders the mapped component.

```rust,ignore
use topcoat::mdx::mdx_components;

let registry = mdx_components! {
    Callout => crate::components::callout,
    Divider => crate::components::divider,
};
```

Component tags receive props from attribute syntax and children from body content. Self-closing tags like `<Divider />` are supported. See the [`mdx_components!`][mdx_components] reference for syntax details.

# Frontmatter

MDX files can carry YAML or TOML frontmatter. The [`mdx_page!`][mdx_page] macro deserializes it at compile time and stores the result in the request extensions. Read it with the `Frontmatter<T>` extractor:

```rust,ignore
use serde::Deserialize;
use topcoat::mdx::Frontmatter;

#[derive(Deserialize)]
struct BlogMeta {
    title: String,
    date: String,
}

#[page("/blog/hello")]
async fn hello_page(
    cx: &Cx,
    meta: Frontmatter<BlogMeta>,
) -> topcoat::Result {
    view! { cx => <h1>(meta.title)</h1> }
}
```

`Frontmatter<T>` implements `Deref<Target = T>` and `FromRequest`, making it a zero-cost extractor when used with `#[page]` handlers backed by `mdx_page!`.

# Routes

The [`mdx_page!`][mdx_page] macro compiles a single file and registers it as a route. The [`mdx_pages!`][mdx_pages] macro walks a directory, compiles every `.mdx` and `.md` file, and registers a handler per file with kebab-case slugs derived from the filename. Both macros accept optional `components`, `overrides`, and `wrapper` arguments.

```rust,ignore
use topcoat::mdx::mdx_pages;

mdx_pages!("content/blog", prefix = "/blog");
```

`mdx_pages!` also emits a content index: a `&'static [MdxIndexEntry]` const named `MDX_INDEX_{DIR}` and an accessor function `mdx_index_{dir}()` for building blog listings and tag pages.

# HTML Element Overrides

Both `mdx_page!` and `mdx_pages!` accept `overrides = { ... }` arguments that replace HTML elements with components. Content authors write normal markdown; the framework renders the elements through your components. This enables custom link handling, heading anchors, code block rendering, and more.

```rust,ignore
use topcoat::mdx::mdx_page;

mdx_page!(
    "/blog/hello",
    "content/hello.mdx",
    overrides = {
        "a" => crate::components::custom_link,
        "h1" => crate::components::heading,
    }
);
```

See the [`compile_mdx!`][compile_mdx] reference for the full list of overridable elements.

# File Extensions

Files with the `.mdx` extension support embedded component tags. Files with the `.md` extension are parsed as plain markdown with no component support. Both extensions are accepted by `compile_mdx!`, `mdx_page!`, and `mdx_pages!`.

# Discover

When the `discover` feature is enabled on the `topcoat-mdx` crate, `mdx_components!` automatically submits each mapping to a global inventory. Use `Router::builder().discover()` to pick up component registrations along with pages registered by `mdx_page!` and `mdx_pages!`.

```toml
topcoat = { version = "0.5.0", features = ["mdx-discover"] }
```

# Macro Reference

- [`compile_mdx!`][compile_mdx] -- Compile a `.mdx` or `.md` file into `view!` AST nodes
- [`mdx_page!`][mdx_page] -- Register a single `.mdx` file as a page route
- [`mdx_pages!`][mdx_pages] -- Scan a directory and register each file as a page route
- [`mdx_components!`][mdx_components] -- Declare a component registry mapping tag names to Rust paths

[compile_mdx]: crate::mdx::compile_mdx
[mdx_page]: crate::mdx::mdx_page
[mdx_pages]: crate::mdx::mdx_pages
[mdx_components]: crate::mdx::mdx_components
