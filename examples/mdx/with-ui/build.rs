fn main() {
    // Render Tailwind CSS from our input stylesheet.
    topcoat::tailwind::BuildConfig::new()
        .input("styles.css")
        .render()
        .unwrap();

    // Stage the Feather icon set for the `iconify_icon!` references.
    topcoat::icon::iconify::BuildConfig::new()
        .icon_set("feather")
        .stage()
        .unwrap();
}
