// The icon font and the window icon are NOT copied into this folder. They are found through Slint include paths:
//   <repo>/src/ThinkVisionControl.Avalonia/Assets/Fonts/FluentSystemIcons-Regular.ttf  (import "FluentSystemIcons-Regular.ttf")
//   <repo>/assets/brand/png/app-icon-32.png                                            (@image-url("app-icon-32.png"))
// <repo> defaults to five levels up (docs/research/lightweight-ui-probes/lookalike/slint -> repo root).
// Set TVC_REPO=<repo root> when building a copy somewhere else.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let repo = std::env::var("TVC_REPO")
        .map(PathBuf::from)
        .unwrap_or_else(|_| manifest.join("../../../../.."));
    let fonts = repo.join("src/ThinkVisionControl.Avalonia/Assets/Fonts");
    let pngs = repo.join("assets/brand/png");
    assert!(fonts.join("FluentSystemIcons-Regular.ttf").exists(), "icon font not found under {fonts:?}; set TVC_REPO");
    println!("cargo:rerun-if-env-changed=TVC_REPO");

    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![fonts, pngs]);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
