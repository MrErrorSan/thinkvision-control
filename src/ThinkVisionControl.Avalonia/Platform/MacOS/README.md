# macOS platform layer

`MacPlatform.Create(createLink, overlayOsd)` builds the macOS `PlatformServices`. Call it on the UI (main) thread after Avalonia has started.

| Service | How |
|---|---|
| `MacHotkeys` | Carbon `RegisterEventHotKey` + `InstallEventHandler` on `GetApplicationEventTarget()`. There is no permission prompt. Shortcuts whose only modifiers are Shift and/or Option fail, by the Sequoia rule. So do F21–F24, which macOS doesn't have |
| `MacMonitorEvents` | `CGDisplayRegisterReconfigurationCallback` ("after" calls only) plus `IORegisterForSystemPower` (`kIOMessageSystemHasPoweredOn`). Both are debounced 500 ms and raised on the UI thread. Sleep requests are acknowledged with `IOAllowPowerChange` |
| `MacAutostart` | `SMAppService.mainAppService` (macOS 13+) through `objc_msgSend`. Works only from an `.app` bundle; otherwise it reports off and does nothing. `requiresApproval` opens Login Items and reports off |
| `MacQuickActions` | No-op. The menu-bar menu is the quick-action surface |
| `UnixSingleInstance` | `flock` on `$TMPDIR/thinkvision-control.lock` (via `FileShare.None`). Arguments travel over the Unix socket `$TMPDIR/CoreFxPipe_tvc-cmd`, which is current-user only |
| `MacThemeInfo` | `NSApp.effectiveAppearance.name`. Accent is null (FluentAvalonia reads it). `Changed` comes from `Application.ActualThemeVariantChanged` |
| `MacWindowTweaks.MakeOverlay` | `setIgnoresMouseEvents:YES`, `setLevel:25`, `setCollectionBehavior:337` on the OSD's `NSWindow`. Applied again on every `Opened` |
| `MacSystemSetup` | One card: `UnsupportedPort`, "Use a USB-C or Thunderbolt port" |

Pure logic lives in `MacRules` (no platform attribute, so it is unit-testable on Windows): VK→kVK, modifiers, the Sequoia rule, appearance names, window flags and bundle path.

## Verified vs unverified

- **Verified:** it compiles (`dotnet build src/ThinkVisionControl.Avalonia -r osx-arm64` and the plain build give 0 warnings and 0 errors).
- **Unverified:** everything at runtime. No Mac was available, so every P/Invoke and `objc_msgSend` signature has only been checked against the headers from memory. These are the least certain:
  - Carbon hot-key events reach a handler on `GetApplicationEventTarget()` inside Avalonia's `NSApplication` loop. KeyboardShortcuts uses `GetEventDispatcherTarget()` instead; switch to that if keys don't fire.
  - Avalonia re-applying the window level on show. This is why the tweaks run again on `Opened`.
  - The pixel convention of `Window.Position` on macOS (points × scale, top-left origin). Mixed-DPI setups may be off.
  - `SMAppService` with an ad-hoc-signed bundle.

## First run on a Mac (checklist)

1. Build the bundle with `scripts/make-macos-app.sh` and launch it. The menu-bar icon appears and there is **no Dock icon**.
2. Press Ctrl+Alt+Up and Ctrl+Alt+Down. Brightness changes, the OSD shows, and **no permission prompt** appears. A Shift+Option-only shortcut is reported as failed.
3. With a full-screen app in front, trigger the OSD. It shows on top, and clicks pass through it to the app.
4. Turn on "Start at login". The app appears in System Settings → General → Login Items, or that pane opens for approval. Log out and back in to confirm that it starts.
5. Unplug and replug the monitor (USB-C). The app reconnects within a few seconds. Do the same after sleep and wake (close the lid, then open it).
6. While the app is running, run `"ThinkVision Control.app/Contents/MacOS/thinkvision-control" --step brightness=+5`. The running instance applies it and the second process exits.
7. Switch Appearance between Light and Dark. The app follows.
8. Click the menu-bar icon, or choose Open ThinkVision Control from its menu. The main window opens, or comes to the front if it's already open.
