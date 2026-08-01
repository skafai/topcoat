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

# Syntax Support

Topcoat MDX uses `markdown-rs` as its parser, which supports CommonMark and the following GFM extensions:

- Tables
- Strikethrough
- Task lists
- Autolinks

HTML passthrough is disabled. Raw HTML blocks and inline HTML tags are not rendered. This is a security measure: raw `<script>` and `<iframe>` elements cannot slip through the MDX pipeline. Only `<` tokens dispatched to MDX JSX productions are processed.

Files with the `.mdx` extension allow embedded component tags. Files with the `.md` extension are parsed as plain markdown with no component support. Both extensions are accepted by `compile_mdx!`, `mdx_page!`, and `mdx_pages!`.

# Component Embedding

Use `mdx_components!` to declare a registry of component mappings. Each entry pairs an identifier with a Rust component path. When the parser encounters `<Callout>` in an `.mdx` file, it renders the mapped component.

```rust,ignore
mdx_components! {
    Callout => components::callout,
    Divider => components::divider,
}
```

Pass the registry as the first argument to `compile_mdx!`. See the [`mdx_components!`] reference for syntax details, props, children, and self-closing tag support.

Components registered via `mdx_components!` can also be discovered automatically when the `discover` feature is enabled on the `topcoat-mdx` crate. Register the feature and use `Router::builder().discover()` to pick them up:

```toml
topcoat = { version = "0.5.0", features = ["mdx-discover"] }
```

# Frontmatter

MDX files can carry YAML or TOML frontmatter at the top of the document. The [`mdx_page!`] macro deserializes frontmatter at compile time and stores the result in the request extensions. Read it in your handler with `Frontmatter<T>`:

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
    Frontmatter(meta): Frontmatter<BlogMeta>,
) -> topcoat::Result {
    view! { cx => <h1>(meta.title)</h1> }
}
```

`Frontmatter<T>` implements `Deref<Target = T>`, so you can access the inner value directly. It also implements `FromRequest`, making it a zero-cost extractor when used with `#[page]` handlers backed by `mdx_page!`.

# Route Registration

The [`mdx_page!`] macro compiles a single file and registers it as a route handler. The [`mdx_pages!`] macro walks a directory tree, compiles every `.mdx` and `.md` file, and registers a handler per file with kebab-case slugs derived from the filename.

Both macros accept optional `frontmatter`, `components`, `overrides`, and `wrapper` arguments. See the macro reference docs for details.

[`mdx_components!`]: macro.mdx_components.html
[`mdx_page!`]: macro.mdx_page.html
[`mdx_pages!`]: macro.mdx_pages.html
[`compile_mdx!`]: macro.compile_mdx.html
