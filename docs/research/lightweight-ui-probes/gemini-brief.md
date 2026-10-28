# Brief given to Gemini

```text
IMPORTANT: Do not run shell commands and do not read or write local files. Use only web search / web fetch, then answer directly in your reply.

RESEARCH BRIEF (you have no prior context; everything you need is here). Cite every claim with a URL. Mark anything you cannot source as [unsourced]. Today is 2026-10-27; prefer 2025-2026 sources.

QUESTION
Which cross-platform desktop UI approach lets a small GPL-2.0-or-later utility app ship under 10 MB installed/download size and cold-launch in under 2 seconds on Windows 11 (x64), macOS (Apple Silicon) and Linux (X11 and Wayland, GNOME and KDE), while staying stable and maintainable?

THE APP ("ThinkVision Control")
Controls a Lenovo monitor over DDC/CI (I2C over the video cable). Features: a settings window (~3 pages: sliders, combo boxes, toggle switches, buttons, a dialog, a shortcut-capture editor); a tray / menu-bar icon with a menu and (on Windows) a small flyout window anchored at the tray; system-wide global hotkeys; a small transparent always-on-top OSD overlay showing a level bar; start at login; single instance + command line forwarding; light/dark theme following the OS. Background mode must idle with near-zero CPU and low memory.
Platform DDC access: Windows dxva2.dll (GetVCPFeature/SetVCPFeature); macOS Apple Silicon IOAVService private API (IOAVServiceReadI2C/WriteI2C); Linux /dev/i2c-* ioctl.

CURRENT STATE (established)
- .NET 10 + Avalonia 12.1.3 + FluentAvalonia 3.1.0. Shared C# core ~3,100 lines (DDC/MCCS codec, queue, profile/settings JSON, per-OS DDC links). UI + per-OS platform layer ~7,800 lines (tray, hotkeys via RegisterHotKey / Carbon / X11 XGrabKey + XDG GlobalShortcuts portal, OSD, autostart, DBus StatusNotifierItem on Linux).
- Measured on Windows: Avalonia NativeAOT minimal app = 34.8 MB (exe 16.8, libSkiaSharp 11.1, ANGLE libGLESv2 5.1, HarfBuzzSharp 1.7); warm start ~0.95 s, cold ~3.3 s; software rendering idle ~13 MB private.
- Linux release today is self-contained JIT: 100 MB unpacked, 41 MB tar.gz. Windows uses framework-dependent WPF (1 MB app) plus a 60 MB .NET Desktop Runtime download.
- Conclusion already reached: Avalonia cannot get under 10 MB because Skia alone is ~11 MB.

CANDIDATES TO COMPARE (each one)
Qt 6 (Widgets vs QML/Quick; static vs dynamic linking; LGPL obligations when statically linking a GPL app; Qt on Linux as system library), GTK 4 (and its practicality on Windows and macOS: bundle size, look, tray support since GtkStatusIcon was removed), wxWidgets 3.2/3.3 (native widgets; wxTaskBarIcon on GNOME/Wayland), Slint (Rust/C++; licence options vs GPL-2.0-or-later; renderer choices femtovg/skia/software; tray story), egui/eframe, iced, Tauri 2 (system webview: WebView2 / WKWebView / WebKitGTK; real memory cost), Fyne (Go), Flutter desktop, Dear ImGui, native-per-OS UI (Win32 or WinUI / AppKit or SwiftUI / GTK) over a shared core library, and staying on .NET with NativeAOT but swapping Avalonia for thin native bindings (Win32 directly, Gir.Core GTK4 bindings AOT status, ObjC interop for AppKit, WinForms NativeAOT status).

FOR EACH CANDIDATE REPORT
1. Realistic shipped size per OS for a small app like this (binary + any bundled libs; note what Linux distros already provide). Give measured numbers from sources where possible.
2. Cold/warm startup time and idle memory, sourced.
3. Tray/menu-bar icon, global hotkeys, transparent always-on-top overlay window — support on Windows, macOS, Linux X11, Linux Wayland (GNOME, KDE). Name the library/crate used (e.g. tray-icon, global-hotkey crates; KStatusNotifierItem; xdg-desktop-portal GlobalShortcuts; wlr-layer-shell vs GNOME).
4. Accessibility (screen readers) per OS.
5. Licence compatibility with a GPL-2.0-or-later app (note Slint's GPLv3 / royalty-free options; Qt LGPLv3 static-link implications).
6. Maturity / maintenance status as of 2026; known stability problems.
7. Rewrite cost: language change from C#, and whether existing DDC libraries exist (e.g. Rust ddc-hi / ddc-winapi / ddc-i2c / ddc-macos Apple Silicon support; C/C++ libddcutil; Go equivalents).

OUTPUT SHAPE
- A comparison table: candidate | Win size | macOS size | Linux size | cold start | idle RAM | tray/hotkey/overlay coverage incl. Wayland | a11y | licence fit | rewrite cost.
- Then: top 3 recommendations ranked, with the reasoning and the biggest risk of each.
- Then: a list of claims you could not verify.
Cite every claim with a URL.
```
