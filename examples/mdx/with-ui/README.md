# MDX with UI

This example demonstrates how to combine **MDX content** with **Topcoat UI** pages.

The blog index (`/blog`) is a `view!` built from UI components (cards, badges, headers). The individual posts are MDX files compiled by `mdx_pages!` and served alongside the UI pages under the `/blog/` prefix.

It demonstrates:

- UI components (cards, badges) for page layouts;
- MDX posts for content;
- `mdx_pages!` to register a directory of MDX files as routes;
- Frontmatter excerpts displayed in the UI blog index;
- Tailwind CSS styling;
- Fontsource web fonts (Geist);
- Iconify icons (Feather);
- `module_router!().discover()` to wire module-derived and explicit-path routes together.

## Prerequisites

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

- The home page (`/`) shows a styled introduction with UI badges.
- The blog index (`/blog`) displays post cards built from UI components, with titles, dates, and excerpts from frontmatter.
- Clicking a card navigates to an individual MDX post (`/blog/intro`, `/blog/getting-started/install`, etc.).

## How it works

- `src/components/` contains the vendored UI component source (cards, badges, etc.).
- `posts/` contains the MDX blog posts with YAML frontmatter.
- `styles.css` defines the theme and its design tokens.
- `build.rs` generates the Tailwind stylesheet and stages the Feather icons.
- `module_router!()` derives the home page (`/`) and blog index (`/blog`) from their module paths.
- `.discover()` picks up the `mdx_pages!` inventory registrations for the individual posts.
- `AssetBundle::load()` loads the generated assets (fonts, stylesheets).
- `mdx_index_posts()` reads frontmatter excerpts into the UI blog index.
