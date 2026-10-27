---
name: Windows installer and fast start
overview: Replace the 174 MB self-contained exe with a small Inno Setup installer that installs the .NET 10 Desktop Runtime only when missing, and cut WPF start-up to under 3 s.
todos:
  - id: publish-mode
    content: Framework-dependent, ReadyToRun folder publish for the WPF app (no single-file bundle).
    status: completed
    parallel: true
  - id: app-lifecycle
    content: Drop the app's own Start menu shortcut, defer the jump list, never let a second launch quit silently.
    status: completed
    parallel: true
  - id: queue-exit
    content: DdcQueue.Dispose waits at most ~1 s for an in-flight monitor command.
    status: completed
    parallel: true
  - id: installer
    content: Inno Setup script + build script producing ThinkVisionControl-Setup-<version>.exe.
    status: completed
    depends-on: publish-mode
  - id: docs-version
    content: Version 0.0.3, README Windows install section.
    status: completed
    depends-on: installer
---

# Windows installer and fast start

## Goal

- Users download one `ThinkVisionControl-Setup-<version>.exe` of a few MB, not a 174 MB exe.
- The installer picks the install folder (default `C:\Program Files\ThinkVision Control`), offers desktop and Start menu
  shortcuts as checkboxes, and installs the .NET 10 Desktop Runtime only if it's missing and the box is ticked.
- If the runtime is missing anyway, launching the app shows the .NET host's own message naming the missing runtime,
  with a download button. No crash.
- The window appears in under 3 s.

## Target Files

- [src/ThinkVisionControl/ThinkVisionControl.csproj](../../../src/ThinkVisionControl/ThinkVisionControl.csproj): publish settings.
- [src/ThinkVisionControl/App.xaml.cs](../../../src/ThinkVisionControl/App.xaml.cs): start-up order, single instance.
- `src/ThinkVisionControl/Platform/StartMenuRegistration.cs`: deleted (the installer owns shortcuts).
- [src/ThinkVisionControl.Core/Ddc/DdcQueue.cs](../../../src/ThinkVisionControl.Core/Ddc/DdcQueue.cs): bounded dispose.
- [packaging/windows/ThinkVisionControl.iss](../../../packaging/windows/ThinkVisionControl.iss): installer.
- [scripts/make-windows-setup.ps1](../../../scripts/make-windows-setup.ps1): publish + compile the installer.

## Key design decisions

- **Runtime is downloaded, not bundled.** Bundling adds ~60 MB and breaks the 50 MB ceiling. The installer fetches
  `https://aka.ms/dotnet/10.0/windowsdesktop-runtime-win-x64.exe` (Microsoft's "latest 10.0" link) and runs it with
  `/install /quiet /norestart`. Exit codes 0, 1638 (newer present) and 3010 (reboot later) count as success.
- **Detection** = a `10.*` folder under `{commonpf64}\dotnet\shared\Microsoft.WindowsDesktop.App`. When found, the
  runtime task is not offered and the Ready page says it's already installed.
- **Shortcuts belong to the installer.** The app used to rewrite a Start menu shortcut on every launch, which would
  override an unticked checkbox and pointed at wherever the exe happened to run from. Removed. The installer also
  deletes that old per-user shortcut.
- **Admin install** (`Program Files` and the runtime both need it). `AppMutex` uses the app's single-instance mutex so
  setup and uninstall ask to close a running copy.
- **Start with Windows stays an app setting.** The app re-applies its Run key from its own settings on every start,
  so an installer checkbox would be overwritten. Uninstall removes the Run value.
- **A second launch never quits silently.** If forwarding to the running copy fails (it is shutting down), wait up to
  5 s for the single-instance mutex and start normally.
- **Exit is bounded.** `DdcQueue.Dispose` waits at most 1 s; a monitor command still running after that is abandoned
  (the process is exiting).

## Verification

- `dotnet test` passes, including: dispose returns within ~1 s while a monitor command blocks for 5 s.
- Publish size and time-to-window measured on this PC; target < 3 s.
- Missing-runtime message: run the published app with its runtimeconfig pointed at an absent framework version and
  confirm the host dialog appears (not a crash).
- Build the setup, install to a test folder, launch from the Start menu, close, launch again, uninstall.

## Open questions

- Runtime install cannot be tested end to end here (this PC already has .NET 10); the download URL was checked to
  resolve to 10.0.14 on 2026-10-27.
