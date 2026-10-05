# Cross-platform UI framework for porting ThinkVision Control (WPF, .NET 10) to macOS and Linux

Status: research notes, current as of 2026-10-05. Items marked **[unverified]** or **[preview]** need confirmation before they are relied on.

**Local measurements.** Some findings come from a probe I built and ran on this machine: Windows 11 Home 26200, .NET SDK 10.0.401, Avalonia 12.1.3, SkiaSharp 3.119.4, Ryzen 5 5500U with hybrid AMD iGPU + GTX 1650. The probe sources are in `footprint_probe/` (Avalonia and WPF baselines), and the raw rows are in `ui_framework_data.json`. They are cited as [Local measurement](ui_framework_data.json). The results come from one machine, so read them as relative, not universal. "WS" means working set and "Private" means `PrivateMemorySize64` (private commit).

**The app being ported** (counted from `src/` on branch `feature/thinkvision-control`): 13 XAML files (1,508 lines), including a 618-line `Ui/Controls/Controls.xaml` of custom templates. The XAML has 67 `*Trigger` elements, and the C# has 30 `DependencyProperty.Register` calls. P/Invoke covers the tray icon (`Tray/TrayIcon.cs`), tray anchoring (`Platform/TrayAnchor.cs`), the flyout and OSD windows, hotkeys, efficiency mode and the monitor-arrival watcher. The csproj sets `UseWPF` and `UseWindowsForms`. The DDC/CI logic is already separated into `ThinkVisionControl.Core`.

---

## Q1. Avalonia's current state: versions, .NET 10, Wayland, licensing

### Takeaway
Avalonia 12.1.3 (22 Sep 2026) is the current stable release. It targets .NET 8+ and recommends .NET 10. The core and the new Wayland backend are MIT, but Wayland is opt-in and experimental. Paid tiers only add tooling and premium controls that this app does not need.

### Cited Findings
- Avalonia 12 dropped .NET Framework and .NET Standard. "Only .NET 8 and later supported", and **.NET 10 is the recommended target** — [Avalonia 12 breaking changes](https://docs.avaloniaui.net/docs/avalonia12-breaking-changes).
- Avalonia 12.0 was announced on **7 April 2026** as a release about "performance, stability, and platform maturity". It brings compiled bindings by default, a reworked compositor with "idle CPU reduced significantly", themeable client-side window decorations, a native Linux accessibility backend (AT-SPI2), and a rename of `SystemDecorations` to `WindowDecorations`. Upgrading is described as taking "hours, not days" — [Avalonia 12 blog](https://avaloniaui.net/blog/avalonia-12). (A NuGet-mirror search summary gave 9 April for 12.0.0. The difference is small, and I treat the blog date as authoritative.)
- 12.0 Preview 1 shipped on 18 Feb 2026 — [Avalonia 12.0 Preview 1](https://avaloniaui.net/blog/avalonia-12.0-preview-1).
- GitHub releases as of 2026-10-05: **12.1.3 (2026-09-22)** and 12.1.2 (2026-09-02). The 11.3.x line is still being patched, most recently with 11.3.22 (2026-09-11) — [AvaloniaUI/Avalonia releases](https://github.com/AvaloniaUI/Avalonia/releases), read via the `gh api`.
- **Avalonia 12.1 (8 July 2026)**: "Avalonia 12.1 lands native Wayland support". The backend "graduated from private preview" but is **experimental**. It is "not picked up by `UsePlatformDetect()` yet" and needs the `Avalonia.Wayland` package plus `UseWayland()`. It supports "mouse, touch, keyboard, clipboard, drag-and-drop" and renders through EGL with an optional dmabuf path. "KDE-specific niceties (global app menu, window icons, blur-behind) are planned as follow-ups". The release also added an `Avalonia.WinUI` package for embedding Avalonia in WinUI 3 — [What's new in Avalonia 12.1](https://avaloniaui.net/blog/release-12-1).
- `Avalonia.Wayland` on NuGet is **MIT**, with 12.1.0 (2026-07-09) through 12.1.3 (2026-09-22) — [NuGet Avalonia.Wayland](https://www.nuget.org/packages/avalonia.wayland/). In Sept 2025 Avalonia had been "considering … dual-licensing the Wayland backend … AGPL, with commercial licensing available through Avalonia Accelerate" — [Bringing Wayland Support to Avalonia](https://avaloniaui.net/blog/bringing-wayland-support-to-avalonia). The NuGet license shows the shipped package ended up MIT.
- Pricing page: the core framework is "MIT licensed", free, with "70+ built-in controls". **Plus** costs €299/yr per seat (VS integration, DevTools, Parcel packaging, Hot Reload). **Pro** costs €899/yr per seat (Media Player, TreeDataGrid, PDF viewer and other premium controls). **Enterprise** starts at €21,000/yr — [Avalonia pricing](https://avaloniaui.net/pricing).
- Wayland's own constraints, as stated by the Avalonia team: "Need to position a window? The core protocol doesn't allow it." System tray is "the StatusNotifierItem protocol on KDE, but GNOME doesn't support it at all." Popup placement "varies between compositors". "Some compositors want us to draw our own window decorations" — [Bringing Wayland Support to Avalonia](https://avaloniaui.net/blog/bringing-wayland-support-to-avalonia).
- In the source, the native Wayland `WindowBaseImpl` has **no-op** `Position`, `Activate()` and `SetTopmost()`, with the comment "no-op platform methods that Wayland doesn't support (Move, SetTopmost, Activate, PointToClient/Screen, etc.)". `WindowImpl.Move()` is empty ("Not supported by Wayland — surfaces have no screen position") — [WindowImplBase.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/WindowImplBase.cs), [WindowImpl.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/WindowImpl.cs).
- The Wayland backend README says compositor crash and restart is "EXPECTED", that Avalonia keeps persistent surface state so it can reconnect, and that a 20 FPS "fallback" render timer is still present — [Avalonia.Wayland README](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/README.md).

### Inferences
- Pin to Avalonia **12.1.x**. FluentAvalonia 3.x requires it (see Q2), and 12.1 is the first release with any native Wayland support.
- On Linux, ship with the default **X11 backend (XWayland on Wayland sessions)** for now. The OSD overlay and the tray-anchored flyout both need absolute positioning and topmost, and the native Wayland backend currently ignores both. Native Wayland should stay an opt-in or experimental switch.
- No paid licence is needed. The only paid items that would help are Parcel (packaging and notarisation) and the VS designer, and both are conveniences.

### Gaps
- No official Avalonia statement on whether or when Wayland becomes the default in `UsePlatformDetect()`.
- I did not verify how XWayland windows behave for topmost or position on GNOME Mutter 47+ versus KWin 6. Compositors may restrict even X11 clients, and this needs a test on real sessions.
- No Avalonia support or LTS policy for 11.x was found. Patches continue in practice.

---

## Q2. Windows 11 fidelity: Fluent theme, Mica/acrylic, macOS vibrancy, FluentAvalonia

### Takeaway
Mica and acrylic work on Windows 11 through `TransparencyLevelHint`. On macOS, `AcrylicBlur` maps to a real `NSVisualEffectView` blur, but Mica is Windows-only. FluentAvalonia 3.1.0 (Aug 2026) supports Avalonia 12.1 and is widely used. An Avalonia app can look very close to Windows 11, but it draws its own controls, so it is a re-creation rather than real WinUI.

### Cited Findings
- On Windows, `Transparent` works on Windows 7+, `AcrylicBlur` ("Blurred, semi-transparent backdrop") on Windows 10 1803+, and `Mica` ("System-tinted material drawn by DWM") on Windows 11. You set `TransparencyLevelHint="Mica"` with `Background="Transparent"`, and the framework "falls back through the list in order". `ExtendClientAreaToDecorationsHint` with `WindowDecorationProperties.ElementRole="TitleBar"` gives native drag and double-click-to-maximise. `FluentTheme` "automatically switches between light and dark variants to match the system preference". On Windows 11 the framework "automatically updates the native title bar to match the application's RequestedThemeVariant". Windows apps are per-monitor DPI aware by default — [Avalonia Windows guide](https://docs.avaloniaui.net/docs/platform-specific-guides/windows).
- `Mica`: "based on desktop wallpaper tint with a blur. This will only work on Windows 11". `AcrylicBlur`: "blur-behind with a high blur radius. This level may fallback to Blur" — [WindowTransparencyLevel API](https://docs.avaloniaui.net/api/avalonia/controls/windowtransparencylevel).
- **macOS:** a search summary claimed "macOS supports only Transparent". The **source contradicts it**. `Avalonia.Native` maps `WindowTransparencyLevel.AcrylicBlur` → `AvnWindowTransparencyMode.Blur` — [TopLevelImpl.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Native/TopLevelImpl.cs). That mode shows an `NSVisualEffectView` with `NSVisualEffectBlendingModeBehindWindow` and material `NSVisualEffectMaterialHUDWindow` (macOS 10.14+) — [AutoFitContentView.mm](https://github.com/AvaloniaUI/Avalonia/blob/master/native/Avalonia.Native/src/OSX/AutoFitContentView.mm). The material is fixed: there is no Avalonia API to pick `.sidebar`, `.menu` or `.popover`.
- **Linux:** blur-behind on KDE Wayland is listed as a planned follow-up, not shipped — [Avalonia 12.1](https://avaloniaui.net/blog/release-12-1).
- **FluentAvalonia** (amwx), a "control library focused on fluent design and bringing more WinUI controls into Avalonia", MIT — [GitHub amwx/FluentAvalonia](https://github.com/amwx/FluentAvalonia).
  - NuGet `FluentAvaloniaUI` release history: **3.1.0 (2026-08-22)**, 3.0.2 (2026-07-14), 3.0.0 (2026-06-20), with previews from 2026-04-12. 3.1.0 depends on **Avalonia >= 12.1.0** and targets net10.0. It has 1.3 M total downloads and is used by SteamTools, StabilityMatrix and GalaxyBudsClient — [NuGet FluentAvaloniaUI](https://www.nuget.org/packages/FluentAvaloniaUI), [3.1.0](https://www.nuget.org/packages/FluentAvaloniaUI/3.1.0).
  - The GitHub README still shows v2.3.0 / Avalonia 11.2.5 as "current". The author cites "limited time", and the GitHub Releases page is empty, so releases happen through NuGet only — [GitHub](https://github.com/amwx/FluentAvalonia), [Releases](https://github.com/amwx/FluentAvalonia/releases). The repo has 1.6k stars and 35 open issues.

### Inferences
- The WPF app already hand-builds its Fluent look in `Controls.xaml` and `Theme.xaml`. Avalonia's `FluentTheme` with FluentAvalonia's `NavigationView`, `SettingsExpander` and `InfoBar` would give at least the same fidelity, probably better, because those are direct WinUI ports.
- What will not be identical on Windows: the system context menu and tray menu (Avalonia's native tray menu is a Win32 menu, as WPF's is now), text rendering (Skia, not DirectWrite/ClearType), and focus visuals. Users comparing it with Settings may notice slight font-rendering differences. This is an inference: I found no side-by-side study.
- FluentAvalonia is a one-maintainer project. It has kept pace with 12.x. FA 3.0 previews started five days after Avalonia 12.0 (12 Apr vs 7 Apr), FA 3.0.0 stable followed about 10 weeks later (20 Jun), and FA 3.1.0 (22 Aug, requiring Avalonia >= 12.1.0) came about 6 weeks after Avalonia 12.1 (8 Jul). That is a dependency risk, but a manageable one, because only a few controls would be used.

### Gaps
- **[unverified]** I did not fetch FluentAvalonia 3.x's exact control list in this session. Historically it includes NavigationView, SettingsExpander, InfoBar, ContentDialog, TaskDialog, Frame, CommandBar and an `AppWindow` with Mica/title-bar integration, but whether all of these survived 3.0 is unconfirmed. Check the FAControlsGallery before committing.
- **[unverified]** Whether Avalonia's `FluentTheme` picks up the Windows accent colour automatically. The fetched docs only mention light/dark switching.
- **[unverified]** Whether Mica still renders when `Win32RenderingMode.Software` is used (relevant to the memory findings in Q4). My screenshot attempt failed.

---

## Q3. Tray / menu-bar icon, flyout and overlay on each OS

### Takeaway
`TrayIcon` + `NativeMenu` are built in. On Linux they use DBus StatusNotifierItem. GNOME needs the AppIndicator extension (preinstalled on Ubuntu, Fedora needs it added), and KDE works out of the box. macOS menu-bar-only mode works through `MacOSPlatformOptions.ShowInDock = false`, not through `LSUIElement`. The tray-anchored flyout and the OSD need per-OS positioning code, and on native Wayland they cannot be positioned at all.

### Cited Findings
- TrayIcon support: Windows "Full support", macOS "Full support", Linux "Works on distributions with StatusNotifierItem or AppIndicator support (confirmed on Ubuntu)". "GNOME users may need the AppIndicator extension." Click semantics differ. On macOS "clicking the tray icon shows the menu". On Windows "right-clicking shows the menu and left-clicking fires the Command" — [Avalonia TrayIcon docs](https://docs.avaloniaui.net/controls/navigation/trayicon).
- The Linux implementation registers `org.kde.StatusNotifierItem-{pid}-{tid}` over DBus — [DBusTrayIconImpl.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.FreeDesktop/DBusTrayIconImpl.cs). On the native Wayland backend: "org.kde.StatusNotifierItem (works on KDE; on GNOME via the AppIndicator extension). No XEmbed fallback … If the watcher service is not on the bus we return null and the framework falls back to its own no-op handling" — [WaylandTopLevelFactory.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/WaylandTopLevelFactory.cs).
- Libappindicator works on Wayland, "but for GNOME Shell the user needs to install an extension". KDE, Cinnamon and Sway work out of the box — search summary of a Linux tray discussion (secondary; consistent with the Avalonia source comment above).
- A historical bug reported a tray icon not updating on GNOME (Avalonia 0.10.8) — [Lightrun mirror of the issue](https://lightrun.com/answers/avaloniaui-avalonia-trayicon-not-updated-in-gnomelinux). It is old, and its current status is unknown.
- **macOS dock hiding:** setting `LSUIElement` in Info.plist "doesn't work with Avalonia applications", because Avalonia sets the activation policy at runtime. Use `.With(new MacOSPlatformOptions { ShowInDock = false })`, which sets `NSApplicationActivationPolicyAccessory` — [Avalonia discussion #13568](https://github.com/AvaloniaUI/Avalonia/discussions/13568). `ShowInDock` defaults to true — [API docs](https://api-docs.avaloniaui.net/docs/P_Avalonia_MacOSPlatformOptions_ShowInDock).
- macOS `NativeMenu` appends standard items ("Quit App Name" ⌘Q). `CFBundleName` is limited to 15 characters. "Some macOS features require your app to run as a proper .app bundle" — [Avalonia macOS guide](https://docs.avaloniaui.net/docs/platform-specific-guides/macos).
- Native Wayland: no window move or topmost (see Q1). On X11, `Window.Position` and `Topmost` are implemented. This is an inference from the backend split, not separately cited.

### Inferences
- **Tray flyout (quick controls):** on Windows, keep the existing `TrayAnchor` logic (Shell_NotifyIconGetRect) behind an interface. On macOS, `TrayIcon` exposes no icon rectangle in the docs I found, so a click-anchored popover needs either the mouse position at click time (a common workaround) or a small native shim to `NSStatusItem.button.window.frame`. On Linux, SNI `Activate(x, y)` gives coordinates on X11/KDE, but GNOME's AppIndicator extension usually opens the menu instead of activating. **Plan for menu-only quick controls on GNOME.**
- **OSD overlay:** a borderless `Topmost` window with `ShowActivated=false` works on Windows, macOS (it must also avoid stealing focus and should join all Spaces, which may need a native call) and X11/XWayland. On native Wayland a true overlay needs `wlr-layer-shell` (KDE and wlroots support it, GNOME does not), and Avalonia does not expose it. A desktop notification is the fallback.
- **Global hotkeys** are outside Avalonia. On macOS this needs `CGEventTap` or Carbon `RegisterEventHotKey` (Accessibility permission). On X11 it needs `XGrabKey`. On Wayland it needs the `org.freedesktop.portal.GlobalShortcuts` portal (KDE yes; GNOME 48+ has partial support **[unverified]**). This applies to every framework option.

### Gaps
- **[unverified]** Whether Avalonia 12 `TrayIcon` exposes a click position or icon bounds on any OS. Not found in docs.
- **[unverified]** Whether macOS template-image (monochrome, auto-tinting) tray icons are supported. The `TrayIcon` docs do not mention it.
- No measured data on GNOME 47/48 behaviour with Avalonia's SNI (left click vs menu).

---

## Q4. Publishing (single-file, NativeAOT) and resource use

### Takeaway
NativeAOT works for Avalonia 12 on win-x64; I built and ran it. With **default GPU rendering**, Avalonia uses *more* memory than this app's WPF build: about 78 MB private with a window, and about 56 MB private when tray-only (vs WPF's ~12 MB never-shown and ~43 MB shown). With **software rendering + NativeAOT** it is far lighter: about 6 MB private / 21 MB WS tray-only, and about 13 MB private / 36 MB WS with the window shown. Published third-party numbers are sparse and come from low-quality sources.

### Cited Findings
- Avalonia NativeAOT is supported. It requires compiled bindings, trimmer roots for reflection, and compile-time DI. "Some third-party Avalonia controls may not be AOT-compatible" — [Avalonia Native AOT docs](https://docs.avaloniaui.net/docs/deployment/native-aot). The docs give no size, memory or startup numbers.
- A community AOT template claims "~18MB size" with UPX and "~20MB usage" without UPX (no version, OS or method given) — [lixinyang123/AvaloniaAOT](https://github.com/lixinyang123/AvaloniaAOT).
- **Local, Avalonia 12.1.3, Windows 11** — [Local measurement](ui_framework_data.json):

  | Build / mode | WS MB | Private MB | Threads | Startup to HWND (warm) |
  |---|---|---|---|---|
  | Avalonia self-contained JIT, window, GPU | ~120 | ~92 | 53 | ~1.8 s |
  | Avalonia single-file trimmed, window, GPU | ~110–119 | ~91–96 | 53 | ~2.0 s |
  | Avalonia **NativeAOT**, window, GPU | ~77 | ~78 | 50 | **~0.95 s** |
  | Avalonia NativeAOT, window, **software** | **~36** | **~12.5** | 9 | – |
  | Avalonia NativeAOT, **tray only**, GPU | ~51.5 | ~56 | 24 | – |
  | Avalonia NativeAOT, **tray only, software** | **~21** | **~6** | 6 | – |
  | Avalonia single-file trimmed, window, software | ~67 | ~26 | 12 | – |
  | Avalonia single-file trimmed, tray only, GPU | ~80 | ~70 | 27 | – |
  | Avalonia single-file trimmed, tray only, software | ~49 | ~20 | 9 | – |
  | WPF self-contained JIT, same window, GPU | ~85–88 | ~43.5 | 22 | ~0.52 s |

  Publish sizes (excluding .pdb): Avalonia NativeAOT **34.8 MB** in 4 files (exe 16.8, libSkiaSharp 11.1, ANGLE `av_libglesv2` 5.1, HarfBuzz 1.7). Single-file trimmed is 35.8 MB. Untrimmed self-contained is 103.5 MB. WPF untrimmed self-contained is 139.5 MB (WPF does not support trimming).
- Earlier probe of this app's WPF shape on the same machine: never-shown WPF ≈ **12 MB private / 43 MB WS**, shown once ≈ **41–45 MB private / ~90 MB WS** — [resource_footprint.md](../Windows%2011%20design%20for%20WPF%20apps/resource_footprint.md).
- Third-party claims, all seen only as search summaries of aggregator or blog posts, **low confidence**: a minimal Avalonia app at "~20–30MB RAM" vs WPF "~25–35MB"; hello-world start "~0.5s (Windows) vs ~0.8s for WPF"; idle working set "~140 MB" for Avalonia vs "~200 MB" for MAUI; "WPF still wins on cold-start time for simple forms-based apps" — [tutorialpedia](https://www.tutorialpedia.org/blog/how-is-avalonia-to-develop-crossplatform-application/), [ctco.blog](https://www.ctco.blog/posts/maui-vs-avalonia-2026-cross-platform-dotnet-ui/), [wpfsharp.com](https://wpfsharp.com/wpf-vs-avalonia-2026/). The ~0.5 s vs ~0.8 s claim **is contradicted by my measurement**: warm JIT Avalonia took ~1.8 s and WPF ~0.5 s. Only AOT Avalonia (~0.95 s) gets close.
- Avalonia 12 claims an "overall memory footprint reduction noted by testers", with no desktop numbers. Its Android AOT startup went from 1,960 ms to 460 ms — [Avalonia 12 blog](https://avaloniaui.net/blog/avalonia-12).

### Inferences
- For a tray app that idles most of the time, the best configuration is **NativeAOT + software rendering** (`Win32PlatformOptions.RenderingMode = [Software]`; on macOS and Linux check the equivalent `AvaloniaNativePlatformOptions` and `X11PlatformOptions` rendering-mode settings). It idles below this app's WPF build: about 6 vs 12 MB private when tray-only. The UI is sliders, toggles and text, so CPU rendering cost is negligible.
- With default GPU rendering, Avalonia starts the ANGLE/D3D stack **even with no window shown** (~56 MB private, 24 threads, tray-only). WPF only pays this when a window first renders. The "do not create the window at login" lever from the WPF work therefore does not apply to Avalonia on the GPU path. Software rendering fixes it.
- NativeAOT requires all bindings to be compiled (`x:DataType`), no reflection-based DI, and AOT-safe third-party controls. FluentAvalonia's AOT-safety is not confirmed (see Gaps).
- Hybrid-GPU laptops inflate the GPU-path numbers (driver threads). Software rendering also avoids waking the discrete GPU.

### Gaps
- No measurements on macOS (Apple Silicon) or Linux. Metal/OpenGL paths and X11 vs Wayland will differ.
- **[unverified]** FluentAvalonia 3.x under NativeAOT/trimming. There was no statement on NuGet or GitHub.
- **[unverified]** Whether Mica/acrylic work with software rendering on Windows (transparency then goes through a layered or premultiplied software surface).
- macOS `.app` bundle size and notarisation steps for AOT builds were not measured.

---

## Q5. Alternatives: Uno Platform, .NET MAUI, Avalonia XPF, Eto.Forms, Tauri/Electron

### Takeaway
None beats Avalonia for this app. MAUI has no Linux. Uno's Skia desktop has no built-in cross-platform tray and lists only X11/framebuffer for Linux. XPF works but costs €9,500+ per app per year of updates. Eto.Forms is native-widget and lightweight but cannot do a Windows 11 Fluent look. Tauri/Electron add a web stack and, on Windows, WebView2-level memory.

### Cited Findings
- **.NET MAUI** supports Android, iOS, macOS 12+ (Mac Catalyst) and Windows (WinUI 3) for .NET 9/10/11. **There is no Linux.** The only extra platform is Tizen (Samsung) — [MAUI supported platforms](https://learn.microsoft.com/en-us/dotnet/maui/supported-platforms) (updated 2026-09-16).
- **Uno Platform:** Skia desktop (`net10.0-desktop`) on Windows 7+, macOS 10.15+, and Linux where "Supported environments are X11 and Framebuffer". **Wayland is not listed** — [Uno supported platforms](https://platform.uno/docs/articles/getting-started/requirements.html). Uno 6 unified Skia rendering across all platforms — [InfoQ, May 2025](https://www.infoq.com/news/2025/05/uno-platform-6-release). For tray icons the only package found is `H.NotifyIcon.Uno` (dev builds, 2.5.0-dev.2), described as covering "WPF/WinUI/Uno.Skia.WPF/Console". There is **no macOS or Linux tray** — [NuGet H.NotifyIcon.Uno](https://www.nuget.org/packages/H.NotifyIcon.Uno/2.5.0-dev.2).
- **Avalonia XPF**, "a binary-compatible WPF runtime" for Windows, macOS 10.14+ and desktop Linux. It is a perpetual licence **per application** with 12 months of updates: **Internal €9,500, Business €29,500, Enterprise €124,500+**. Limits include no DirectX, `System.Drawing.Common` deprecated off Windows, FlowDocument limits, "Multiple UI threads unsupported on macOS" and no per-pixel hit transparency — [Avalonia XPF](https://avaloniaui.net/xpf). A third-party aggregator lists lower figures (€7,800 / €19,500 / €89,000) — [fitgap](https://us.fitgap.com/products/avalonia). This **conflicts** with the vendor page, which I treat as authoritative.
- **Eto.Forms:** latest release **2.12.0 (2026-09-24)**, with 2.11.0 on 2026-03-02 — [picoe/Eto releases](https://github.com/picoe/Eto/releases), via `gh api`. It has a `TrayIndicator` class (image, menu, title, visibility) — [Eto TrayIndicator API](https://pages.picoe.ca/docs/api/html/T_Eto_Forms_TrayIndicator.htm). (Eto wraps native toolkits: WPF/WinForms on Windows, AppKit on macOS, GTK on Linux. This is general knowledge and was not re-fetched.)
- **Tauri/Electron:** a Tauri issue reports Tauri on WebView2 having "similar memory usage" to Electron. One developer measured ~80 MB (Tauri, Windows) vs ~120 MB (Electron) idle — [tauri-apps/tauri #5889](https://github.com/tauri-apps/tauri/issues/5889), [levminer: Tauri vs Electron](https://www.levminer.com/blog/tauri-vs-electron) (seen as search summaries).

### Inferences
- **Uno:** it has the WinUI API and the best Windows 11 visual fidelity of the cross-platform options, but no Wayland, no cross-platform tray, and a heavier toolchain. A tray-first app would be fighting the framework. It also needs its own tray and overlay shims on macOS and Linux.
- **MAUI:** ruled out by the Linux requirement. Even on macOS, Mac Catalyst is a poor fit for a menu-bar accessory app.
- **XPF:** it is the lowest-effort path (the existing WPF code would run largely unchanged), but €9,500+ per app is out of proportion for a free or small utility. It also inherits WPF's Win32-isms, and the P/Invoke tray, flyout and hotkey code would still need per-OS rewrites.
- **Eto.Forms:** genuinely native and light on macOS and GTK, but its Windows backend is WPF/WinForms with no Fluent controls. The settings UI would look worse on Windows than it does now, and GNOME tray still needs the AppIndicator path.
- **Tauri/Electron + .NET sidecar:** two runtimes plus IPC. Memory is at or above WPF on Windows (WebView2), and Fluent fidelity would come from a web component kit (e.g. Fluent UI Web Components). This only makes sense if a web UI is wanted for other reasons.

### Gaps
- I found no measured idle memory for Uno Skia desktop or Eto.Forms tray apps.
- Uno's Wayland roadmap was not researched in depth.

---

## Q6. Keep WPF on Windows + Avalonia for macOS/Linux, or Avalonia everywhere?

### Takeaway
Avalonia everywhere is recommended. It can look Windows 11-native (Mica, Fluent controls, theme-aware title bar), and with AOT + software rendering it is lighter than the current WPF build. Two UIs would roughly double UI maintenance for a gain of slightly more "real" Windows text rendering.

### Cited Findings
- Avalonia on Windows supports Mica, acrylic, a theme-synced native title bar, per-monitor DPI and automatic light/dark — [Avalonia Windows guide](https://docs.avaloniaui.net/docs/platform-specific-guides/windows).
- FluentAvalonia ports WinUI controls ("bringing more WinUI controls into Avalonia") — [amwx/FluentAvalonia](https://github.com/amwx/FluentAvalonia).
- Avalonia 12 ships a native AT-SPI2 Linux accessibility backend — [Avalonia 12 blog](https://avaloniaui.net/blog/avalonia-12). (The current app exposes UI Automation names, per commit `de01ed9`.)
- Memory: Avalonia NativeAOT + software rendering idles at ~6 MB private tray-only, vs ~12 MB for never-shown WPF — [Local measurement](ui_framework_data.json); [resource_footprint.md](../Windows%2011%20design%20for%20WPF%20apps/resource_footprint.md).

### Inferences
- **Two-UI approach**: the existing WPF app stays untouched on Windows, and a second Avalonia head shares `ThinkVisionControl.Core`. Cost: every page, card, dialog and quick-control change is made twice in two XAML dialects, and the 618-line control-template file is duplicated. The repo already separates Core well, so the shared layer is fine, but the ViewModel layer would need to be framework-neutral (no `System.Windows` types). Benefit: zero regression risk on Windows and DirectWrite text.
- **Avalonia everywhere**: one UI, with platform services (tray anchor, OSD positioning, hotkeys, efficiency mode, device-arrival) behind interfaces with per-OS implementations. Windows keeps its existing P/Invoke code, moved behind those interfaces. Risk: a visible change on Windows (Skia text rendering, slightly different control metrics), and FluentAvalonia becomes a dependency.
- A middle path is to build the Avalonia app, run it on Windows in parallel with WPF for one release, and delete WPF once parity is reached.
- "Can a single Avalonia app feel native on Windows 11?" Visually mostly yes: Fluent theme, Mica, Win32 tray menu, and the theme-synced title bar are all documented. Remaining tells (inference): Skia text rendering, no system-provided `ContentDialog` smoke layer behaviour unless FluentAvalonia supplies it, and focus visuals. Snap layouts on a custom title bar are not documented in what I read.

### Gaps
- No user study or side-by-side screenshots comparing Avalonia Fluent and WinUI 3 or Windows Settings.
- **[unverified]** Windows 11 snap-layout flyout on maximize-button hover with `ExtendClientAreaToDecorationsHint`.

---

## Q7. Porting effort from WPF XAML to Avalonia XAML

### Takeaway
It is moderate and mostly mechanical, except for the 67 triggers and the 618-line custom template file. Triggers have no direct equivalent and must become pseudo-class selectors or class bindings. DependencyProperties map almost one-to-one to `StyledProperty`/`DirectProperty`.

### Cited Findings
- Mapping highlights — [WPF→Avalonia cheat sheet](https://docs.avaloniaui.net/docs/migration/wpf/cheat-sheet):
  - "No triggers in Avalonia". `Style.Triggers` becomes pseudo-classes (`:pointerover`, `:pressed`), `DataTrigger` becomes bindings, classes or converters, and `EventTrigger` becomes animations on pseudo-classes.
  - `DependencyProperty` becomes `StyledProperty` (styleable) or `DirectProperty` (fast, no styling), registered with `AvaloniaProperty.Register<TOwner,TValue>`.
  - `RelativeSource Self`/`AncestorType`/`ElementName` become `$self`, `$parent[Type]` and `#name`. `UpdateSourceTrigger` is "Not needed".
  - `Visibility=Collapsed` becomes `IsVisible=False`, and `Hidden` becomes `Opacity=0`.
  - `ToolTip` becomes the `ToolTip.Tip` attached property. `ListView` becomes `ListBox`. `DataGrid` is a separate package.
  - RoutedCommand and CommandBinding are replaced by `ICommand`. Layout transforms become `LayoutTransformControl`. `DrawingBrush` becomes `VisualBrush`.
  - `.xaml` becomes `.axaml`, and resource URIs use `avares://Assembly/path`.
- Avalonia styling uses a "CSS-like system using selectors, style classes, and pseudo-classes" — [Migrating from WPF](https://docs.avaloniaui.net/docs/migration/wpf/).
- Avalonia 12 makes compiled bindings the default, so `x:DataType` is required on views — [Avalonia 12 blog](https://avaloniaui.net/blog/avalonia-12); [breaking changes](https://docs.avaloniaui.net/docs/avalonia12-breaking-changes).
- Tooling: the free Avalonia for VS Code extension, plus paid Plus (€299/yr) for the Visual Studio designer, DevTools and Hot Reload — [Avalonia pricing](https://avaloniaui.net/pricing). DeveloperTools packages for Avalonia 12 exist on NuGet — [AvaloniaUI.DeveloperTools](https://www.nuget.org/packages/AvaloniaUI.DeveloperTools).

### Inferences
- Rough effort for this codebase: the pages (`MonitorPage`, `DisplayPage`, `AppSettingsPage`, ~265 lines) are a quick port, especially if replaced with FluentAvalonia `SettingsExpander` instead of the custom settings-card templates. `Controls.xaml` (618 lines, which holds most of the triggers) is best **deleted rather than ported**, because FluentTheme and FluentAvalonia already provide those looks. `QuickControlsView` (208 lines) and `OsdWindow`/`FlyoutWindow` port directly, but their code-behind P/Invoke needs per-OS services.
- The 30 `DependencyProperty.Register` calls convert mechanically (`StyledProperty<T>`, with `PropertyChanged` overrides replacing the metadata callbacks).
- The biggest non-XAML work is per-OS platform services, not the UI: tray anchor, OSD placement, hotkeys, device-arrival (IOKit / udev or DBus), and autostart (LaunchAgent / XDG autostart). This is the same under any framework.
- The WinForms dependency (`UseWindowsForms`) must go from any shared project.

### Gaps
- No published effort estimates or case studies for small WPF→Avalonia ports were found in this session.
- `x:Static`, `MultiBinding` and `ControlTemplate`→`ControlTheme` specifics were not covered by the cheat-sheet summary. Check the full [styling migration page](https://docs.avaloniaui.net/docs/migration/wpf/styling).

---

## Q8. Recommendation and risks

### Takeaway
Use **Avalonia 12.1.x everywhere, with FluentAvalonia 3.1 and the MIT core only**. Publish NativeAOT, use software rendering for the tray-idle footprint, use X11/XWayland on Linux for now, and set `ShowInDock=false` on macOS. Keep all OS integration behind interfaces in a platform layer, with the existing Win32 code as the Windows implementation.

### Cited Findings
- Version, licence and .NET 10 fit — [Avalonia 12 breaking changes](https://docs.avaloniaui.net/docs/avalonia12-breaking-changes), [Avalonia pricing](https://avaloniaui.net/pricing), [GitHub releases](https://github.com/AvaloniaUI/Avalonia/releases).
- Tray/menu-bar support on all three OSes, with GNOME needing AppIndicator — [TrayIcon docs](https://docs.avaloniaui.net/controls/navigation/trayicon).
- macOS accessory mode — [discussion #13568](https://github.com/AvaloniaUI/Avalonia/discussions/13568).
- Wayland backend experimental and opt-in, with no position or topmost — [Avalonia 12.1](https://avaloniaui.net/blog/release-12-1), [WindowImplBase.cs](https://github.com/AvaloniaUI/Avalonia/blob/master/src/Avalonia.Wayland/WindowImplBase.cs).
- Footprint — [Local measurement](ui_framework_data.json).

### Inferences: risks, ranked
1. **Linux overlay and flyout on Wayland** (high). On native Wayland there is no positioning or topmost. On XWayland, GNOME may still constrain focus and stacking. Mitigation: X11 backend by default, notifications as the OSD fallback, menu-only quick controls on GNOME.
2. **GNOME tray** (high, product-level). It is invisible without the AppIndicator extension (Ubuntu ships it, Fedora/vanilla GNOME do not). Mitigation: detect `org.kde.StatusNotifierWatcher` on DBus at startup, and if it is missing open the settings window instead of starting silently. Document the extension.
3. **Memory on the GPU path** (medium). Default Avalonia uses more memory than WPF (~78 vs ~43 MB private shown, ~56 vs ~12 MB tray-only). Mitigation: software rendering + NativeAOT (~6–13 MB private), after verifying Mica still works in that mode.
4. **FluentAvalonia bus factor** (medium). One maintainer, NuGet-only releases, README out of date. Mitigation: use only a handful of controls, which are small enough to vendor if it stalls.
5. **NativeAOT compatibility** (medium). All bindings must be compiled, trimming must be configured, and FluentAvalonia's AOT safety is unverified. Mitigation: CI publishes with AOT on all three RIDs from day one. The fallback is single-file trimmed (~20 MB private tray-only with software rendering).
6. **Windows visual regression** (low–medium). Skia text vs DirectWrite. Mitigation: a side-by-side review before removing WPF.
7. **macOS specifics** (medium). Tray icon bounds for a popover, non-activating overlay across Spaces, Accessibility permission for global hotkeys, and code signing/notarisation for an AOT `.app`. These need small native shims or ObjC interop.

### Gaps
- Everything Linux and macOS in this document is from docs and source, not from runs. A spike on an Apple Silicon Mac, Ubuntu 24.04/26.04 GNOME (Wayland) and Fedora/Kubuntu KDE 6 (Wayland) should validate tray click behaviour, overlay topmost under XWayland, and idle memory before the port starts.
