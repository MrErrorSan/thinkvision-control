// The window icon lives with the brand assets; the icon font is under ui/fonts.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let brand = manifest.join("../../assets/brand/png");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![brand]);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();
}
