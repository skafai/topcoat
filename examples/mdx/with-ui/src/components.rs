use topcoat::{
    Result,
    view::{view, View, component},
};

/// A card container with a title and child content.
#[component]
pub async fn card(title: &'static str, #[default] child: View) -> Result {
    view! {
        <div class="rounded-xl border bg-card p-6 shadow-sm">
            <h2 class="text-xl font-semibold">(title)</h2>
            <div class="mt-4">(child)</div>
        </div>
    }
}

/// A small badge label with child content.
#[component]
#[allow(dead_code)]
pub async fn badge(#[default] child: View) -> Result {
    view! {
        <span class="inline-flex items-center rounded-full border px-2.5 py-0.5 text-xs font-medium">
            (child)
        </span>
    }
}

/// A call-to-action button with an icon.
#[component]
pub async fn cta_button(label: &'static str) -> Result {
    use topcoat::icon::{icon, iconify::iconify_icon};
    view! {
        <button class="inline-flex items-center justify-center rounded-md text-sm font-medium \
            h-10 px-4 py-2 bg-primary text-primary-foreground">
            (label)
            icon(data: iconify_icon!("feather:arrow-right"))
        </button>
    }
}
