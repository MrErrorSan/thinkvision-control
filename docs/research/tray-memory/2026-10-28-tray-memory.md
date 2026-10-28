# Why the tray app uses ~65 MB

Investigated 2026-10-28 after a field report: 67 MB in Task Manager while in the tray. Covers the **WPF app** (`src/ThinkVisionControl`), which is what the Windows installer ships. Raw numbers: [2026-10-28-tray-memory-data.json](2026-10-28-tray-memory-data.json). Script: [measure.ps1](measure.ps1). Machine: Ryzen 5 5500U, AMD iGPU + GTX 1650, .NET 10.0.14.

"Task Manager" below is the Memory column (active private working set). "Committed" is private bytes: what the process really holds, whether or not it currently sits in RAM.

## Answer

The app does not grow because of its own code or data. **Opening the window once costs about 45 MB, and WPF never gives it back.** Closing to the tray frees nothing.

| State | Task Manager | Committed |
|---|---|---|
| Started in tray, window never opened | **15 MB** | 20 MB |
| Window open | 46–65 MB | 72–91 MB |
| Window closed, back in tray (+40 s) | **55–65 MB** | 79–90 MB |

The 48–50 MB .NET runtime on disk is not the problem. Its DLLs are shared, read-only mappings: they appear in the total working set (85 MB even in the tray-only state) but not in Task Manager's number.

## Where the ~45 MB goes

Each variant was measured with one open/close cycle, after closing to the tray.

| Variant | Task Manager | Committed | What it shows |
|---|---|---|---|
| Defaults | 64 MB | 90 MB | — |
| Software rendering (`RenderMode.SoftwareOnly`) | 43 MB | 58 MB | GPU render path ≈ **20 MB** |
| Fluent theme off (`ThemeMode.None`) | 33 MB | 57 MB | Fluent theme ≈ **15–30 MB** |
| Both | 28 MB | 41 MB | floor for a WPF window in-process |
| Defaults + working-set trim | **5 MB** | 57–73 MB | cosmetic: memory still committed |

- **Managed .NET memory is small.** The GC heap is 3–4 MB after close (about 10 MB committed); JIT'd IL is 0.4 MB. GC tuning cannot fix this. The 2026-10-05 research reached the same conclusion.
- **The GPU driver loads into the process.** Opening the window loads 35 DLLs. The largest are the AMD user-mode driver (`atiumd6a`, `atiumd64`, `aticfx64`), `d3d9`, `wpfgfx_cor3` and `D3DCompiler_47`. Driver allocations stay for the life of the process. Even under `SoftwareOnly`, WPF still loads the AMD DLLs.
- **The Fluent theme (`ThemeMode="System"`) is the second cost.** Its resource dictionaries (they even pull in `System.Windows.Controls.Ribbon`) stay alive at application level after the window closes.
- **A working-set trim (`EmptyWorkingSet`) only changes the number Task Manager shows.** Committed memory is unchanged, the pages fault back in on next use, and the app already sets low memory priority while hidden, so Windows trims it first under pressure anyway.

## What can reach 5–10 MB

| Route | Tray memory after the window was used | Cost |
|---|---|---|
| Keep as is | 55–65 MB | — |
| Software rendering | ~43 MB | Mica under `SoftwareOnly` is unverified; CPU drawing |
| **Restart on close**: when the window closes in background mode, the app relaunches itself in tray-only mode and exits | **~15 MB** (committed ~20) | Hotkeys are unavailable for about 1 s after closing; the monitor is re-probed (capabilities are cached) |
| Working-set trim on close | 5 MB shown, ~60 MB committed | Cosmetic; the number is not real |
| Tray host without WPF (NativeAOT Win32 tray + hotkeys + DDC; window in its own process) | ~3–5 MB (probe `cs-win32-aot`: 3.3 MB private) | A second executable; Core must be AOT-safe. This is rewrite territory (see [../lightweight-ui-probes/](../lightweight-ui-probes/)) |

With WPF in the process, about 15 MB is the floor. A real 5–10 MB needs a tray process that never loads WPF.

## Side observation

In all 6 runs, a second launch (what a Start-menu click does) while the app was in the tray opened the window and brought it to the foreground. The "Start menu does nothing" report did not reproduce with the current build on this machine.
