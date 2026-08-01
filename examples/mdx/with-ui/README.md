# MDX with UI

This example shows how to embed Topcoat UI components inside MDX content.

It demonstrates:

- Tailwind CSS styling;
- Fontsource web fonts (Geist);
- Iconify icons (Feather);
- Rust components used inline with markdown.

## Prerequisites

This example uses:

- Topcoat MDX;
- Topcoat UI;
- Tailwind CSS;
- Fontsource;
- Iconify;
- generated asset bundles.

Install the local Topcoat CLI from the repository root if it is not already installed:

```sh
cargo install --path crates/topcoat-cli --locked
```

The first build requires an internet connection to download the font and Feather icon set.

## Run the example

From the repository root, enter the example directory:

```sh
cd examples/mdx/with-ui
```

Start the development server:

```sh
topcoat dev
```

The application is served by default at:

```text
http://127.0.0.1:3000
```

## Expected result

Open the application in your browser.

The page should display styled card components rendered from `pages/index.mdx`, using the Geist font and Tailwind CSS.

## How it works

- `build.rs` generates the Tailwind stylesheet and stages the Feather icons.
- `AssetBundle::load()` loads the generated assets.
- `fontsource_font!(GEIST, host: Asset)` self-hosts the Geist font.
- `tailwind::stylesheet!()` returns the generated stylesheet URL.
- `compile_mdx!()` renders MDX content with embedded Rust components.
- `Router::builder().discover().build()` auto-registers pages and assets.
