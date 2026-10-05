# OS integration for porting ThinkVision Control (.NET 10 tray utility) to macOS and Linux

Research date: 2026-10-05. Scope: macOS 14–26 on Apple Silicon; Linux GNOME and KDE Plasma on X11 and Wayland. Items marked **[unverified]** come from prior knowledge or search snippets that could not be confirmed against a primary source in this session. Treat them as leads to check, not facts.

## 1. Global hotkeys (e.g. Ctrl+Alt+Up for brightness)

### Takeaway
- **macOS:** Carbon `RegisterEventHotKey` is the right API. It needs **no** Accessibility or Input Monitoring permission. The catch: since Sequoia, every hotkey must include at least one modifier other than Shift or Option. Ctrl+Alt+Up is fine because it includes Control.
- **Linux X11:** `XGrabKey` works.
- **Linux Wayland:** the only sanctioned route is the xdg-desktop-portal **GlobalShortcuts** portal.
  - KDE Plasma has implemented it since 5.27.
  - GNOME has implemented it since GNOME 48 (March 2025). GNOME 46 (Ubuntu 24.04 LTS) and older do not have it.
  - Always offer a CLI fallback (`thinkvision-control --step brightness=+5`) that users can bind as a desktop custom shortcut.
- **SharpHook** is a keyboard *hook*, not a hotkey registrar:
  - On macOS it needs Accessibility permission.
  - On Wayland it needs elevated privileges.
  - It is the wrong tool for this app.

### Cited Findings
**macOS**
- `RegisterEventHotKey` is "sandbox-safe and requires no permissions — unlike event taps". The sindresorhus/KeyboardShortcuts library builds on it and answers "Does this package cause any permission dialogs? No." It also describes itself as "fully sandboxed and Mac App Store compatible". It uses Carbon because "there are some [Carbon APIs] left that Apple never shipped modern replacements for. This includes registering global keyboard shortcuts." — [KeyboardShortcuts README](https://github.com/sindresorhus/KeyboardShortcuts)
- macOS Sequoia restriction, from an Apple Frameworks Engineer: "This was an intentional change in macOS Sequoia to limit the ability of key-logging malware to observe keys in other applications… shift+option can be used to generate alternate characters in passwords… macOS Sequoia now requires that a hotkey registration use at least one modifier that is not shift or option." If you break the rule, registration fails with error -9868 (eventInternalErr). — [Apple Developer Forums thread 763878](https://developer.apple.com/forums/thread/763878?page=2)
- A search snippet claims macOS 15.2 beta 2 relaxed this again to allow Option-only and Option+Shift hotkeys. **[unverified]**: the fetched forum page did not contain that statement. — [Apple Developer Forums thread 763878](https://developer.apple.com/forums/thread/763878?page=2)
- An NSEvent global monitor (`addGlobalMonitorForEventsMatchingMask:handler:`) "may only monitor key events if accessibility is enabled or if the application is trusted for accessibility (see AXIsProcessTrusted)". It is also observe-only, so it cannot consume the keystroke. — [Apple "Monitoring Events" guide](https://developer.apple.com/library/mac/documentation/cocoa/conceptual/eventoverview/MonitoringEvents/MonitoringEvents.html)
- An alternative is a listen-only `CGEventTap`. It needs the **Input Monitoring** privilege rather than Accessibility. You can check and request that privilege with `CGPreflightListenEventAccess` and `CGRequestListenEventAccess`. — [Apple Developer Forums, "Accessibility permission in sandboxed app"](https://developer.apple.com/forums/thread/707680) (via search snippet)

**Wayland portal**
- **The GlobalShortcuts portal API** (`org.freedesktop.portal.GlobalShortcuts`):
  - **Methods:** `CreateSession`, `BindShortcuts` and `ListShortcuts`. `ConfigureShortcuts` was "added in version 2 of this interface".
  - **Signals:** `Activated`, `Deactivated` and `ShortcutsChanged`.
  - **Shortcut definition:** each shortcut has an id, a description and an optional `preferred_trigger` in XDG shortcuts format.
  - **BindShortcuts:** typically shows a configuration dialog, and "An application can only attempt to bind shortcuts of a session once."
  - **Result:** the bound shortcuts come back with a `trigger_description` for display in the app's UI.
  - — [xdg-desktop-portal GlobalShortcuts docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.GlobalShortcuts.html)
- The GlobalShortcuts interface was introduced in xdg-desktop-portal 1.16.0. — [Flathub Discourse: Portals 1.16.0 release](https://discourse.flathub.org/t/portals-1-16-0-release-is-out/3404) (via search snippet)
- **KDE:** Plasma 5.27 (Feb 2023) added "support for the global shortcuts portal on Wayland". — [Phoronix: KDE Plasma 5.27 Released](https://www.phoronix.com/news/KDE-Plasma-5.27-Released) (via search snippet)
- **GNOME:**
  - "the GNOME desktop portal now supports the Global Shortcuts interface… Applications can register desktop-wide shortcuts, and users can edit and revoke them through the system settings." — [This Week in GNOME #189 (Feb 2025)](https://thisweek.gnome.org/posts/2025/02/twig-189)
  - The feature shipped in **GNOME 48**. Apps must ask permission to add global shortcuts, and users can later disable an app's shortcuts in Settings. — [UbuntuHandbook: GNOME 48 RC with Global Shortcuts](https://ubuntuhandbook.org/index.php/2025/03/gnome-48-rc-global-shortcuts-hdr-luminance/); [GNOME 48 "What's new for developers"](https://release.gnome.org/48/developers/)
- **App ID for non-Flatpak apps:**
  - A non-sandboxed app gets an app ID through `org.freedesktop.host.portal.Registry.Register`. The ID "must be able to match the basename of a .desktop file".
  - "Registering must be done before any portal method call", and "can only be done at most once".
  - Apps should re-register when `NameOwnerChanged` shows the portal restarted.
  - — [Registry interface docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.host.portal.Registry.html)

**.NET libraries**
- **SharpHook:**
  - The README says "Wayland support lacks several features and requires elevated privileges", and that "macOS requires that the access to the Accessibility API be enabled for the application if it wants to create a global hook".
  - It targets ".NET 10+, .NET Framework 4.7.2+, and .NET Standard 2.0".
  - — [SharpHook README](https://github.com/TolikPylypchuk/SharpHook)
- **SharpHook v8.0.0:**
  - It no longer uses X11 APIs by default on Wayland.
  - It ships four libuiohook binaries: `libuiohook.so`, `-xrecord`, `-x11` and `-wayland`.
  - You can choose the backend with `ILinuxBackendProvider` and query features with `IFeatureProvider`.
  - It moved to libuiohook 2.0.0 and dropped explicit net8/net9 targets.
  - — [SharpHook v8.0.0 release notes](https://newreleases.io/project/github/TolikPylypchuk/SharpHook/release/v8.0.0)
- **Tmds.DBus** is the .NET D-Bus library.
  - Its newer `Tmds.DBus.Protocol` package and the `dotnet dbus` tool generate C# proxies from D-Bus introspection. — [Tmds.DBus README](https://github.com/tmds/Tmds.DBus)
  - It targets .NET Standard 2.0 / .NET 6+. — [Red Hat Developer: Connecting .NET Core to D-Bus](https://developers.redhat.com/index%2ephp/blog/2017/09/18/connecting-net-core-d-bus)
  - I found no published C# example for GlobalShortcuts. The Rust crate `ashpd` has a reference wrapper. — [ashpd global_shortcuts](https://fractal-tobias-kuendig-a2308b584e67215e00d29986814e3bc2ee19fca1.pages.gitlab.gnome.org/ashpd/desktop/global_shortcuts/struct.GlobalShortcuts.html)

### Inferences
**macOS**
- Call `RegisterEventHotKey` / `InstallEventHandler` (Carbon.framework) directly via P/Invoke. This needs no permission prompt.
- Avoid Shift+Option-only combos. Ctrl+Alt+Arrow and Ctrl+Cmd+Arrow are both safe.
- Do not use SharpHook or NSEvent global monitors on macOS. Both trigger the Accessibility prompt, which a brightness utility does not need.
- NHotkey has no macOS or Linux backends **[unverified]**, so it does not help here.

**Linux X11**
- Use P/Invoke to libX11:
  - `XGrabKey` on the root window. Grab with the NumLock (Mod2) and CapsLock (Lock) variants as well, or the hotkey fails whenever those locks are on.
  - Use `XKeysymToKeycode` to translate key symbols.
  - Run an event loop on a dedicated thread with its own `Display*` **[unverified detail from prior knowledge]**.
- On a Wayland session, an `XGrabKey` from an XWayland client only fires while an X11 window has focus. Do not rely on it under Wayland **[unverified, prior knowledge]**.

**Linux Wayland**
1. Detect the session with `XDG_SESSION_TYPE=wayland`.
2. If the app is not a Flatpak, call `Registry.Register("<app-id>")` first. The app ID must match the installed `.desktop` file.
3. `CreateSession`.
4. `ListShortcuts`. If none are returned, call `BindShortcuts` with `preferred_trigger` values like `CTRL+ALT+Up`. This shows a one-time compositor dialog.
5. Listen for `Activated`.

Things to plan for:
- The compositor or the user decides the final trigger, so show the returned `trigger_description` in the app.
- **Coverage as of October 2026:**
  - Supported: KDE Plasma 5.27+ and 6.x, GNOME 48+ (for example Fedora 42+ and Ubuntu 25.04+).
  - Not supported: **Ubuntu 24.04 LTS (GNOME 46)**, which is still the most common LTS. It needs the CLI fallback. This mapping is from release history: Ubuntu 24.04 ships GNOME 46 **[unverified in this session]**.

**Fallback (always ship it)**
- Ship the CLI command `thinkvision-control --step brightness=+5`. It forwards over the single-instance socket (section 4).
- Document how to bind it as a desktop custom shortcut:
  - GNOME: Settings → Keyboard → Custom Shortcuts.
  - KDE: System Settings → Shortcuts → Add Command.

### Gaps
- I could not confirm the exact `xdg-desktop-portal` version that added the Registry interface. Nor could I confirm whether GNOME's portal rejects GlobalShortcuts from unregistered host apps or just handles them poorly.
- Exact SharpHook Wayland mechanism: it is likely evdev/libinput and therefore needs `input` group membership, but this is unconfirmed.
- I found no source on whether the portal triggers distinguish key repeat. That matters for holding Ctrl+Alt+Up.

## 2. Tray / menu bar

### Takeaway
- **macOS:**
  - The menu-bar item is an NSStatusItem. Avalonia's `TrayIcon` maps to it, and clicking opens the menu.
  - Hide the Dock icon with `LSUIElement=true` in Info.plist or the Accessory activation policy. Avalonia exposes this as `MacOSPlatformOptions.ShowInDock = false`.
- **Linux:**
  - Avalonia's tray uses StatusNotifierItem/AppIndicator over D-Bus.
  - KDE supports it natively.
  - Stock GNOME does not. It needs the "AppIndicator and KStatusNotifierItem Support" extension, which is preinstalled on Ubuntu but not on stock Fedora Workstation.
  - GNOME's own optional "Status Icons" extension only handles XEmbed and explicitly does not support AppIndicators.
- **Anchoring a flyout:**
  - Practical on macOS.
  - Unreliable on Linux: SNI click coordinates are not guaranteed, and Wayland clients cannot position windows anyway.

### Cited Findings
**Avalonia TrayIcon**
- `TrayIcon` works on Windows and macOS, and on Linux "on distributions with StatusNotifierItem or AppIndicator" (confirmed working on Ubuntu).
- Click behaviour: on macOS "clicking the tray icon shows the menu". On Windows, right-click shows the menu and left-click runs the `Command`.
- A tray menu must use `NativeMenu`.
- From the docs: "GNOME users may need the AppIndicator extension."
- — [Avalonia TrayIcon docs](https://docs.avaloniaui.net/docs/reference/controls/tray-icon)

**Avalonia on Wayland**
- Avalonia's Wayland write-up: "Need to show a system tray icon? That's the StatusNotifierItem protocol on KDE, but GNOME doesn't support it at all." It also notes that "setting window positions, system tray icons, or global hotkeys, are either missing or require compositor-specific extensions." — [Avalonia blog: Bringing Wayland Support to Avalonia (2025-09-19)](https://avaloniaui.net/blog/bringing-wayland-support-to-avalonia)
- Avalonia 12.1 (2026-07-08) ships a native Wayland backend. It is "experimental, so it isn't picked up by `UsePlatformDetect()` yet". You enable it with the `Avalonia.Wayland` package and `UseWayland()`. — [Avalonia 12.1 release notes](https://avaloniaui.net/blog/release-12-1)

**GNOME tray support**
- GNOME's official "Status Icons" extension (GNOME 47) "leverages the xembed support in gnome-shell". Per its author, "AppIndicators are not it, and there is no plan on supporting them." It is "NOT a default feature". — [OMG! Ubuntu, Aug 2024](https://www.omgubuntu.co.uk/2024/08/gnome-official-status-icons-extension)
- Canonical makes the AppIndicator/KStatusNotifierItem extension. It is packaged in Fedora as `gnome-shell-extension-appindicator`, preinstalled on Ubuntu, and historically not included in Fedora Workstation by default. Caveat: the Fedora thread is from 2019. — [Fedora desktop list thread](https://lists.stg.fedoraproject.org/archives/list/desktop@lists.fedoraproject.org/thread/DA2AEAAS4QDCBL3SSPKTHTLREKDFSZQV/); [Fedora Discussion, F40 tray icons](https://discussion.fedoraproject.org/t/fedora-40-and-gnome-46-not-working-with-tray-icons-reloaded-extensions/116989)

**StatusNotifierItem spec**
- The SNI spec is still a freedesktop draft "replacing the Freedesktop System tray specification".
- It defines the methods `ContextMenu`, `Activate`, `SecondaryActivate` and `Scroll`. The method details pages could not be fetched.
- — [StatusNotifierItem spec](https://specifications.freedesktop.org/status-notifier-item/latest/)

**macOS Dock hiding**
- `MacOSPlatformOptions.ShowInDock` controls whether the app shows in the Dock. The default is true. — [Avalonia API: ShowInDock](https://api-docs.avaloniaui.net/docs/P_Avalonia_MacOSPlatformOptions_ShowInDock)
- `NSApp.setActivationPolicy(.accessory)`: the app "does not appear in the Dock and does not have a menu bar, but it may be activated programmatically or by clicking on one of its windows". `LSUIElement=1` in Info.plist has the same effect from launch. — search snippets (macworld / cocoa-dev); the primary Apple doc page was not fetched **[unverified wording]**

### Inferences
**macOS**
- Use Avalonia `TrayIcon` (NSStatusItem) with a `NativeMenu`.
- Set `LSUIElement=true` in Info.plist rather than switching policy at runtime. This avoids a Dock icon flashing at launch. Also set `ShowInDock=false` for consistency.
- To anchor a flyout under the menu-bar icon, you need the status item button's screen frame (`statusItem.button.window.frame`). Avalonia's `TrayIcon` does not appear to expose this **[unverified]**.
- Options:
  - Use P/Invoke through the Objective-C runtime (`objc_msgSend`) to read it.
  - Position the flyout top-right under the menu bar. macOS menu-bar items are always at the top of the active screen, so this approximation works well.

**Linux**
- **Behaviour difference:** if the item sets `ItemIsMenu=true` or the host prefers it, left-click often opens the menu. The GNOME AppIndicator extension behaves this way; KDE sends `Activate` on left-click **[unverified, prior knowledge]**.
- Design for both:
  - Put every action, including "Open window", in the context menu.
  - Treat left-click activation as optional.
- **Popup anchoring:**
  - SNI `Activate(x, y)` passes click coordinates **[unverified: the spec page was not reachable]**.
  - On X11 you can position a window near them.
  - On Wayland a regular toplevel cannot set its own position, so show the flyout as a normal centered window. Better still, keep the quick controls in the native menu: brightness presets as menu items.
- **GNOME without the extension:** no tray at all.
  - Detect it: check whether `org.kde.StatusNotifierWatcher` exists on the session bus.
  - If absent, open the main window on launch.
  - Rely on `.desktop` actions (section 5) and hotkeys or the CLI.

### Gaps
- Exact text of SNI `Activate`/`ContextMenu` x,y semantics (spec subpages returned 404/403).
- Whether Avalonia's Linux `TrayIcon` raises `Clicked` on left-click under KDE vs GNOME-extension, and whether it exposes any icon geometry.

## 3. Start at login

### Takeaway
- **macOS 13+:** use `SMAppService.mainApp.register()`.
  - The app shows under its own name in System Settings → General → Login Items.
  - The user can disable it, which shows up as status `requiresApproval`.
- **Linux:** write `~/.config/autostart/<app-id>.desktop` per the XDG Autostart spec. This works on GNOME and KDE on X11 and Wayland.
- systemd user units are an alternative but rarely needed for a GUI tray app.

### Cited Findings
**macOS**
- "In macOS 13 and later, use `SMAppService` to register and control `LoginItems`, `LaunchAgents`, and `LaunchDaemons` as helper executables for your app."
- `mainApp` is "An app service object that corresponds to the main application as a login item".
- `register()` "Registers the service so it can begin launching subject to user approval".
- `openSystemSettingsLoginItems()` "Opens System Settings to the Login Items control panel".
- For agents, `register`/`unregister` replace "installing property lists in `~/Library/LaunchAgents`".
- Available on macOS 13.0+.
- — [Apple: SMAppService](https://developer.apple.com/documentation/servicemanagement/smappservice)
- `SMAppService.Status` values:
  - `enabled`
  - `notRegistered`
  - `requiresApproval`: registered but disabled by the user, or not yet approved in System Settings → General → Login Items & Extensions
  - `notFound`
  - — [Nil Coalescing: Launch at Login setting](https://nilcoalescing.com/blog/LaunchAtLoginSetting) (via search snippet)

**Linux**
- XDG autostart:
  - Default locations are `~/.config/autostart/` for the user and `/etc/xdg/autostart/` system-wide. When the same file name exists in several places, "only the file under the most important directory should be used".
  - `Hidden=true` means "the .desktop file MUST be ignored". A user can disable a system-wide entry by creating a local copy with `Hidden=true`.
  - `OnlyShowIn`/`NotShowIn` are mutually exclusive.
  - A non-empty `TryExec` that does not resolve to an installed executable means the entry "MUST NOT be autostarted".
  - — [XDG Autostart spec](https://specifications.freedesktop.org/autostart/latest/)

### Inferences
**macOS**
- Call `SMAppService` through P/Invoke via the Objective-C runtime (`objc_getClass("SMAppService")`, `sel_registerName("mainAppService")`, `register:`). You need to load `/System/Library/Frameworks/ServiceManagement.framework`. The Objective-C selector name is `mainAppService` **[unverified selector name]**.
- It only works from a real, signed `.app` bundle with a bundle ID. Expect failures when run from `dotnet run` **[unverified, but widely reported]**.
- Show the status in the app's settings UI. On `requiresApproval`, offer a button that calls `openSystemSettingsLoginItems`.
- Fallback for unsigned builds: write a LaunchAgent plist to `~/Library/LaunchAgents/` with `RunAtLoad` and `ProgramArguments` pointing at `Contents/MacOS/<exe> --background`. On macOS 13+ this still triggers the "Background Items Added" notification **[unverified]**.

**Linux**
- Write the autostart file with `Exec=<path> --background`, `X-GNOME-Autostart-enabled=true` (GNOME-specific, harmless elsewhere) and `TryExec`.
- **Flatpak builds:**
  - The sandbox cannot write the host autostart folder. Use `org.freedesktop.portal.Background.RequestBackground` with `autostart=true`, which writes the entry for you and may prompt the user.
  - **[unverified in this session — portal docs not fetched]**
- **AppImage:** `Exec` must point at the AppImage's own path (`$APPIMAGE` env var). If the file is moved, the entry breaks.
- **systemd user units:** systemd's `systemd-xdg-autostart-generator` already converts autostart entries to units on GNOME and KDE, so a hand-written `graphical-session.target` user unit adds little **[unverified]**.

### Gaps
- No primary-source confirmation fetched for the Background portal autostart option or for `SMAppService` behaviour with unsigned/ad-hoc-signed bundles.

## 4. Single instance and forwarding arguments

### Takeaway
- The existing named-pipe design ports directly. On macOS and Linux, .NET `NamedPipeServerStream`/`NamedPipeClientStream` are built on Unix domain sockets.
  - `PipeOptions.CurrentUserOnly` is enforced with socket permissions (0600) plus a peer UID check.
  - Message mode is unsupported, so you need your own framing.
  - `maxNumberOfServerInstances` is not enforced across processes, so the pipe cannot serve as the single-instance lock.
- On macOS, launching through Finder/Dock/`open` normally activates the running instance instead of starting a second one. Handle the reopen event to show the window.

### Cited Findings
**.NET pipes on Unix**
- Socket file mode: when `CurrentUserOnly` is set, the server sets the socket file mode to `UserRead | UserWrite` and checks the connecting peer's UID with `GetPeerID`.
- `PipeTransmissionMode.Message` throws `PlatformNotSupportedException`; only byte mode is supported.
- Instance limit: "We don't have a good way to enforce maxNumberOfServerInstances across processes; we only factor it in for streams created in this process."
- — [.NET source: NamedPipeServerStream.Unix.cs](https://source.dot.net/System.IO.Pipes/System/IO/Pipes/NamedPipeServerStream.Unix.cs.html)
- Unix domain socket path limits are about 107 bytes on Linux and 103 on macOS. macOS's per-user temp directory is long, so MSBuild uses `/tmp` for its pipe paths. — [.NET source: MSBuild NamedPipeUtil.cs](https://source.dot.net/MSBuild/NamedPipeUtil.cs.html) (via search snippet)

### Inferences
**Single-instance lock**
- Don't use a named Mutex for this. On Unix, .NET named mutexes are process-shared through files in a temp directory and are not robust across sessions or users **[unverified]**.
- Instead, take an exclusive lock on a lock file:
  - Linux: `$XDG_RUNTIME_DIR/thinkvision-control.lock`.
  - macOS: `~/Library/Application Support/<bundle-id>/instance.lock`.
  - Open it with `FileShare.None`, or use `flock` via P/Invoke.
  - Then open the socket.

**Socket path**
- You can also skip `NamedPipe*` and use `Socket` with `UnixDomainSocketEndPoint` directly.
  - Linux: put the socket at `$XDG_RUNTIME_DIR/thinkvision-control.sock`.
  - macOS: put it at `/tmp/<bundle-id>-<uid>.sock`, or in `NSTemporaryDirectory`, keeping it under 103 chars.
- This gives explicit control over the path and cleanup of stale socket files. Delete the file after confirming the lock is free.

**Length-prefixed framing**
- Use a length prefix (or newline-delimited JSON) for messages such as `--step`, `--show` and `--set`, because message mode is unavailable.

**macOS reopen**
- LaunchServices activates an already running bundle instead of starting a new one. The app receives `applicationShouldHandleReopen:hasVisibleWindows:` when the user double-clicks it again in Finder or Launchpad. With `LSUIElement`, there is no Dock icon to click.
- Avalonia exposes this through `IActivatableLifetime` (`Activated` event, `ActivationKind.Reopen`) **[unverified API name — confirm in Avalonia 11.1+/12 docs]**.
- When the app is started from a terminal (`Contents/MacOS/<exe> --step …`), LaunchServices is bypassed, so a second process starts. That process must forward its arguments over the socket and exit.
- `open -a ThinkVisionControl --args --step …` delivers the args only if the app is not already running **[unverified, prior knowledge]**. So the CLI should be the binary itself, or a small shell shim on PATH, not `open`.

### Gaps
- I did not fetch the Avalonia `IActivatableLifetime` docs.
- I did not fetch Apple's `applicationShouldHandleReopen` reference page.

## 5. Quick actions equivalent to the Windows jump list

### Takeaway
- **Linux:** `.desktop` file `Actions=` / `[Desktop Action id]` entries are a close match for the Windows jump list. They appear in the GNOME and KDE launcher context menus and run `Exec` commands such as `thinkvision-control --preset reading`.
- **macOS:** the closest match is the Dock menu (`applicationDockMenu:`, Avalonia `NativeDock.Menu`). It needs a Dock icon, which conflicts with a menu-bar-only app. For an `LSUIElement` app, the menu-bar item's menu *is* the quick-action surface.

### Cited Findings
**Linux**
- Desktop entries list actions in the `Actions` key. Each action has a `[Desktop Action %s]` group, where only `Name` is required.
  - `Name` is a "Label that will be shown to the user… as a submenu of a launcher".
  - `Exec` is "required if `DBusActivatable` is not set to `true`".
  - "Application launchers should expose them to the user (for example, as a submenu) within the context of the application."
  - — [Desktop Entry spec: Additional application actions](https://specifications.freedesktop.org/desktop-entry/latest/extra-actions.html)

**macOS**
- Avalonia supports a right-click Dock menu through `NativeDock.Menu` on the `Application` element. — [Avalonia macOS platform guide](https://docs.avaloniaui.net/docs/platform-specific-guides/macos)
- `ShowInDock` defaults to true. When it is false there is no Dock icon. — [Avalonia API: ShowInDock](https://api-docs.avaloniaui.net/docs/P_Avalonia_MacOSPlatformOptions_ShowInDock)

### Inferences
**macOS**
- One option is to hide the Dock icon by default (true tray-app behaviour). Then:
  - Put presets and input-source switching in the NSStatusItem menu.
  - Optionally support macOS **Shortcuts / App Intents** for Spotlight and Siri. That requires Swift and a real bundle, and is out of scope for pure .NET **[inference]**.
- The other option is a user setting "Show in Dock". When on, switch to the Regular activation policy and populate `NativeDock.Menu`.
  - Switching activation policy at runtime is supported by AppKit (`NSApp.setActivationPolicy`).
  - **[unverified: whether Avalonia's `ShowInDock` can be toggled after startup]**

**Linux**
- The actions need an installed `.desktop` file:
  - `.deb`, `.rpm` and Flatpak install one.
  - An AppImage needs AppImageLauncher or a first-run "integrate" step.
- On GNOME, actions appear on right-click in the dash/overview.
- On KDE, actions appear in Kickoff and in the task manager context menu **[unverified exact surfaces]**.
- **Desktop entry ID:** the `.desktop` file name should equal the reverse-DNS app ID, for example `io.github.<owner>.ThinkVisionControl.desktop`. Use the same ID for:
  - the portal Registry (section 1),
  - autostart (section 3),
  - Flatpak.

### Gaps
- No source fetched documenting exactly where KDE Plasma 6 shows desktop actions.

## 6. On-screen overlay (OSD) window: click-through, always on top, no focus

### Takeaway
- **macOS:** fully feasible with native calls: a borderless NSPanel/NSWindow with a high window level, `ignoresMouseEvents = YES` and no activation. Avalonia covers Topmost but not click-through, so the rest needs a small Objective-C runtime P/Invoke on the native handle.
- **Linux X11:** feasible, but click-through requires the XShape extension (an empty input shape).
- **Linux Wayland:**
  - KDE: feasible only through wlr-layer-shell, which KDE supports via layer-shell-qt. Avalonia does not expose it.
  - GNOME: no third-party layer-shell at all, and Mutter cannot be asked to keep a window on top or position it.
- **Recommendation:** use the native OSD window on macOS and X11. On Wayland fall back to a desktop notification (`org.freedesktop.Notifications` with a replace-id and progress hint), or skip the OSD entirely.

### Cited Findings
**Avalonia**
- Avalonia's window docs: "Avalonia currently does not support WPF-like transparent click-through behavior" (it points to native interop). `Topmost` is described as "Keep the window above all others." The docs add that "macOS and Linux have more limited support for transparency". — [Avalonia: How to work with Windows](https://docs.avaloniaui.net/docs/how-to/window-how-to)
- Maintainer answer: "with X11 click-through transparency is only available with the XShape extension. You can't do it with alpha-channel alone on that platform."
  - Avalonia renders on the GPU rather than with `UpdateLayeredWindow`, so Windows layered-window click-through does not apply.
  - — [Avalonia discussion #11911](https://github.com/AvaloniaUI/Avalonia/discussions/11911)
  - This contradicts search-engine summaries claiming `Background="{x:Null}"` gives click-through. That works for WPF-style hit-testing *within* a window, not for OS-level pass-through.

**Wayland**
- "The core protocol doesn't allow" setting window positions on Wayland. — [Avalonia blog: Bringing Wayland Support](https://avaloniaui.net/blog/bringing-wayland-support-to-avalonia)
- Mutter (GNOME) has an issue open for "implement layer_shell protocol". GNOME has no layer-shell for third-party apps and no compositor-agnostic way to position an overlay surface. The workarounds are a companion GNOME Shell extension called over D-Bus, or a plain xdg-toplevel. — [GNOME Mutter issue #973](https://gitlab.gnome.org/GNOME/mutter/-/issues/973); [Wayland issue #305: protocol for always-on-top windows](https://gitlab.freedesktop.org/wayland/wayland/-/issues/305) (status and comments not retrievable; via search snippet)
- **KDE:** layer-shell-qt is "a Qt component to allow applications to make use of the Wayland wl-layer-shell protocol". KOverlay is "a click-through, always-on-top overlay panel for Wayland desktops tested on Fedora KDE/Plasma Wayland, built with Qt 6 + QML + LayerShellQt". — [Gentoo: kde-plasma/layer-shell-qt](https://packages.gentoo.org/package/kde-plasma/layer-shell-qt); [KOverlay](https://github.com/erik96/koverlay)

### Inferences
**macOS**
- Get the `NSWindow*` from Avalonia (`TryGetPlatformHandle()` → `NSWindow` handle). Then use `objc_msgSend` to set:
  - `setIgnoresMouseEvents: YES`
  - `setLevel:` to `NSStatusWindowLevel` (25) or `NSPopUpMenuWindowLevel` (101)
  - `setCollectionBehavior:` to `CanJoinAllSpaces | FullScreenAuxiliary | Stationary`, so it shows over full-screen apps
  - `setHasShadow: NO`
- Show it with `orderFrontRegardless` rather than `makeKeyAndOrderFront`, so it never takes focus.
- Avalonia `ShowActivated=false` may be enough to avoid activation **[unverified]**.
- These AppKit APIs are long-standing. The constants should be confirmed **[unverified values]**.

**Linux X11**
- Topmost and positioning work.
- For click-through, P/Invoke `XShapeCombineRectangles(display, win, ShapeInput, 0, 0, NULL, 0, ShapeSet, Unsorted)` to set an empty input region.
- Set the window type hint `_NET_WM_WINDOW_TYPE_NOTIFICATION` or `_TOOLTIP`, plus `ShowActivated=false` / `ShowInTaskbar=false`.

**Linux Wayland**
- By default Avalonia apps run through XWayland (the native Wayland backend is opt-in in 12.1). The X11 OSD code then still "works", but:
  - GNOME/KDE may not honour position or always-on-top for XWayland windows over native windows.
  - Behaviour varies **[unverified]**.
- Practical approach:
  - On `XDG_SESSION_TYPE=wayland`, default the OSD to off.
  - Use `org.freedesktop.Notifications.Notify` with `replaces_id` (to update in place) and the `value` hint (0–100 progress bar), via Tmds.DBus. GNOME and KDE both render notifications. KDE shows the `value` hint as a progress bar; GNOME ignores it **[unverified]**.
- On KDE Wayland, a richer OSD would require a native layer-shell surface outside Avalonia, which is not worth the cost.

### Gaps
- Whether Avalonia 12.x's native Wayland backend exposes any xdg-activation or layer-shell hooks; the 12.1 notes mention none.
- Exact rendering of notification progress hints on GNOME Shell 49/50.

## 7. Theme detection (light/dark)

### Takeaway
- Avalonia already follows the OS theme by default (`ActualThemeVariant` with `RequestedThemeVariant=Default`) and fires change notifications, so most of this is free.
- **macOS:** Avalonia reads `NSApp.effectiveAppearance`.
- **Linux:** the portable source is the xdg-desktop-portal Settings key `org.freedesktop.appearance color-scheme`, which also offers accent colour and contrast.
- The tray icon is separate: it needs monochrome/template handling per platform.

### Cited Findings
**Avalonia**
- "By default, Avalonia inherits the theme variant set by the system-wide user preference". `ActualThemeVariant` returns the OS theme variant when no explicit variant is requested. — [Avalonia: Theme variants](https://docs.avaloniaui.net/docs/styling/theme-variants); [ActualThemeVariant API](https://api-docs.avaloniaui.net/docs/P_Avalonia_Application_ActualThemeVariant)
- `PlatformColorValues` carries dark-mode and accent-colour information (`ThemeVariant` property). — [Avalonia API: PlatformColorValues](https://api-docs.avaloniaui.net/docs/T_Avalonia_Platform_PlatformColorValues)

**Portal Settings**
- `org.freedesktop.appearance` keys:
  - `color-scheme` (u): "0: No preference, 1: Prefer dark appearance, 2: Prefer light appearance"
  - `accent-color` ((ddd)): RGB values in [0,1]
  - `contrast` (u): 0 normal, 1 higher
- `ReadOne` "was added in version 2". The `SettingChanged` signal fires on changes.
- — [xdg-desktop-portal Settings docs](https://flatpak.github.io/xdg-desktop-portal/docs/doc-org.freedesktop.portal.Settings.html)

### Inferences
**In-app**
- Subscribe to `Application.ActualThemeVariantChanged`, or `PlatformSettings.ColorValuesChanged`, to swap the Fluent palette.
- Fallback if Avalonia's Linux detection misses (for example portal absent on old distros): read the portal via Tmds.DBus.
  - Use `ReadOne`, falling back to the older `Read` method on v1 portals.
  - On GNOME, last resort: `gsettings get org.gnome.desktop.interface color-scheme`, which returns `'prefer-dark'` **[unverified, prior knowledge]**.

**macOS menu-bar icon**
- Supply a monochrome template image (`NSImage.template = YES`) so macOS tints it for light/dark menu bars and the selected state.
- Avalonia `TrayIcon` may not expose "template" **[unverified]**. If not, set it via P/Invoke, or swap icons on theme change. The menu bar's appearance can differ from the app's when the wallpaper tints it.

**Linux tray icon**
- Ship a symbolic-style icon (`*-symbolic` via `IconName`) where the theme provides it.
- Otherwise pick light or dark pixmaps from the `color-scheme`. Remember that the GNOME top bar is always dark **[unverified for GNOME 49/50]**.

### Gaps
- I did not confirm whether Avalonia's Linux `IPlatformSettings` reads the portal `color-scheme` (likely, but not checked in source).
- I did not confirm whether Avalonia's `TrayIcon` supports macOS template images.

## 8. Packaging and distribution

### Takeaway
**macOS**
- Ship a `.app` bundle built from `dotnet publish -r osx-arm64 -p:UseAppHost=true`, ideally single-file, with a hand-written Info.plist (including `LSUIElement`).
- Sign it with a **Developer ID Application** certificate, using hardened runtime and the `com.apple.security.cs.allow-jit` entitlement. Notarize with `xcrun notarytool` and staple. Wrap it in a DMG, which also needs notarizing and stapling.
- This needs the **$99/year Apple Developer Program**.
- Unsigned apps can still be opened, but since macOS 15 only via System Settings → Privacy & Security → "Open Anyway". The Control-click bypass is gone.

**Linux**
- **AppImage** or **.deb/.rpm**: the simplest options for an app that needs `/dev/i2c-*`, since there is no sandbox. The host-side requirement (i2c-dev module and a udev rule or `i2c` group) can be shipped in a .deb/.rpm.
- **Flatpak** is awkward:
  - There is no `--device=i2c`; you would need `--device=all`.
  - Host udev and group setup is still required.
  - Flathub may push back.
- **PupNet Deploy** builds AppImage, Flatpak, deb and rpm from one config. It has no macOS output.

### Cited Findings
**macOS bundle and signing**
- Avalonia's macOS deployment guide:
  - Publish with `dotnet publish -r osx-x64 --configuration Release -p:UseAppHost=true`; use `osx-arm64` for Apple Silicon.
  - Required Info.plist keys: `CFBundleExecutable`, `CFBundleIdentifier`, `CFBundleIconFile`, `NSHighResolutionCapable` and `CFBundleVersion`.
  - Sign with `codesign --force --timestamp --options=runtime --entitlements MyApp.entitlements --sign "Developer ID: …" MyApp.app`, signing nested files first. "Apple recommends avoiding `--deep`".
  - "The JIT-compiled code exception [`com.apple.security.cs.allow-jit`] is required to run Avalonia apps under hardened runtime."
  - Notarize with `xcrun notarytool store-credentials …`, then `ditto -c -k --sequesterRsrc --keepParent MyApp.app MyApp.zip`, `xcrun notarytool submit MyApp.zip --keychain-profile … --wait` and `xcrun stapler staple MyApp.app`.
  - For a DMG, notarize the `.app` and the `.dmg` separately and staple the DMG.
  - It recommends `PublishSingleFile=true` to simplify signing.
  - Notarization "Requires Developer ID Application certificate (paid subscription)".
  - — [Avalonia: Deploying on macOS](https://docs.avaloniaui.net/docs/deployment/macos)
- Hardened runtime is required for notarization, and .NET needs the `allow-jit` entitlement. — [Microsoft Xamarin blog: macOS hardened runtime & notarization](https://devblogs.microsoft.com/xamarin/macos-hardened-runtime-notary/)
- Apple Developer Program: "$99 annual membership". It covers distributing "Mac apps outside the Mac App Store using your Developer ID certificate and Apple's notarization process". — [Apple Developer Program](https://developer.apple.com/programs/)
- Apple support: "By default, macOS Catalina and later also requires software to be notarized". Users can override a block via System Settings → Privacy & Security → "Open Anyway". — [Apple Support 102445](https://support.apple.com/en-us/102445)
- Sources disagree on how hard the Sequoia bypass is:
  - Hackaday reports that from macOS Sequoia 15 the Control-click bypass is no longer available, and that 15.1 disables the bypass "completely". — [Hackaday](https://hackaday.com/tag/appledrm/)
  - Apple's support page still documents "Open Anyway". — [Apple Support 102445](https://support.apple.com/en-us/102445)
  - Treat it as: the bypass now takes the System Settings route.

**Flatpak**
- Flatpak `--device` values:
  - `dri`
  - `kvm`
  - `shm`
  - `input` (`/dev/input`)
  - `usb` (`/dev/bus/usb`)
  - `all`: "All devices, including all of the above except `shm`"
- The docs note: "While not ideal, `--device=all` can be used to access devices like webcams…"
- The docs do not mention i2c.
- — [Flatpak docs: Sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html); [flatpak-build-finish(1)](https://man7.org/linux/man-pages/man1/flatpak-build-finish.1.html)

**PupNet Deploy**
- Ships .NET apps "as: AppImage for Linux, Flatpak for Linux, Debian Binary Package, RPM Binary Package, Setup File for Windows, Zip and tar.gz".
- The tool targets .NET 10. It is AGPL-3.0 and actively maintained.
- Its Flatpak output "is unsuitable for use with Flathub" directly.
- It does not support macOS.
- — [PupNet Deploy GitHub](https://github.com/kuiperzone/PupNet-Deploy)

### Inferences
**macOS bundle**
- Build `osx-arm64` only, unless Intel Macs on macOS 14–26 matter.
  - macOS 26 is described as the last release to support Intel Macs **[unverified]**.
  - A universal build would need `lipo` on two apphost publishes, which complicates single-file.
- Native libraries from the DDC/I2C layer (for example a bundled helper or `.dylib`) must each be signed with the same identity and hardened runtime before the bundle is signed.
- The `Dotnet.Bundle` NuGet ("dotnet-bundle", `dotnet msbuild -t:BundleApp`) can generate the `.app` structure and Info.plist. Its maintenance status as of 2026 is **[unverified]**.
- Put `LSUIElement=true` in Info.plist yourself.
- DMG: `hdiutil create -volname … -srcfolder … -ov -format UDZO` is enough. `create-dmg` adds the layout and background **[prior knowledge]**.
- Notarization needs an Apple ID with 2FA and an app-specific password or App Store Connect API key. This can run in GitHub Actions macOS runners.
- Without the $99 membership, ship ad-hoc signed or unsigned builds and document the "Open Anyway" step. Apple Silicon requires at least ad-hoc signing (`codesign -s -`) for native arm64 code to run **[unverified, prior knowledge]**.

**Linux hardware access**
- DDC/CI needs `/dev/i2c-*` read/write: the `i2c-dev` module loaded, plus a udev rule granting the `i2c` group or uaccess.
- ddcutil ships such a rule (`60-ddcutil-i2c.rules`, uaccess-based) **[unverified file name]**.
- A `.deb`/`.rpm` can install the udev rule and a `modules-load.d` entry. An AppImage and a Flatpak cannot, so they need a documented one-time setup command or `pkexec` helper.

**Linux formats**
- **Flatpak:**
  - Device access needs `--device=all`.
  - Global shortcuts and autostart then go through portals (sections 1 and 3).
  - Tray works through SNI on the session bus with `--talk-name=org.kde.StatusNotifierWatcher` **[unverified exact finish-arg]**.
  - Viable for Flathub only if reviewers accept `--device=all` for monitor control. Flathub's policy on this was not checked.
- **Recommended Linux mix:** AppImage plus .deb/.rpm via PupNet for the first release. Flatpak later.
- **macOS:** handle separately with a script: publish → assemble bundle → codesign → notarize → DMG.

### Gaps
- I did not fetch Flathub's review policy on `--device=all`.
- I did not confirm the Dotnet.Bundle (dotnet-bundle) maintenance status or Avalonia's current recommended macOS bundling tool. The Avalonia deployment page shows the manual method only.
- I did not confirm whether `PublishSingleFile` + `IncludeNativeLibrariesForSelfExtract` causes notarization or library-validation issues with extracted native libs. If so, it may need `com.apple.security.cs.disable-library-validation`.
- I did not confirm whether macOS 26 changes Gatekeeper further.
