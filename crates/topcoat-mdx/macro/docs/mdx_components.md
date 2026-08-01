The `mdx_components!` macro produces a component registry mapping MDX tag names to Rust component paths. It is a `macro_rules!` that emits a braced block consumed by `compile_mdx!`, `mdx_page!`, and `mdx_pages!`. When the `discover` feature is enabled, it also submits each mapping to a global inventory.

```rust
use topcoat::mdx::mdx_components;

let registry = mdx_components! {
    Callout => crate::components::callout,
    Divider => crate::components::divider,
};
```

# Syntax

```text
mdx_components! { TagName => path::to::component, }
```

Each entry maps an identifier to a Rust component path. The identifier becomes the tag name recognized in `.mdx` files.

## Trailing commas

Trailing commas are supported:

```rust
use topcoat::mdx::mdx_components;

let registry = mdx_components! {
    Callout => crate::components::callout,
    Divider => crate::components::divider,
};
```

## Qualified paths

Component paths can be fully qualified:

```rust
use topcoat::mdx::mdx_components;

let registry = mdx_components! {
    Callout => crate::ui::blog::callout::Callout,
    Admonition => super::components::Admonition,
};
```

# Usage with `compile_mdx!`

Pass the macro invocation directly as the first argument to `compile_mdx!`:

```rust,ignore
use topcoat::{mdx::{compile_mdx, mdx_components}, router::page, view::view};

#[page("/blog/post")]
async fn post_page() -> topcoat::Result {
    view! {
        compile_mdx!(
            mdx_components! {
                Callout => components::callout,
                Divider => components::divider,
            },
            "content/post.mdx"
        )
    }
}
```

# Component Props

When the parser encounters a component tag in the `.mdx` file, attribute syntax becomes component props:

```mdx
<Callout type="info" title="Note">
This is the callout body content.
</Callout>
```

The `type` and `title` attributes are passed as props to the `callout` component. The body content is passed as the `child` prop.

# Self-Closing Tags

Self-closing component tags are supported:

```mdx
<Divider />
```

This renders the `divider` component with no children.

# Inventory Discovery

When the `discover` feature is enabled on the `topcoat-mdx` crate, `mdx_components!` automatically submits each mapping to a global inventory. The `mdx_pages!` macro discovers these registrations when compiling pages from a directory scan, so you do not need to pass `components = {...}` to every page.

Enable discovery in your `Cargo.toml`:

```toml
topcoat = { version = "0.5.0", features = ["mdx-discover"] }
```

After enabling, register your components once and use `Router::builder().discover()` to pick them up along with pages registered by `mdx_page!` and `mdx_pages!`.

# Type

`mdx_components!` is a `macro_rules!` macro, not a procedural macro. The braced block it produces is valid Rust syntax parsed by the procedural macro that consumes it. The `MdxComponentMapping` type holds the inventory entries when discovery is active.

[`compile_mdx!`]: macro.compile_mdx.html
[`MdxComponentMapping`]: struct.MdxComponentMapping.html
