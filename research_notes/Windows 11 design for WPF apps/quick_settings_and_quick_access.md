# Windows 11 Quick Settings extensibility and quick-access alternatives for a DDC/CI monitor utility

Research date: 2026-10-05. Context: ThinkVisionControl is an unpackaged .NET 10 WPF app that already has a tray icon (`src/ThinkVisionControl/Tray/TrayIcon.cs`), hotkeys and a single-instance relaunch path.

**Bottom line: No.** As of October 2026, third-party apps cannot add tiles, toggles or sliders to Windows 11 Quick Settings. There is no documented or announced API, and Microsoft has said so on the record. The Quick Settings brightness slider does not drive external monitors over DDC/CI, and apps cannot hook into it. The supported options are the tray flyout, hotkeys, jump lists, toasts and (packaged only) Widgets.

## 1. Can third-party apps add tiles, toggles or sliders to Windows 11 Quick Settings?

### Takeaway
No. Microsoft's answer has been "there is currently no extension API for Windows 11 Quick Settings" (Microsoft Q&A, Dec 2022), and nothing found from 2023 to October 2026 changes that. Recent Quick Settings changes are all first-party: a scrollable layout, and Energy Recommendations in a September 2026 Insider build. The brightness slider covers only the one panel Windows designates for brightness (normally the built-in laptop screen). For non-internal connectors it works only through an OEM driver and registry opt-in, not through apps or DDC/CI.

### Cited Findings
- On the question "Is there any API available to add a custom item in the Windows 11 taskbar panel?" (12 Dec 2022), Microsoft employee Junjie Zhu (MSFT) answered: "There is currently no extension API for Windows 11 Quick Settings." The answer points to Raymond Chen's comment on the Stack Overflow question "Can I create a custom item for Windows 11 Quick Settings?" — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/1124942/api-for-adding-an-item-in-the-windows-11-taskbar-p)
  - The linked Stack Overflow thread (https://stackoverflow.com/questions/71059438) could not be fetched (tool blocked), so Raymond Chen's exact wording is not verified here.
- Microsoft's main third-party "surface in the OS" API of the period is **App Actions** (Build 2025), not Quick Settings. App Actions appear in **Click to Do** (contextual right-click). They ship in Windows SDK 10.0.26100.4188, with launch partners such as Zoom, Todoist, Raycast and Spark Mail — [Windows Central](https://www.windowscentral.com/software-apps/windows-11/windows-11-app-actions-api-announcement-build-2025)
- Quick Settings layout changes have been first-party only:
  - A scrollable, rearrangeable list of quick actions was tested from Canary build 25967 (Oct 2023) — [BetaNews](https://betanews.com/article/windows-11-build-25967); [Anoop C Nair](https://www.anoopcnair.com/?p=145968)
  - The scrollable flyout was reported as part of version 24H2 — search-result summary of [Eleven Forum](https://www.elevenforum.com/t/kb5039239-windows-11-insider-release-preview-build-26100-863-24h2-june-15.25400/page-8). This is a weaker source and the version wasn't checked against a primary one.
- **Insider-only, September 2026:** Experimental build 26340.9577 adds Energy Recommendations "directly in Quick Settings". The post says nothing about third-party extensibility, brightness or external monitors — [Windows Insider Blog, 25 Sep 2026](https://blogs.windows.com/windows-insider/2026/09/25/announcing-new-builds-for-25-september-2026/)
- On the brightness slider and external monitors, Microsoft Support says: "You might not see the Brightness slider on devices that have an external monitor. To change the brightness on an external monitor, use the buttons on it." Settings carries a similar line for desktop PCs — [Microsoft Support](https://support.microsoft.com/en-us/windows/hardware/display-graphics/change-display-brightness-and-color-in-windows)
- At the driver level, "Windows has the ability to designate exactly one display panel to support the system software brightness control."
  - OEMs can flag a non-internal connector through the `BrightnessControl` DWORD under the display class key (bit 0 = non-internal brightness support, bit 1 = ACPI override). They must ship their own monitor.inf and still implement the brightness control in the graphics driver.
  - This is an OEM/driver mechanism for internal panels wired through external connectors. It does not route the slider to DDC/CI on arbitrary monitors.
  - [Microsoft Learn: Supporting brightness controls for external display connectors](https://learn.microsoft.com/en-us/windows-hardware/drivers/display/supporting-brightness-controls-for-external-display-connectors)
- Microsoft's own DDC/CI tool, PowerToys **Power Display** (shipped in PowerToys 0.99, 28 Apr 2026), uses a hotkey, a tray icon and a WinUI 3 flyout. Its design doc does not mention Quick Settings integration — [PowerDisplay design doc](https://microsoft.github.io/PowerToys/modules/powerdisplay/design/); [Microsoft Learn: Power Display](https://learn.microsoft.com/en-ca/windows/powertoys/power-display); release date per [Pureinfotech](https://pureinfotech.com/windows-11-power-display-monitor-controls/)
- Windhawk, the best-known Windows 11 shell-modding framework, "us[es] memory injection to apply changes while Windows is running" — [Logicity summary](https://logicity.in/en/blog/windhawk-unlocks-the-windows-11-settings-microsoft-hides). No Windhawk mod or other hack that adds a custom Quick Settings tile turned up in searching ([Ramen Software taskbar mods list](https://ramensoftware.com/windhawk-mods-for-the-windows-11-taskbar/comment-page-1)).

### Inferences
- Even Microsoft's own DDC/CI product ships outside Quick Settings. That is strong circumstantial evidence there is no private-but-usable path an outside developer could rely on.
- An app has no supported way to put an external monitor under the Quick Settings slider. The driver path needs OEM driver work and a custom monitor.inf. It's built for a single panel and doesn't use DDC/CI.
- Any hack that injects into the shell process to add a tile would be:
  - **Fragile:** it breaks on cumulative updates, because Quick Settings internals are undocumented and keep being redesigned (scrollable layout, energy recommendations).
  - **Security-sensitive:** code injection into explorer or the shell host may be flagged by AV/EDR.
  - **Unsuitable** for a small utility meant for ordinary users.

### Gaps
- Raymond Chen's exact Stack Overflow comment wasn't retrieved (fetch blocked).
- No Feedback Hub entry with an official Microsoft response, and no Microsoft statement from 2024–2026 that specifically rules a Quick Settings API in or out, was found. Absence of an announcement is the evidence for 2023–2026.
- No public reports of a working third-party Quick Settings injection hack were found, so its specific risks can't be cited.
- Not verified on any current Insider channel (Canary/Dev/Beta/Experimental) whether the brightness slider now shows DDC/CI monitors. No source claims it does.

## 2. What did Windows 10 Action Center quick actions allow, and what happened in Windows 11?

### Takeaway
Windows 10 Action Center quick actions were a fixed set of system toggles. Users could reorder and choose among them, but no public API for third-party quick actions was found. Windows 11 split Action Center into Notification Center and Quick Settings, and kept the "first-party tiles only" model.

### Cited Findings
- Windows 10 Action Center had a quick-actions area that users could collapse to four or expand to show all. Items were built-in settings such as Wi-Fi, Bluetooth and Airplane mode — [Wikipedia: Action Center](https://en.wikipedia.org/wiki/Action_Center)
- "The Action Center was replaced with Quick Settings and the Notification Center in Windows 11." — [Wikipedia: Action Center](https://en.wikipedia.org/wiki/Action_Center)
- Users could customize which built-in quick actions appear and reorder them (drag-and-drop arrived in later builds) — [Digital Citizen](https://www.digitalcitizen.life/how-access-use-customize-quick-actions-windows-10/); [Windows Central](https://windowscentral.com/action-centers-quick-actions-get-drag-n-drop-customization-internal-builds)
- Third-party apps did participate in the notifications half of Action Center, with toasts that persisted there — [Guiding Tech](https://www.guidingtech.com/what-is-action-center-in-windows-10-and-how-to-use-it/)

### Inferences
- Third-party apps never had quick-action tiles, so there was no Windows 10 capability to "lose" in Windows 11. The only third-party presence then and now is notifications.

### Gaps
- No Microsoft developer doc explicitly says "no third-party quick actions API in Windows 10". The conclusion rests on the lack of any such API in docs or press. The Wikipedia page does not state it outright.

## 3. How do comparable apps provide quick access, and which surfaces work for an unpackaged WPF .NET app?

### Takeaway
Every comparable DDC/CI app uses the same pattern: a resident tray icon whose left-click opens a slider flyout near the tray, plus global hotkeys, often a command line, and sometimes mouse-wheel adjustment. None uses Quick Settings or Widgets. The usable surfaces for an unpackaged WPF app are:
- tray flyout
- hotkeys
- jump lists
- toast notifications with buttons (via Windows App SDK `AppNotificationManager`, which supports unpackaged apps)
- a CLI

The Widgets Board needs a packaged (MSIX) provider.

### Cited Findings
- **PowerToys Power Display (Microsoft, 2026):**
  - Entry points are a global hotkey, a tray icon (`TrayIconService`) and a WinUI 3 flyout.
  - It runs as a separate resident process, `PowerToys.PowerDisplay.exe`, launched when the module is enabled.
  - It uses DDC/CI for external monitors and WMI for laptop panels (brightness only).
  - [PowerDisplay design doc](https://microsoft.github.io/PowerToys/modules/powerdisplay/design/)
  - Controls offered: brightness, contrast, volume, input source, rotation, colour temperature, power state; profiles; Light Switch integration — [Microsoft Learn: Power Display](https://learn.microsoft.com/en-ca/windows/powertoys/power-display)
- **Twinkle Tray:**
  - "Click the icon to bring up the Adjust Brightness flyout. Click away to hide the flyout." Hotkeys can target specific or all displays.
  - A CLI (v1.13.0+) takes `--MonitorNum`, `--Offset`, and `--Overlay`/`--Panel` to show feedback. It "Uses your Personalization settings to match your taskbar."
  - [Twinkle Tray README](https://cdn.jsdelivr.net/gh/xanderfrangos/twinkle-tray@master/README.md)
  - Hotkeys can send arbitrary VCP codes (contrast, volume, power state) — [Neowin / search summary](https://www.neowin.net/software/twinkle-tray-1170/)
- **Monitorian:**
  - Built with WPF (.NET Framework 4.8); has a tray icon flyout.
  - Supports precision-touchpad and mouse horizontal-wheel brightness changes, hot keys and shortcut keys (add-on), and a `/get` and `/set` command line. Rounded corners became the default on Windows 11 in v3.5. It shows at most 4 monitors at a time.
  - [Monitorian on GitHub](https://github.com/emoacht/Monitorian)
- **EarTrumpet** (volume, not DDC/CI, but the closest tray-flyout comparable): described as "a WPF app consisting of a notification area icon with a context menu, flyout and a volume mixer window", distributed through the Microsoft Store.
  - Source: a fork of the README — [README (fork)](https://github.com/wvware/EarTrumpet/blob/master/EarTrumpet/README.md); Store distribution per [official repo](https://github.com/File-New-Project/EarTrumpet). The official repo page itself did not state the framework.
  - Release notes mention Acrylic slowdown workarounds and a fix for "the flyout opened outside the work area" — [Deskmodder](https://www.deskmodder.de/blog/?p=158190)
- **Widgets Board:**
  - "Currently you can implement a widget provider using a packaged Win32 desktop app or a Progressive Web App (PWA)." — [Microsoft Learn: Widget providers](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/widget-providers) (updated 2026-07)
  - "In the current release, only packaged apps can be registered as widget providers." — [Microsoft Learn: Implement a widget provider (C#)](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-cs) (updated 2026-08-19)
- **App notifications (toasts):**
  - Windows App SDK `AppNotificationManager` is recommended for WinUI 3, WPF, WinForms and unpackaged Win32 apps. For unpackaged apps, `Register()` sets up COM activation automatically, with no manifest or AUMID work, given `<WindowsPackageType>None</WindowsPackageType>` — [Microsoft Learn: App notifications (console/unpackaged)](https://learn.microsoft.com/en-us/WINDOWS/APPS/develop/notifications/app-notifications/app-notifications-console). This is a search-result summary; the page body was not fetched.
  - Notifications support buttons that can act "without launching the app's UI". Clicking a notification when the app isn't running COM-launches it. Not supported for elevated apps: `Show` "will fail silently" — [Microsoft Learn: App notifications quickstart](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-quickstart)
- **Jump lists:**
  - `System.Windows.Shell.JumpList` is in WPF's PresentationFramework.dll, available from .NET Framework 4.0 through windowsdesktop-11.0. It shows on the taskbar button (right-click) and Start.
  - A `JumpTask` launches a program with `Arguments`. It can be declared in App.xaml, or set in code with `JumpList.SetJumpList` and `Apply()`.
  - No packaging requirement is stated.
  - [Microsoft Learn: JumpList class](https://learn.microsoft.com/en-us/dotnet/api/system.windows.shell.jumplist)

### Inferences
- The industry-standard answer to "Quick Settings for monitors" is a tray flyout. Microsoft itself chose it for Power Display in 2026.
- ThinkVisionControl's existing tray icon and hotkeys are already on the right path.
- Twinkle Tray's "--Overlay" and Power Display's hotkey flyout suggest one more pattern worth copying: a hotkey that shows a transient OSD-style overlay, like the built-in volume/brightness OSD.

### Gaps
- No source confirmed whether Twinkle Tray or Monitorian change brightness by scrolling the mouse wheel over the tray icon itself, as opposed to over the flyout slider. Monitorian's README mentions wheel/touchpad but the context (icon vs slider) wasn't confirmed.
- Not checked: Windows Spotlight. It is a Microsoft content feed (lock screen/desktop background) with no known third-party app-control surface; no source was gathered, so treat it as not applicable.
- EarTrumpet's current Windows 11 flyout tech (pure WPF vs XAML Islands) wasn't confirmed from the official repo.

## 4. Each viable surface: effort, requirements, resource cost, UX fit (ranked)

### Takeaway
Ranked for an unpackaged WPF DDC/CI utility:
1. Tray icon plus anchored flyout with sliders.
2. Global hotkeys with a transient OSD.
3. Taskbar/Start jump list tasks for presets and input switching.
4. Toasts with action buttons, for event-driven prompts only.
5. Command line for scripting.
6. Widgets Board, which needs MSIX packaging and Windows App SDK, and has a weaker interaction fit.

Quick Settings is not available.

### Cited Findings
- Tray flyout is the documented pattern. Left-click on a notification-area icon "should display what the user would expect... For instance, it could show status text for a status icon, or a slider for the volume control." Right-click "should bring up a normal shortcut menu" — [Microsoft Learn: Notifications and the Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)
- Widget provider requirements, per [Microsoft Learn: Implement a widget provider (C#)](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-cs):
  - Windows App SDK **2.3.1+** in the current walkthrough.
  - Developer mode for sideload testing.
  - A Windows Application Packaging Project (MSIX).
  - A `com:ExeServer` COM server registration.
  - A `uap3:AppExtension Name="com.microsoft.windows.widgets"` declaration.
  - Content is Adaptive Cards JSON with `Action.Execute`.
  - "the only supported widget host is the Widgets Board".
  - Distribution through the Microsoft Store is suggested.
- Widget lifetime: the host calls `Activate` and `Deactivate` per widget. The sample provider exits when no widgets are enabled, which implies COM activation on demand rather than a resident process — [same doc](https://learn.microsoft.com/en-us/windows/apps/develop/widgets/implement-widget-provider-cs)
- Toast requirements and limits (unpackaged supported; not elevated) — [Quickstart](https://learn.microsoft.com/en-us/windows/apps/develop/notifications/app-notifications/app-notifications-quickstart); [unpackaged guidance](https://learn.microsoft.com/en-us/WINDOWS/APPS/develop/notifications/app-notifications/app-notifications-console)
- Jump lists need no packaging and are in-box WPF — [JumpList class](https://learn.microsoft.com/en-us/dotnet/api/system.windows.shell.jumplist)
- The legacy Win32 guidance says the notification area "is not for critical information... It is also not intended for quick program or command access. As of Windows 7, much of that functionality is best accomplished through an application's taskbar button" — [Microsoft Learn: Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area) (doc ms.date 2018)
  - This is contradicted in practice by Microsoft's own 2026 PowerToys Power Display, which uses a tray icon and flyout — [PowerDisplay design doc](https://microsoft.github.io/PowerToys/modules/powerdisplay/design/)

### Ranked surface table (synthesis; requirements cited above, effort/cost estimates are inferences)

| Rank | Surface | Unpackaged WPF? | Requirements | Resident process? | Effort | UX fit for brightness/contrast/input |
|---|---|---|---|---|---|---|
| 1 | Tray icon left-click to anchored flyout (sliders, input picker, presets) | Yes | `Shell_NotifyIcon` (or WinForms `NotifyIcon`), `Shell_NotifyIconGetRect` / `CalculatePopupWindowPosition`, optional DWM acrylic backdrop | Yes (already true for this app) | Medium (positioning, focus/dismiss, DPI, multi-monitor taskbar) | Best: same mental model as the volume/network flyouts; what Power Display, Twinkle Tray and Monitorian do |
| 2 | Global hotkeys and a transient OSD overlay | Yes | `RegisterHotKey`; small topmost no-activate window | Yes | Low–medium (hotkeys already exist) | Excellent for repeat adjustments; mirrors the built-in brightness OSD |
| 3 | Jump list tasks (e.g. "Switch to HDMI 1", "Preset: Night") | Yes | `System.Windows.Shell.JumpList`; tasks relaunch the exe with arguments, so the single-instance app forwards args | No (shell persists the list) | Low | Good for discrete actions, poor for sliders. Visible only from a taskbar button or Start entry (pinned or running) |
| 4 | Toasts with buttons (e.g. "Monitor reconnected — restore preset?") | Yes, via Windows App SDK `AppNotificationManager` (adds a WinAppSDK runtime dependency) | Not elevated; `Register()` handles COM activation when unpackaged | No (COM-launches the app on click) | Medium (adds Windows App SDK dependency) | Situational only. Not a control surface, and toasts shouldn't be spammed |
| 5 | Command-line verbs (`--set brightness=…`) | Yes | Argument parsing; forward to running instance | No | Low | For power users, Task Scheduler and other launchers (Stream Deck etc.) |
| 6 | Windows 11 Widgets Board | **No**: needs MSIX/package identity | Windows App SDK 2.3.1+, COM ExeServer, widget AppExtension, Adaptive Cards | No (COM-activated on demand) | High (packaging, signing, Store or sideload, a second UI technology) | Weak: the board is a separate full-height panel, Adaptive Card input is limited, and users must add the widget manually |
| — | Quick Settings tile/slider | **No API** | — | — | Impossible (supported) / hack-only | — |

### Inferences
- Rank 1 and rank 2 cover almost all use cases, and both already sit on top of the resident process the app needs anyway: DDC/CI handles are cheap to keep and the app already runs in the tray.
- Widgets are the only "glanceable shell panel" that third parties can join, so they are the closest thing to Quick Settings. But the MSIX requirement and Adaptive Cards make it a poor return for this app.
  - Unverified inference: Adaptive Cards 1.5 has no native slider input (Input.Number, Input.ChoiceSet and Action.Execute buttons exist), so brightness would be +/- buttons or presets.
  - Unverified inference: a "packaged with external location" (sparse) package might give an otherwise unpackaged app the needed identity. Microsoft's widget docs only describe full MSIX packaging.
- Jump-list tasks are the cheapest addition with real value, especially for input switching and presets, which are discrete actions.

### Gaps
- Not verified whether Windows App SDK widget providers can use sparse/external-location packaging rather than full MSIX.
- Not verified whether current Adaptive Cards versions in the Widgets host support a slider input.
- No measured resource costs (RAM/CPU) for an idle WPF tray process vs a COM-activated widget provider were found.

## 5. Windows 11 tray flyout design: how apps implement an acrylic flyout anchored to the tray in WPF

### Takeaway
Use a borderless, non-activating-until-shown WPF window:
- Position it with `Shell_NotifyIconGetRect` and `CalculatePopupWindowPosition`, so it sits next to the icon inside the work area.
- Give it a Windows 11 backdrop through `DwmSetWindowAttribute(DWMWA_SYSTEMBACKDROP_TYPE = DWMSBT_TRANSIENTWINDOW)` ("Desktop Acrylic", 22H2+) and rounded corners.
- Dismiss it on deactivation.

Libraries such as WPF-UI also offer a tray service, but it has known bugs.

### Cited Findings
- Left-click should show a popup such as "a slider for the volume control". The popup "should be placed near the coordinate of the click in the notification area. Use the CalculatePopupWindowPosition to determine its location." The icon's bounding rectangle comes from `Shell_NotifyIconGetRect`. Use `NOTIFYICON_VERSION_4`, and identify icons by GUID on Windows 7+ — [Microsoft Learn: Notifications and the Notification Area](https://learn.microsoft.com/en-us/windows/win32/shell/notification-area)
- Legacy UX guide for tray menus: "Remove rather than disable any context menu items that don't apply." — [Notification Area UX guide](https://learn.microsoft.com/en-us/windows/win32/uxguide/winenv-notification) (via sibling note `settings_ux_patterns.md`)
- `DWM_SYSTEMBACKDROP_TYPE` values: `DWMSBT_TRANSIENTWINDOW` (3) = Desktop Acrylic, intended for transient surfaces such as flyouts; `DWMSBT_MAINWINDOW` (2) = Mica. Minimum build 22621 — [Microsoft Learn: DWM_SYSTEMBACKDROP_TYPE](https://learn.microsoft.com/en-us/windows/win32/api/dwmapi/ne-dwmapi-dwm_systembackdrop_type) (via sibling note `wpf_implementation_options.md`)
- WPF-UI provides `Wpf.Ui.Tray` / `INotifyIconService` — [WPF-UI API docs](https://wpfui.lepo.co/api/Wpf.Ui.Tray.INotifyIconService.html). Its tray package has open bugs since 2022 (icon change, unregister, recovery after an Explorer crash) — [lepoco/wpfui issues](https://github.com/lepoco/wpfui/issues?q=is%3Aopen+NotifyIcon) (via sibling note)
- Twinkle Tray matches the taskbar's personalization (light/dark/accent) so its flyout blends with the Windows 11 tray — [Twinkle Tray README](https://cdn.jsdelivr.net/gh/xanderfrangos/twinkle-tray@master/README.md). Monitorian adopted Windows 11 rounded corners by default in v3.5 — [Monitorian](https://github.com/emoacht/Monitorian)
- EarTrumpet's release notes report Acrylic performance workarounds and a fix for the flyout "open[ing] outside the work area", both common pitfalls — [Deskmodder](https://www.deskmodder.de/blog/?p=158190)
- On Windows 11, a reported issue says the `NIN_POPUPOPEN` (rich tooltip) message is not generated — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/588939/(windows-11)-vm-popupopen-message-is-not-generated). Don't rely on custom rich-tooltip popups.

### Inferences
- Implementation sketch for this app (keeping its existing WinForms `NotifyIcon`):
  1. On left click, get the icon rect (`Shell_NotifyIconGetRect`, needs the icon's hWnd/uID or GUID).
  2. Compute the position with `CalculatePopupWindowPosition`, aligned to the taskbar edge from `SHAppBarMessage(ABM_GETTASKBARPOS)`.
  3. Show a `WindowStyle=None`, `ShowInTaskbar=False`, `Topmost` window with a `DWMSBT_TRANSIENTWINDOW` backdrop and rounded corners.
  4. Close it on `Deactivated` or Esc.
  5. Add a slide-up entrance animation to match the shell flyouts.
- Handle the `TaskbarCreated` registered message so the icon is re-added after an Explorer restart. This is a known failure mode that WPF-UI issue #606 also hits.
- Handle multiple taskbars: the icon may sit on the primary taskbar while the user is working on the ThinkVision monitor. `CalculatePopupWindowPosition` keeps the window on the monitor containing the anchor point.

### Gaps
- Microsoft hasn't published Fluent design guidance specific to Windows 11 tray flyouts (size, padding, animation) for third-party Win32 apps. The only official guidance found is the legacy Win32 notification-area doc (ms.date 2018). Spacing and animation must be copied from the shell's own flyouts or from WinUI flyout specs.
- `TaskbarCreated` and `ABM_GETTASKBARPOS` behaviour on Windows 11 wasn't sourced in this session (standard Win32 knowledge, but uncited).
