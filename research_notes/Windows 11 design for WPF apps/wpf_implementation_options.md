# Making a WPF .NET 10 app look and behave like a native Windows 11 (Fluent) app — implementation options

Research date: 2026-10-05. Target app context (checked in this repo): `src/ThinkVisionControl/ThinkVisionControl.csproj` targets `net10.0-windows` with `UseWPF` + `UseWindowsForms` (WinForms only for `NotifyIcon`), has a hand-rolled `Ui/Theme.xaml`, a `WindowsThemeManager.cs` that already calls `DwmSetWindowAttribute(DWMWA_USE_IMMERSIVE_DARK_MODE)`, windows `PanelWindow`, `OsdWindow`, `FinderWindow`, `HotkeyCaptureWindow`, and uses `Segoe MDL2 Assets` glyphs (E706, E713, E770, E74E, E708, E734) in `PanelWindow.xaml`.

## 1. Built-in WPF Fluent theme (.NET 9, updated in .NET 10)

### Takeaway
The built-in Fluent theme is enabled with `ThemeMode="System"` on `Application` (XAML) or by merging `PresentationFramework.Fluent;component/Themes/Fluent.xaml`; it restyles stock WPF controls, follows light/dark and system accent, and automatically applies Mica (DWMSBT_MAINWINDOW) plus dark title bar to each window on Windows 11 22H2+. In .NET 10 (and in the current dotnet/wpf main branch, i.e. .NET 11) the `ThemeMode` API is still `[Experimental("WPF0001")]` and Microsoft still says Fluent "is still in progress"; it adds no new control types (no ToggleSwitch, NavigationView, InfoBar, NumberBox, ProgressRing, ContentDialog, cards).

### Cited Findings
**Enabling**
- Two ways to apply: set `ThemeMode`, or merge the resource dictionary `pack://application:,,,/PresentationFramework.Fluent;component/Themes/Fluent.xaml` into `Application.Resources` (or a single `Window.Resources`). — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90)
- `ThemeMode` values: `Light`, `Dark`, `System` (follows Windows app mode), `None` (default; Aero2). Can be set on `Application` or `Window`. — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90)
- "If the ThemeMode is set to any value other than None at the application-level, None can no longer be applied at the window-level." — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90). The dotnet/wpf developer guide says the same: a Window with `ThemeMode=None` "will adhere to Application's ThemeMode". — [dotnet/wpf Documentation/docs/using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- Fixed light/dark without `ThemeMode`: merge `Fluent.Light.xaml` or `Fluent.Dark.xaml` instead of `Fluent.xaml` (`Fluent.xaml` itself reacts to system theme changes). — [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- Setting `ThemeMode` in XAML works without suppression; accessing it from code raises error WPF0001; suppress with `<NoWarn>$(NoWarn);WPF0001</NoWarn>` or `#pragma warning disable WPF0001`. — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90)

**Experimental status (as of Oct 2026)**
- The .NET 10 API reference shows `[System.Diagnostics.CodeAnalysis.Experimental("WPF0001")]` on `Application.ThemeMode`, with remark "This property is experimental and may be removed in future versions." Monikers listed: windowsdesktop-9.0, 10.0, 11.0. Page updated 2026-07-01. — [Application.ThemeMode (API ref)](https://learn.microsoft.com/en-us/dotnet/api/system.windows.application.thememode?view=windowsdesktop-10.0)
- In dotnet/wpf `main` (checked 2026-10-05), both `Application.ThemeMode` and `Window.ThemeMode` still carry `[Experimental("WPF0001")]`. — [Application.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Application.cs), [Window.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Window.cs)
- The team's guide: "Fluent theme is still in experimental mode and there can be some breaking changes (like UI behavior, the way in which Fluent is loaded, how the keys are referenced, ThemeMode APIs etc) in the upcoming .NET releases." — [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- .NET 10 shipped November 2025; its WPF notes say "Fluent UI style support is still in progress." — [What's new in WPF .NET 10](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net100)

**Which controls are styled**
- .NET 10 added Fluent styles for DatePicker, GridSplitter, GridView, GroupBox, Hyperlink, Label, NavigationWindow, RichTextBox, TextBox; fixed Expander animation, HighContrast crashes, missing `RecognizesAccessKey`, RTL issues in MenuItem/Expander/TreeViewItem. — [What's new in WPF .NET 10](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net100)
- Full list of Fluent style files in dotnet/wpf main: Button, Calendar, CheckBox, CollectionViewGroup, ComboBox, ContentControl, ContextMenu, DataGrid, DatePicker, DocumentViewer, Expander, Frame, GridSplitter, GridView, GroupBox, GroupItem, HeaderedContentControl, Hyperlink, ItemsControl, Label, ListBox(Item), ListView(Item), Menu, MenuItem, NavigationWindow, Page, PasswordBox, ProgressBar, RadioButton, RepeatButton, ResizeGrip, RichTextBox, ScrollBar, ScrollViewer, Separator, Slider, StatusBar(Item), TabControl, TextBlock, TextBox, Thumb, ToggleButton, ToolBar, ToolTip, TreeView(Item), UserControl, Window. Theme dictionaries: Fluent.xaml, Fluent.Light.xaml, Fluent.Dark.xaml, Fluent.HC.xaml. — [dotnet/wpf PresentationFramework.Fluent/Styles](https://github.com/dotnet/wpf/tree/main/src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/Styles)
- A code search for "ToggleSwitch" inside PresentationFramework.Fluent only hits colour resource files (Resources/Theme/Light.xaml, Dark.xaml, HC.xaml), not any style file — i.e. brushes exist but there is no ToggleSwitch style/control. — [dotnet/wpf code search](https://github.com/dotnet/wpf/tree/main/src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent)
- MessageBox is not Fluent-styled; open issue "Apply Fluent Theme to WPF MessageBox" (#10415, opened 2025-02-05). — [dotnet/wpf#10415](https://github.com/dotnet/wpf/issues/10415)
- The Fluent `Window.xaml` style in dotnet/wpf carries the header "Copyright (C) Leszek Pomianowski and WPF UI Contributors" — the built-in theme's styles derive from WPF-UI. — [dotnet/wpf Styles/Window.xaml](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/Styles/Window.xaml)

**Mica / backdrop (automatic)**
- `ThemeMode` "also controls the application of backdrop and darkmode on window." — [Application.ThemeMode (API ref)](https://learn.microsoft.com/en-us/dotnet/api/system.windows.application.thememode?view=windowsdesktop-10.0)
- "When Fluent theme is enabled we set Mica (DWMSBT_MAINWINDOW) backdrop on the Window." No public API to choose other backdrop types. Opt-out switch: `Switch.System.Windows.Appearance.DisableFluentThemeWindowBackdrop` via `<RuntimeHostConfigurationOption Include="Switch.System.Windows.Appearance.DisableFluentThemeWindowBackdrop" Value="True" />` or runtimeconfig `configProperties`. — [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- Implementation (`WindowBackdropManager.SetBackdrop`): skipped if `window.AllowsTransparency` is true, if backdrop disabled, or if Fluent not enabled; otherwise sets `HwndSource.CompositionTarget.BackgroundColor = Transparent`, calls `DwmExtendFrameIntoClientArea` with margins -1, then `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE)`. `IsBackdropEnabled` requires `Utility.IsWindows11_22H2OrNewer` and the switch not set. Internal `Window.WindowBackdropType` defaults to `MainWindow` (Mica); the property is `internal`. — [WindowBackdropManager.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Appearance/WindowBackdropManager.cs), [Window.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Window.cs)
- `Window.cs` also calls `DwmSetWindowAttributeUseImmersiveDarkMode` as part of theme application (dark title bar handled automatically). — [Window.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Window.cs)
- Fluent `Window.xaml` template falls back to `{DynamicResource WindowBackground}` (opaque) when HighContrast is on, when the backdrop switch is set, or when the OS is not Windows 11. — [Styles/Window.xaml](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/Themes/PresentationFramework.Fluent/Styles/Window.xaml)
- .NET 10 servicing: PR "[release/10.0] Fix Fluent in Windows 10" (#11342, merged 2026-01-01) — default transparent background rendered wrongly on Windows 10, fixed by overriding background by OS/backdrop preference and listening for HC changes; PR #11417 (2026-02-04) and backport #11621 (2026-05-06) stop crashes when DWM restarts and `DwmExtendFrameIntoClientArea` fails; PR #11284 (2025-11-26) "Enable retry mechanism in Fluent Crashes". — [dotnet/wpf#11341](https://github.com/dotnet/wpf/pull/11341), [#11417](https://github.com/dotnet/wpf/pull/11417), [#11621](https://github.com/dotnet/wpf/pull/11621), [#11284](https://github.com/dotnet/wpf/pull/11284)

**System accent colour**
- .NET 9 added `SystemColors.AccentColor`, `AccentColorLight1..3`, `AccentColorDark1..3` (+ `...Brush`, `...Key`, `...BrushKey`); "Accent colors are available with or without the Fluent theme"; use `DynamicResource` and the colour updates live when the user changes it. — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90)
- Fluent colour dictionaries expose keys like `AccentFillColorDefaultBrush`, `AccentTextFillColorPrimaryBrush`, `TextOnAccentFillColorPrimaryBrush`, `SystemFillColorAttentionBrush`; guidance: use Light1–3 variants in dark mode and Dark1–3 in light mode. — [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)

**Known bugs / limitations (open issues in dotnet/wpf, as of 2026-10-05)**
- Styles set in App.xaml not adhering to Fluent / not applied to Window: #10834 (2025-05-07), #10858 (2025-05-19). — [#10834](https://github.com/dotnet/wpf/issues/10834), [#10858](https://github.com/dotnet/wpf/issues/10858)
- `WindowChrome.NonClientFrameEdges` causes layout misalignment after theme switch (#11204, 2025-10-22). — [#11204](https://github.com/dotnet/wpf/issues/11204)
- Crash with CheckBox on multiple UI threads (#11356, 2026-01-13). — [#11356](https://github.com/dotnet/wpf/issues/11356)
- Animations play even when Windows animation effects are disabled (#11798, 2026-07-21). — [#11798](https://github.com/dotnet/wpf/issues/11798)
- ComboBox doesn't update dropdown when bound collection changes (#11844, 2026-08-15); editable ComboBox keyboard trap on Shift+Tab (#11777, 2026-07-13). — [#11844](https://github.com/dotnet/wpf/issues/11844), [#11777](https://github.com/dotnet/wpf/issues/11777)
- Keyboard focus indication weaker than Aero (#11054); window flashing in dark mode (#10513); crash when theme changed (#9906, open since 2024-10). — [#11054](https://github.com/dotnet/wpf/issues/11054), [#10513](https://github.com/dotnet/wpf/issues/10513), [#9906](https://github.com/dotnet/wpf/issues/9906)
- WindowsFormsHost renders a black square under Fluent (#10044); high-contrast crash (#10043, .NET 9). — [#10044](https://github.com/dotnet/wpf/issues/10044), [#10043](https://github.com/dotnet/wpf/issues/10043)
- Community request that the theme should be "Fluent2" (#9561) and roadmap issue "Enabling Fluent as Default System Theme – Next Steps?" (#9283) remain open. — [#9561](https://github.com/dotnet/wpf/issues/9561), [#9283](https://github.com/dotnet/wpf/issues/9283)

### Inferences
- For this app, `ThemeMode="System"` in `App.xaml` is a zero-dependency way to get Fluent-styled Button/Slider/ComboBox/CheckBox/TabControl/ScrollViewer, Mica, dark title bar and accent — but the existing hand-rolled `Theme.xaml` implicit styles will override/fight Fluent styles (see #10834/#10858); those implicit styles would need to be removed or rebased with `BasedOn="{StaticResource {x:Type Button}}"`.
- The click-through OSD window almost certainly uses `AllowsTransparency=True`, so the backdrop manager skips it (no Mica applied there, which is desirable), but since window-level `ThemeMode=None` cannot opt out once the app is Fluent, the OSD will still pick up Fluent implicit styles/fonts; give it explicit local styles.
- Missing Windows 11 controls (ToggleSwitch, InfoBar, NavigationView, NumberBox, ProgressRing, ContentDialog, cards/SettingsCard) would have to be hand-built on top of Fluent brushes (`CardBackgroundFillColorDefaultBrush` style keys etc.). A ToggleSwitch is a ~60-line ToggleButton template; NavigationView for three pages can be a ListBox styled as a left nav.
- Because `WPF0001` only affects code access, using `ThemeMode` purely from XAML avoids touching the experimental API surface; if the app wants an in-app Light/Dark/System picker it must suppress WPF0001 and accept possible API churn in .NET 11/12.

### Gaps
- Could not find an official statement of whether WPF0001 will be removed (API promoted to stable) in .NET 11 (due Nov 2026); main branch still has it as of 2026-10-05.
- Did not verify whether Fluent theme restyles WinForms `ContextMenuStrip` (it should not — separate UI stack); tray menu theming must be handled separately (WinForms dark-mode APIs were not researched here).

## 2. WPF-UI (lepoco/wpfui)

### Takeaway
WPF-UI 4.3.0 (2026-05-04, MIT) supports net10.0-windows and is the most complete Fluent control set for WPF: FluentWindow + TitleBar (Snap Layouts), NavigationView, Card/CardExpander, ToggleSwitch, InfoBar, NumberBox, ProgressRing, ContentDialog, Snackbar, SymbolIcon (Fluent System Icons font), Mica/Acrylic/Tabbed backdrops, `SystemThemeWatcher`, plus a separate `WPF-UI.Tray` package. Cost: a ~6.5 MB `Wpf.Ui.dll`, ~418 open issues, and a history of releases being NuGet-deprecated for "critical bugs".

### Cited Findings
**Versions / support / licence**
- NuGet `WPF-UI` latest 4.3.0 released 2026-05-04; licence MIT; target frameworks net462/net472/net481, net8.0-windows, net9.0-windows, net10.0-windows; dependency `WPF-UI.Abstractions (>= 4.3.0)`. 1.2M total downloads. — [NuGet WPF-UI](https://www.nuget.org/packages/WPF-UI) (versions/dates re-checked via the NuGet registration API)
- Recent history: 4.1.0 (2025-11-24), 4.2.0 (2026-01-10), 4.2.1 (2026-04-23), 4.3.0 (2026-05-04). NuGet marks 4.2.0, 4.1.0, 4.0.x, 3.1.0 and 3.0.x "Deprecated (critical bugs)"; 4.2.1, 4.3.0 and 3.1.1 are active. — [NuGet WPF-UI](https://www.nuget.org/packages/WPF-UI)
- Release notes: 4.1.0 added ".NET 10 support", Win32 PInvoke cleanup, 60+ control fixes, FlaUI tests; 4.2.0 system-accent colorization checks, ToggleSwitch `LabelPosition`, TitleBar `CenterContent`; 4.2.1 "System accent color application toggle in UiApplication", ProgressRing fix; 4.3.0 NotifyIcon tooltip Unicode, NumberBox Compact spin mode; 4.0.0 introduced `Wpf.Ui.Abstractions`, signed packages, ARM64, updated Fluent System Icons. — [lepoco/wpfui releases](https://github.com/lepoco/wpfui/releases) (note: the fetched summary mis-stated years as 2023/2024; NuGet dates above are authoritative)
- `WPF-UI.Tray` 4.3.0 (2026-05-04, MIT) ships in lockstep with the main package. — [NuGet WPF-UI.Tray](https://www.nuget.org/packages/WPF-UI.Tray)
- Repo: MIT, ~9.7k stars, ~1.0k forks, 2,137 commits, 418 open issues; last push 2026-06-27 (checked via GitHub API). Gallery app in Microsoft Store; `winget install 'WPF UI'`; VS 2022 template extension. — [lepoco/wpfui](https://github.com/lepoco/wpfui)

**Size (relevant to single-file publish)**
- In the 4.3.0 nupkg, `lib/net10.0-windows7.0/Wpf.Ui.dll` is 6,565,888 bytes; `Wpf.Ui.Tray.dll` is 43,008 bytes; `Wpf.Ui.Abstractions.dll` 7,168 bytes (measured by downloading the packages from [nuget.org flat container](https://api.nuget.org/v3-flatcontainer/wpf-ui/4.3.0/wpf-ui.4.3.0.nupkg)).

**Controls and features**
- README features: Fluent themes, modified base controls (Page, ToggleButton, List), extra controls (Navigation, NumberBox, Dialog, Snackbar), Windows 11 Snap Layout support in title bar, tray icon and menu via Wpf.Ui.Tray, VS designer support, Segoe Fluent Icons integration; usage of `FluentWindow`, `TitleBar`, `Card`. — [lepoco/wpfui README](https://github.com/lepoco/wpfui)
- Docs list topics for navigation-view, symbolicon, fonticon, icons, menu, accent, themes, system-theme-watcher. — [wpfui docs folder](https://github.com/lepoco/wpfui/tree/main/docs/documentation)
- Icons: library ships the Fluent UI System Icons font, mapped to `SymbolRegular`/`SymbolFilled` enums (e.g. `<ui:SymbolIcon Symbol="Fluent24"/>`). Segoe Fluent Icons cannot be redistributed in the DLL per its EULA; it is installed on Windows 11, and on Windows 10 must be added manually. — [wpfui docs/icons.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/icons.md)

**Setup**
- App.xaml: namespace `xmlns:ui="http://schemas.lepo.co/wpfui/2022/xaml"`, merged dictionaries `<ui:ThemesDictionary Theme="Dark" />` and `<ui:ControlsDictionary />`. — [wpfui getting-started.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/getting-started.md)
- Runtime theme: `ApplicationThemeManager.Apply(ApplicationTheme.Light, WindowBackdropType.Mica)`; `ApplySystemTheme()` for one-time sync; brushes must be `DynamicResource`. — [wpfui themes.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/themes.md)
- `SystemThemeWatcher.Watch(this)` in the main window constructor syncs theme, accent and backdrop live; hooks WndProc. Backdrop types: None, Auto, Mica, Acrylic, Tabbed. — [wpfui system-theme-watcher.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/system-theme-watcher.md)
- Source shows the watcher reacts to `WM_DWMCOLORIZATIONCOLORCHANGED`, `WM_THEMECHANGED`, `WM_SYSCOLORCHANGE`, `WM_SETTINGCHANGE`. — [SystemThemeWatcher.cs](https://github.com/lepoco/wpfui/blob/main/src/Wpf.Ui/Appearance/SystemThemeWatcher.cs)
- Accent: `ApplicationAccentColorManager.ApplySystemAccent()`, `ApplicationThemeManager.Apply(..., updateAccent: true)`, custom `ApplicationAccentColorManager.Apply(color, theme)`, `GetColorizationColor()`. — [wpfui accent.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/accent.md)

**Known open issues (sample, as of 2026-10-05)**
- Tray: "Changing NotifyIcon.Icon for a registered NotifyIcon does not work" (#408, 2022), "Unregistering tray icon does not work" (#240, 2022), "NotifyIcon Disappear After Window Manager Recover From Crash" (#606), "BalloonTip support in Wpf.Ui.Tray" request (#1713, 2026-05), "Improvements to WPF-UI.Tray NotifyIconService" (#1668, 2026-03). — [lepoco/wpfui issues](https://github.com/lepoco/wpfui/issues?q=is%3Aopen+NotifyIcon)
- Window/backdrop: "Switching theme loses transparency" (#1163), "Mica [Light] Theme not working properly" (#621), "Cannot disable resizing in FluentWindow" (#1691, 2026-04), "NavigationView Rendering Issue on Specific Hardware" (#1594, 2025-12), "Cleartype not working" (#1746, 2026-07). — [lepoco/wpfui issues](https://github.com/lepoco/wpfui/issues)

### Inferences
- WPF-UI gives the closest match to Windows 11 Settings-app look (NavigationView + SettingsCard-style cards + ToggleSwitch + InfoBar) with the least custom XAML, which suits a three-page monitor-control app.
- Its tray package has long-standing open bugs; keeping the existing WinForms `NotifyIcon` (already working, no extra dependency) is lower-risk than migrating to `Wpf.Ui.Tray`.
- The ~6.5 MB DLL roughly doubles-to-triples a framework-dependent single-file exe of a small app; for a self-contained publish (~60–150 MB of runtime) it is negligible. WPF does not support trimming, so the size can't be trimmed away (trimming status not re-verified in this pass).
- The pattern of whole release lines being NuGet-deprecated for "critical bugs" suggests pinning an exact version and re-testing on each upgrade.

### Gaps
- No reliable list found of large/well-known production apps built on WPF-UI (beyond download/star counts and the fact that Microsoft's built-in Fluent styles were derived from it).
- Did not verify whether WPF-UI 4.x coexists cleanly with `Application.ThemeMode` (mixing both is likely to conflict; not tested).

## 3. Other libraries: iNKORE.UI.WPF.Modern, ModernWpf, HandyControl, Windows Community Toolkit

### Takeaway
ModernWpf (Kinnara) was revived in 2026 and has a 1.0.0-rc.1 (2026-09-06, MIT, net10 target) — promising but still pre-release. iNKORE.UI.WPF.Modern's licence now forbids commercial use without permission and requires consumers to be open-source, which is a blocker for many projects. HandyControl is its own non-Fluent design language and its newest build is a 3.6.0 RC. Windows Community Toolkit (SettingsCard etc.) is WinUI/UWP-only and cannot be used in WPF.

### Cited Findings
- **ModernWpfUI** (NuGet id `ModernWpfUI`, author Yimeng Wu, project github.com/Kinnara/ModernWpf, MIT): 1.0.0-preview.5/6 (2026-08-10), preview.7 (2026-08-11), 1.0.0-rc.1 (2026-09-06); targets net462, net8.0-windows, net10.0-windows. — [NuGet ModernWpfUI](https://www.nuget.org/packages/ModernWpfUI) (via NuGet registration API)
- ModernWPF 1.0.0-rc.1 release notes: "freezes the intended 1.0 public CLR API and resource-key surface"; references a "WinUI3 sync" upstream cutoff; fixes to ContentDialog focus, RadioButtons, CalendarDatePicker, AutoSuggestBox, CommandBar; includes migration guide from 0.9.x. Repo last push 2026-10-05, ~5k stars, not archived. — [Kinnara/ModernWpf release v1.0.0-rc.1](https://github.com/Kinnara/ModernWpf/releases/tag/v1.0.0-rc.1)
- **iNKORE.UI.WPF.Modern**: latest 0.10.2.1 (2025-11-18), targets net452 and net6.0-windows; repo ~1k stars, last push 2026-10-01; licence is custom: "Commercial Use: You may not incorporate any part of the Software into commercial products without explicit written permission from iNKORE Studios" and "All software incorporating or referencing the Software must be fully open-source unless otherwise explicitly permitted". — [NuGet iNKORE.UI.WPF.Modern](https://www.nuget.org/packages/iNKORE.UI.WPF.Modern), [iNKORE LICENSE.md](https://github.com/iNKORE-NET/UI.WPF.Modern/blob/main/LICENSE.md)
- **HandyControl**: latest stable 3.5.1 (2024-03-05); 3.6.0-rc2 (2025-05-20), 3.6.0-rc3 (2025-09-28); repo MIT, ~7.2k stars, last push 2026-08-11. — [NuGet HandyControl](https://www.nuget.org/packages/HandyControl), [HandyOrg/HandyControl](https://github.com/HandyOrg/HandyControl)

### Inferences
- HandyControl's visual language is not Windows 11 Fluent, so it does not meet the "genuine Windows 11 look" goal.
- ModernWpf 1.0 could become a strong WinUI-parity option (NavigationView, ContentDialog, NumberBox, ToggleSwitch existed in 0.9.x), but adopting an RC in October 2026 means waiting for 1.0 stable.
- Windows Community Toolkit: SettingsControls (SettingsCard/SettingsExpander) are WinUI controls; for WPF the equivalent must come from WPF-UI (CardControl/CardExpander) or be hand-built.

### Gaps
- Did not fetch a Windows Community Toolkit page in this pass to cite "WinUI-only" explicitly; the claim rests on the toolkit being built for WinUI 2/3 (well known, but uncited here).
- ModernWpf 1.0 visual fidelity to Windows 11 (Fluent 2 corner radii, Mica support) was not verified.

## 4. Mica, dark title bar and rounded corners via DWM (hand-rolled path)

### Takeaway
Mica is `DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE = 38, DWMSBT_MAINWINDOW = 2)` on Windows 11 build 22621 (22H2)+, with the WPF composition background made transparent and the frame extended into the client area (exactly what WPF's own `WindowBackdropManager` does). Dark title bar is attribute 20 (Win11 22000+); rounded corners are attribute 33 (Win11 22000+, default already rounded for normal top-level windows).

### Cited Findings
- `DWMWINDOWATTRIBUTE`: `DWMWA_USE_IMMERSIVE_DARK_MODE = 20`, `DWMWA_WINDOW_CORNER_PREFERENCE = 33`, followed sequentially by `DWMWA_BORDER_COLOR` (34), `DWMWA_CAPTION_COLOR` (35), `DWMWA_TEXT_COLOR` (36), `DWMWA_VISIBLE_FRAME_BORDER_THICKNESS` (37), `DWMWA_SYSTEMBACKDROP_TYPE` (38). Dark mode, corner preference, border/caption/text colour supported from Windows 11 Build 22000; `DWMWA_SYSTEMBACKDROP_TYPE` from Build 22621. `DWMWA_COLOR_NONE = 0xFFFFFFFE` suppresses the border; `DWMWA_COLOR_DEFAULT = 0xFFFFFFFF` resets. — [DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
- "For compatibility reasons, all windows default to light mode regardless of the system setting" — must set attribute 20 to TRUE for dark frame. — [DWMWINDOWATTRIBUTE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwmwindowattribute)
- `DWM_SYSTEMBACKDROP_TYPE`: `DWMSBT_AUTO` (0), `DWMSBT_NONE` (1), `DWMSBT_MAINWINDOW` (2, Mica), `DWMSBT_TRANSIENTWINDOW` (3, Desktop Acrylic), `DWMSBT_TABBEDWINDOW` (4, Mica Alt). AUTO "applies the backdrop material just behind the default Win32 title bar" and may draw nothing. Min client: Windows 11 Build 22621. — [DWM_SYSTEMBACKDROP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type)
- Mica "only captures the background wallpaper once", adapts as the window moves, and "falls back to a neutral color when the app is inactive". Microsoft's official Win32 Mica guidance uses Windows App SDK `MicaController` (not the DWM attribute). — [Use Mica material in Win32 apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-mica-win32)
- WPF's own implementation (reference recipe): skip if `AllowsTransparency`; set `HwndSource.FromHwnd(hwnd).CompositionTarget.BackgroundColor = Colors.Transparent`; `DwmExtendFrameIntoClientArea(hwnd, MARGINS{-1,-1,-1,-1})`; `DwmSetWindowAttribute(hwnd, DWMWA_SYSTEMBACKDROP_TYPE, DWMSBT_MAINWINDOW)`; restore with `SystemColors.WindowColor` and `DWMSBT_NONE`. — [WindowBackdropManager.cs](https://github.com/dotnet/wpf/blob/main/src/Microsoft.DotNet.Wpf/src/PresentationFramework/System/Windows/Appearance/WindowBackdropManager.cs)
- Win32 dark-title-bar sample: `BOOL value = TRUE; DwmSetWindowAttribute(hWnd, DWMWA_USE_IMMERSIVE_DARK_MODE, &value, sizeof(value));` (set FALSE for light). — [Support Dark and Light themes in Win32 apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes)
- DWM can crash/restart, causing `DwmExtendFrameIntoClientArea` to fail; WPF added handling so this doesn't crash the app (PR #11417). — [dotnet/wpf#11417](https://github.com/dotnet/wpf/pull/11417)

### Inferences
- The WPF window's `Background` must also be transparent (or a semi-transparent "layer" brush) for Mica to show through; WPF's Fluent Window style does this by binding Background to a transparent default when backdrop is active.
- The repo's `WindowsThemeManager.cs` already sets attribute 20; adding attribute 38 + transparent composition background + frame extension (~30 lines) yields Mica on the main window without any library, if the hand-rolled route is kept. Must re-apply attribute 20 on theme change and keep an opaque fallback for Windows 10 / 21H2 and for High Contrast.
- Do not apply Mica to the click-through OSD (`AllowsTransparency=True` layered window — DWM backdrops don't apply, and WPF itself skips such windows).

### Gaps
- Microsoft does not document a WPF-specific Mica recipe; the DWM-attribute approach for WPF is evidenced by WPF's own source rather than a how-to article.

## 5. Icons: Segoe Fluent Icons and fallback

### Takeaway
Segoe Fluent Icons ships with Windows 11 and is the replacement for the deprecated Segoe MDL2 Assets; it mostly keeps the same code points, so the app's existing MDL2 glyphs can switch font family with a `"Segoe Fluent Icons, Segoe MDL2 Assets"` fallback list. All code points below were verified present in the installed `SegoeIcons.ttf` (v1.54, 1,635 glyphs) on this Windows 11 machine; all except Contrast (E7A1) are also present in `segmdl2.ttf` (v1.86).

### Cited Findings
- Availability: included by default on Windows 11; on Windows 10 it must be downloaded; Segoe MDL2 Assets is deprecated in favour of it. Recommended sizes 16, 20, 24, 32, 40, 48, 64. — [Segoe Fluent Icons font](https://learn.microsoft.com/en-us/windows/apps/design/style/segoe-fluent-icons-font)
- Glyphs (name → code point) from the official table — [Segoe Fluent Icons font](https://learn.microsoft.com/en-us/windows/apps/design/style/segoe-fluent-icons-font):
  - Brightness / display: Brightness E706, LowerBrightness EC8A, Light E793, Contrast E7A1, TVMonitor E7F4, Projector E95D, Devices E772
  - Input: Input E961
  - Colour: Color E790
  - Settings E713; KeyboardClassic E765, KeyboardStandard E92E, KeyboardShortcut EDA7
  - Power: PowerButton E7E8
  - Reset-like: Refresh E72C, Undo E7A7, Sync E895 (ResetDrive EBC4 exists but is a drive icon)
  - Status: Info E946, Info2 EA1F, Warning E7BA, Important E8C9, Error E783, StatusError EA83, StatusWarning EA84, Help E897
  - Navigation/chrome: GlobalNavButton E700, Back E72B, More E712, Pin E718, Home E80F, ChevronDown E70D, CheckMark E73E, Accept E8FB, Cancel E711, ChromeMinimize E921, ChromeMaximize E922, ChromeRestore E923, ChromeClose E8BB
- No glyph named "Plug", "HDMI", "Palette" or "Sun" was found in the table. — [Segoe Fluent Icons font](https://learn.microsoft.com/en-us/windows/apps/design/style/segoe-fluent-icons-font)
- Local verification (this machine, Windows 11 build 26200): every code point above exists in Segoe Fluent Icons; E7A1 is missing from Segoe MDL2 Assets. — (measured with `System.Windows.Media.GlyphTypeface.CharacterToGlyphMap` against `C:\Windows\Fonts\SegoeIcons.ttf` and `segmdl2.ttf`)
- WPF-UI does not bundle Segoe Fluent Icons (EULA); it relies on the OS copy. — [wpfui docs/icons.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/icons.md)

### Inferences
- In WPF, `FontFamily="Segoe Fluent Icons, Segoe MDL2 Assets"` gives automatic per-glyph fallback on Windows 10 (except Contrast E7A1, which would render as a box there).
- WPF-UI's `SymbolIcon` uses a different icon set (Fluent UI System Icons, as used in Windows 11 Settings and Office) — arguably closer to the Windows 11 Settings app look than Segoe Fluent Icons, and works on Windows 10 without font install.

### Gaps
- The glyph list came via a summarised fetch of the docs table; code points were then verified to exist in the font, but the name↔glyph mapping (i.e. that E961 visually is "Input") was not visually confirmed.

## 6. System accent colour and theme-change detection in WPF

### Takeaway
On .NET 9+/10, `SystemColors.AccentColor*` (and their `...BrushKey`s) are the simplest source and update live when used via `DynamicResource`, with or without the Fluent theme. For light/dark detection, the documented Windows approach is `UISettings.GetColorValue(UIColorType.Foreground)` + `ColorValuesChanged`; WPF-UI and WPF itself also hook `WM_SETTINGCHANGE`/`WM_DWMCOLORIZATIONCOLORCHANGED`.

### Cited Findings
- `SystemColors.AccentColor`, `AccentColorLight1..3`, `AccentColorDark1..3` (+ Brush and resource keys); "Accent colors are available with or without the Fluent theme"; use `{DynamicResource {x:Static SystemColors.AccentColorBrushKey}}` to update live. — [What's new in WPF .NET 9](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net90)
- Use Light1–3 in dark mode and Dark1–3 in light mode. — [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- Dark mode detection: `new UISettings().GetColorValue(UIColorType.Foreground)` and check `IsColorLight` (`(5*G + 2*R + B) > 8*128`); subscribe to `UISettings.ColorValuesChanged` to track changes. — [Support Dark and Light themes in Win32 apps](https://learn.microsoft.com/en-us/windows/apps/desktop/modernize/ui/apply-windows-themes)
- WPF-UI's watcher listens for `WM_DWMCOLORIZATIONCOLORCHANGED`, `WM_THEMECHANGED`, `WM_SYSCOLORCHANGE`, `WM_SETTINGCHANGE`; its docs describe `WM_WININICHANGE` triggering `ApplySystemTheme()`. — [SystemThemeWatcher.cs](https://github.com/lepoco/wpfui/blob/main/src/Wpf.Ui/Appearance/SystemThemeWatcher.cs), [system-theme-watcher.md](https://github.com/lepoco/wpfui/blob/main/docs/documentation/system-theme-watcher.md)

### Inferences
- `UISettings` requires a Windows SDK TFM (e.g. `net10.0-windows10.0.19041.0`) for WinRT projection; the registry (`HKCU\Software\Microsoft\Windows\CurrentVersion\Themes\Personalize\AppsUseLightTheme`) + `WM_SETTINGCHANGE` ("ImmersiveColorSet") route avoids that and is what the repo's `WindowsThemeManager.cs` likely already does. `UISettings.ColorValuesChanged` fires on a background thread, so marshal to the Dispatcher.
- With `ThemeMode="System"` none of this plumbing is needed for the main UI.

### Gaps
- Did not fetch a primary source for the `AppsUseLightTheme` registry value or the `ImmersiveColorSet` WM_SETTINGCHANGE parameter in this pass (widely used, but uncited here).
- Threading behaviour of `ColorValuesChanged` not cited from a primary source.

## 7. Recommendation for this app (built-in Fluent vs WPF-UI vs hand-rolled)

### Takeaway
Two defensible options: (A) **built-in `ThemeMode="System"`** + a handful of hand-built controls (ToggleSwitch, left nav list, card border, InfoBar) — zero dependencies, Microsoft-maintained, automatic Mica/dark title bar/accent, but experimental API and some open control bugs; (B) **WPF-UI 4.3.0** — highest fidelity and least custom XAML (NavigationView, CardControl, ToggleSwitch, InfoBar, NumberBox, ContentDialog), MIT, net10 supported, but +~6.5 MB and its own bug history. Pure hand-rolled styles are the most effort for the lowest fidelity. For a small utility with three pages, a tray icon and an overlay, (A) is the lower-risk default; pick (B) if a Windows 11 Settings-style NavigationView + settings cards is the explicit goal.

### Cited Findings
- Built-in theme: zero package dependency, auto Mica + dark title bar + accent; experimental (`WPF0001`), "still in progress"; no new controls. — [What's new in WPF .NET 10](https://learn.microsoft.com/en-us/dotnet/desktop/wpf/whats-new/net100), [Application.ThemeMode](https://learn.microsoft.com/en-us/dotnet/api/system.windows.application.thememode?view=windowsdesktop-10.0), [using-fluent.md](https://github.com/dotnet/wpf/blob/main/Documentation/docs/using-fluent.md)
- WPF-UI: MIT, net10 target, 4.3.0 (2026-05-04), `Wpf.Ui.dll` 6.5 MB for net10. — [NuGet WPF-UI](https://www.nuget.org/packages/WPF-UI)
- iNKORE licence restricts commercial/closed-source use; ModernWpf 1.0 still RC. — [iNKORE LICENSE.md](https://github.com/iNKORE-NET/UI.WPF.Modern/blob/main/LICENSE.md), [ModernWpf rc.1](https://github.com/Kinnara/ModernWpf/releases/tag/v1.0.0-rc.1)

### Inferences
Concrete setup for option A (built-in):
1. `App.xaml`: add `ThemeMode="System"` to `<Application>` (XAML use needs no WPF0001 suppression).
2. Remove or rebase implicit styles in `Ui/Theme.xaml` (`BasedOn="{StaticResource {x:Type Button}}"`) so Fluent templates aren't replaced; replace hard-coded colours with Fluent keys (`TextFillColorPrimaryBrush`, `CardBackgroundFillColorDefaultBrush`, `AccentFillColorDefaultBrush`, etc.) via `DynamicResource`.
3. Delete manual title-bar dark-mode code for windows covered by Fluent (WPF sets attribute 20 itself), or keep it only for non-Fluent windows.
4. Leave `Window.Background` unset on the main window so Mica shows; add semi-opaque card borders for content.
5. OSD window: keep `AllowsTransparency=True` (no backdrop applied) and give it explicit local styles, since window-level `ThemeMode=None` cannot opt out.
6. Switch icon `FontFamily` to `"Segoe Fluent Icons, Segoe MDL2 Assets"`.
7. Hand-build: ToggleSwitch (ToggleButton template), left navigation (ListBox styled as nav), card (Border with `CardBackgroundFillColorDefaultBrush`/`CardStrokeColorDefaultBrush`, CornerRadius 8), InfoBar (Border + icon + text).
8. If an in-app theme picker is wanted: `<NoWarn>$(NoWarn);WPF0001</NoWarn>` and set `Application.Current.ThemeMode` in code.
9. Tray: keep WinForms `NotifyIcon` (Fluent theme does not affect it).

Concrete setup for option B (WPF-UI):
1. `dotnet add package WPF-UI --version 4.3.0` (pin exact version).
2. `App.xaml`: `xmlns:ui="http://schemas.lepo.co/wpfui/2022/xaml"`, merge `<ui:ThemesDictionary Theme="Dark"/>` + `<ui:ControlsDictionary/>`; do not also set `ThemeMode`.
3. Main window → `ui:FluentWindow` with `ui:TitleBar`, `WindowBackdropType="Mica"`, `ExtendsContentIntoTitleBar="True"`; call `SystemThemeWatcher.Watch(this)` in the constructor.
4. Pages → `ui:NavigationView` with three items; settings rows → `ui:CardControl`/`ui:CardExpander`; toggles → `ui:ToggleSwitch`; status → `ui:InfoBar`; icons → `ui:SymbolIcon`.
5. Keep WinForms `NotifyIcon` rather than `WPF-UI.Tray` (open tray bugs since 2022).
6. OSD window stays a plain `Window` with explicit styles.

Trade-off summary:
| | Built-in Fluent (ThemeMode) | WPF-UI 4.3.0 | Hand-rolled styles + DWM |
|---|---|---|---|
| Added size | 0 (in shared framework) | ~6.5 MB DLL | 0 |
| Mica / dark title bar | Automatic (22H2+) | Automatic (Mica/Acrylic/Tabbed selectable) | Manual DWM P/Invoke |
| Accent / follow OS | Automatic | `SystemThemeWatcher` | Manual |
| Win11 controls (ToggleSwitch, NavView, InfoBar, cards, NumberBox) | Not included — hand-build | Included | Hand-build |
| Stability | Experimental API; open control bugs; MS-maintained | Community; NuGet-deprecated releases for critical bugs; 418 open issues | Whatever you write |
| Effort for this app | Low–medium | Medium (window/page rewrite) | High |
| Fidelity | Good for stock controls | Highest | Depends on effort |

### Gaps
- No hands-on test in this repo of either option; effort estimates are judgement.
- Exact single-file exe size impact not measured (depends on framework-dependent vs self-contained publish and compression).
