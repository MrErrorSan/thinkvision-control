// The window icon lives with the brand assets; the icon font is under ui/fonts.
// On Windows the .exe also embeds the app icon and version info: the Start menu, desktop shortcut, taskbar and
// Explorer take the icon from the file, not from the window.
use std::path::PathBuf;

fn main() {
    let manifest = PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").unwrap());
    let assets = manifest.join("../../assets/brand");
    let config = slint_build::CompilerConfiguration::new()
        .with_style("fluent".into())
        .with_include_paths(vec![assets.join("png")]);
    slint_build::compile_with_config("ui/main.slint", config).unwrap();

    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let icon = assets.join("ico/ThinkVisionControl.ico");
        println!("cargo:rerun-if-changed={}", icon.display());
        winresource::WindowsResource::new()
            .set_icon(icon.to_str().unwrap())
            .set("ProductName", "ThinkVision Control")
            .set("FileDescription", "ThinkVision Control")
            .set("LegalCopyright", "GPL-2.0-or-later")
            .compile()
            .expect("embed the Windows icon and version info (needs rc.exe from the Windows SDK)");
    }
}
