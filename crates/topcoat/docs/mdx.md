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

The parser supports `CommonMark` and GFM extensions including tables, strikethrough, task lists, and autolinks. HTML passthrough is disabled so that only component tags are processed through the MDX JSX path.

See the [`compile_mdx!`][compile_mdx] reference for the full list of supported features: reference links, footnotes, heading IDs, and code block meta strings.

# Component Embedding

Use [`mdx_components!`][mdx_components] to declare a registry of component mappings. Each entry pairs an identifier with a Rust component path. When the parser encounters a matching tag in an `.mdx` file, it renders the mapped component.

```text
mdx_components! {
    Callout => crate::components::callout,
    Divider => crate::components::divider,
}
```

Component tags receive props from attribute syntax and children from body content. Self-closing tags like `<Divider />` are supported. See the [`mdx_components!`][mdx_components] reference for syntax details.

# Frontmatter

MDX files can carry YAML or TOML frontmatter, delimited by `---` for YAML or `+++` for TOML. It never renders as content.

```mdx
---
title: Hello
date: "2024-01-01"
tags:
  - rust
---

# Hello
```

[`mdx_pages!`][mdx_pages] reads the `title`, `date`, `excerpt`, and `tags` fields of every scanned file into a compile-time index, reachable through a generated accessor:

```rust,ignore
use topcoat::{Result, mdx::mdx_pages, router::page, view::view};

mdx_pages!("posts", prefix = "/blog");

#[page("/blog")]
async fn blog_index() -> Result {
    view! {
        <ul>
            for post in mdx_index_posts() {
                <li><a href=(post.path)>(post.title.unwrap_or(post.slug))</a></li>
            }
        </ul>
    }
}
```

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

Both extensions are parsed with the same MDX grammar; the extension is a naming convention, not a parser switch. Component tags work in `.md` files too, and MDX syntax rules (such as `{/* text */}` comments) apply to both. Both extensions are accepted by `compile_mdx!`, `mdx_page!`, and `mdx_pages!`.

# Discover

`mdx_page!` and `mdx_pages!` register each file as a page route in the link-time inventory. Enable the `discover` feature and call `Router::builder().discover()` to mount them, so adding a file to a scanned directory is enough to publish a route.

```toml
topcoat = { version = "0.5.0", features = ["mdx", "discover"] }
```

Component registries are read from the macro invocation at compile time, so they are declared per call rather than globally.

# Macro Reference

- [`compile_mdx!`][compile_mdx] -- Compile a `.mdx` or `.md` file into `view!` AST nodes
- [`mdx_page!`][mdx_page] -- Register a single `.mdx` file as a page route
- [`mdx_pages!`][mdx_pages] -- Scan a directory and register each file as a page route
- [`mdx_components!`][mdx_components] -- Declare a component registry mapping tag names to Rust paths

[compile_mdx]: crate::mdx::compile_mdx
[mdx_page]: crate::mdx::mdx_page
[mdx_pages]: crate::mdx::mdx_pages
[mdx_components]: crate::mdx::mdx_components
