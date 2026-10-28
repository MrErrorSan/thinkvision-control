# Slint background mode: tray, hotkeys, OSD (Windows)

A prototype built on the Slint look-alike to see how a Slint app behaves in the tray.

**Short run:** the owner cut scope, so this is 1 cycle, 5 s waits, a 30 s idle-CPU window, 10 latency presses and a 40-press burst. Linux, the Avalonia baseline and repeat cycles were skipped.

**Measured on:** Windows 11, Ryzen 5 5500U, 100 % scale.

**Build:** release, software renderer, 11.0 MB exe.

**Raw data:**
- [results.json](results.json), merged by `combine.py` from `results-slint-{hide,drop}.json`.
- [measure.ps1](measure.ps1) is the harness.

**Sources:** in [`../lookalike/slint/`](../lookalike/slint/), which now includes the background mode:
- `src/main.rs`
- `ui/background.slint`: `Tray` uses Slint's built-in `SystemTrayIcon`; `Osd` is the on-screen display.
- Run with: `lookalike-slint.exe --startup --close=hide|drop [--log=file]`.

## What was built

- **Tray:**
  - Slint's own `SystemTrayIcon` (feature `system-tray`).
  - Left click opens the window.
  - Right-click menu: Open, Day preset, Night preset, Quit.
  - The `tray-icon` crate wasn't needed.
- **Hotkeys:**
  - The `global-hotkey` 0.9 crate.
  - Ctrl+Alt+Up and Ctrl+Alt+Down step a fake brightness by 5 (0–100); Ctrl+Alt+O opens the window.
  - Its event handler calls `slint::invoke_from_event_loop`. There is no polling timer, and the loop is `run_event_loop_until_quit`.
- **OSD:**
  - A 264×64 frameless, always-on-top card.
  - On each step it shows at the bottom centre of the work area and hides after 1.5 s.
  - Window setup:
    - Created with winit `with_active(false)` and `with_skip_taskbar(true)`.
    - Once the HWND exists: `set_cursor_hittest(false)` for click-through, plus WS_EX_TOOLWINDOW, WS_EX_NOACTIVATE and LWA alpha 255.
    - DWM rounded corners and HWND_TOPMOST.
- **Close strategies:**
  - `hide` keeps the main window component (hidden) and keeps the OSD.
  - `drop` drops the main window component on close (recreated on the next open) and drops the OSD after it auto-hides.

## Results

Task Manager MB is `WorkingSetPrivate`, the same column as `docs/research/tray-memory`.

| State | Slint `hide`: Task Manager / private / WS MB | Slint `drop`: Task Manager / private / WS MB | WPF today (tray-memory doc) |
|---|---|---|---|
| Tray only, never shown (+5 s and +35 s) | **2.18** / 2.87 / 19.4 | **2.18** / 2.86 / 19.4 | 15 |
| Window open | 5.83 / 6.57 / 25.8 | 5.81 / 6.54 / 25.8 | – |
| Closed to tray, +5 s and +10 s | **5.83** / 6.57 / 25.8 (no drop) | **3.05** / 3.82 / 23.1 | 55–65 (15 with restart-on-close) |
| After the hotkey test, OSD hidden, +5 s and +10 s | 5.84 / 6.52 / 25.9 | 3.09 / 3.82 / 23.3 | – |
| GDI / USER objects: tray → open → closed → after OSD | 12/12 → 22/21 → 22/21 → 26/24 | 12/12 → 22/21 → 15/17 → 15/17 | – |
| Handles; threads | about 232–239; 10 → 6 | about 232–241; 10 → 6 | – |

**Idle CPU** (exact cycle counts from `QueryProcessCycleTime`; `TotalProcessorTime` only moves in 15.6 ms steps):

| Window | `hide` | `drop` |
|---|---|---|
| 30 s, tray only | 0.19 Mcycles (about 0.1 ms) | 0.22 Mcycles (about 0.1 ms) |
| +5 s to +10 s after close | 0 | 0 |
| +5 s to +10 s after the hotkey test | 0 | 0 |

**Hotkey latency:** 10 presses about 2 s apart, measured from the `QueryPerformanceCounter` tick just before `SendInput` until the OSD is shown (EnumWindows plus the layered-alpha check, polled every 1 ms).

| | `hide` (OSD kept) | `drop` (OSD recreated each time) |
|---|---|---|
| First press (creates the OSD window) | 34.8 ms | 25.7 ms |
| Other presses: median / p95 | **4.7 / 8.1 ms** | 21.7 / 38.0 ms (every press creates a window) |
| App side: SendInput → hotkey received | 1.2–3.0 ms | 1.4–2.7 ms |
| App side: hotkey → OSD show call | 1.2–2.4 ms | 0.7–2.2 ms |
| Focus stolen | 0 of 10 | 0 of 10 |

**Burst:** 40 × Ctrl+Alt+Up at 30 ms intervals.

| | `hide` | `drop` |
|---|---|---|
| Hotkey events received | 40 / 40 | 40 / 40 |
| Values | 5, 10, …, 100, then 100 × 20 | same |
| Final value | 100 (correct) | 100 (correct) |
| SendInput → hotkey: median / max | 1.8 / 4.7 ms | 2.1 / 19.3 ms |
| CPU for the burst + 300 ms | 151 Mcycles (TotalProcessorTime: 156 ms) | 186 Mcycles (31 ms) |
| Peak private / WS | 6.61 / 26.0 MB | 4.01 / 23.4 MB |

## Answers

1. **Does memory drop in the background?**
   - **With `drop`: yes.** Closing the window frees about 2.8 MB (Task Manager 5.8 → 3.05 MB; GDI 22 → 15). It doesn't fall back to the 2.2 MB tray-only level: about 0.9 MB stays (font and glyph caches, allocator slack).
   - **With `hide`: no.** It stays at 5.8 MB, because the hidden window keeps its component tree and its window-sized softbuffer DIB.
   - **Either way it's tiny** next to WPF's 15 MB tray-only and 55–65 MB after use.
2. **Idle CPU:** effectively zero.
   - Tray-only: 0.2 Mcycles in 30 s (a single wakeup's worth).
   - After close and after OSD use: 0 cycles in the sampled windows.
   - There's no timer; the loop sleeps until a hotkey, tray or window message arrives.
3. **Hotkey to OSD:**
   - Kept OSD: about 4–8 ms (median 4.7 ms).
   - Recreated OSD: about 20–38 ms.
   - First press: 26–35 ms.
   - Delivery is reliable: a 30 ms auto-repeat burst lost nothing and ended at the correct value.

## What Slint and winit keep alive

- **After hide:**
  - The Slint component tree.
  - The winit window (HWND).
  - The softbuffer surface: a window-sized DIB, about 2.9 MB at 1008×720.
  - The renderer's partial-render state.
  - Hiding is just `set_visible(false)`; nothing is suspended.
- **After drop:** window, surface and component are freed. What stays for the life of the process:
  - The winit event loop and its hidden "Winit Thread Event Target" window.
  - The tray icon.
  - global-hotkey's message window.
  - The software renderer's glyph cache (an LRU of up to about 1 MiB) and the system font collection.
  - Embedded font and image data.
- **Threads:** 10 at start, 6 after a while.
  - global-hotkey spawns a short thread per press. It polls `GetAsyncKeyState` every 50 ms until the key is released, so that's wakeups only while a key is held.

## Problems found, and their fixes in the prototype

- **A redraw spin from touching a hidden window.** Two things caused it:
  - Setting a property on the hidden main window (the brightness sync).
  - Hiding the OSD with Win32 `SW_HIDE` behind Slint's back.

  Either one made the main thread use about 8–15 ms of CPU per second indefinitely, until the window was shown again. The redraw it requested never completed.
  **Fix:** only push values into a visible window (set them on open), and never hide a Slint window behind Slint's back.
- **winit and a no-focus OSD don't mix.** After the first show, winit's `set_visible(true)` uses SW_SHOW, which activates the window. Every visibility change also rewrites GWL_EXSTYLE, dropping WS_EX_TOOLWINDOW and WS_EX_NOACTIVATE.
  **Fix (keep strategy):** the OSD stays "visible" and toggles layered alpha (0 / 255). The drop strategy gets a fresh `with_active(false)` window each time, so its first show is SW_SHOWNOACTIVATE.
- **Lazy window creation.** Slint creates the HWND on the next loop iteration, not inside `show()`. So the OSD styling (click-through, tool window) lands about 150 ms after the first show. For that moment the brand-new OSD isn't click-through, and could briefly be listed in Alt+Tab. It never has a taskbar button: `skip_taskbar` is set at creation.
- **Possible stale frame.** In keep mode the OSD is made opaque before Slint repaints the new value, so the old value may show for one frame (a few ms).
- **Not tested:**
  - Tray icon after an Explorer restart (`TaskbarCreated`).
  - Mixed-DPI placement: the OSD position assumes 100 % scale.
  - Transparency: the software renderer can't draw a per-pixel-transparent window on Windows, so the window is the card and DWM rounds the corners.
- **Earlier, longer run (superseded binary).**
  - One `hide` burst received only 5 of 40 presses. That run had no SendInput diagnostics. The rerun recorded SendInput 6/6 events for every press and got 40/40, so the earlier loss was not reproduced. It was probably an input-injection or foreground-focus artefact.
  - That run also had one window re-open that cost 670 ms of CPU, which didn't reproduce later.
  - The earlier run's memory numbers matched this one (tray 2.0–2.1 MB; open 5.6 MB; drop-after-close 2.9 MB).

## Caveats

- **Short run:** 1 cycle and short waits. Memory could still settle over minutes; the earlier 40 s waits showed no change.
- **Synthetic input:** keys were injected with SendInput while the user's own session was active. Ctrl+Alt modifier presses reach whatever app is in the foreground.
- **Burst CPU:** TotalProcessorTime and cycle counts disagree on burst CPU (31–156 ms against 151–186 Mcycles). The cycle counts are the reliable ones; at roughly 2–4 GHz they correspond to about 40–90 ms.
