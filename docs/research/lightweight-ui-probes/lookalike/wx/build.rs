// Windows: embed the common-controls v6 + per-monitor-v2 DPI manifest (wx requires comctl6) and the app icon.
fn main() {
    #[cfg(windows)]
    {
        let mut res = winresource::WindowsResource::new();
        res.set_manifest(
            r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">
  <dependency><dependentAssembly><assemblyIdentity type="win32" name="Microsoft.Windows.Common-Controls"
    version="6.0.0.0" processorArchitecture="*" publicKeyToken="6595b64144ccf1df" language="*"/></dependentAssembly></dependency>
  <application xmlns="urn:schemas-microsoft-com:asm.v3"><windowsSettings>
    <dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">true/pm</dpiAware>
    <dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">PerMonitorV2</dpiAwareness>
  </windowsSettings></application>
</assembly>"#,
        );
        // wxMSW frames use the "wxICON_AAA" resource as their default icon.
        let ico = std::env::var("TVC_ICO").unwrap_or_else(|_| "../../../../../assets/brand/ico/ThinkVisionControl.ico".into());
        if std::path::Path::new(&ico).exists() {
            res.set_icon_with_id(&ico, "wxICON_AAA");
        }
        res.compile().expect("resource");
    }
}
