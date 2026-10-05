# Minimising the resource footprint of a WPF .NET 10 tray utility (ThinkVision Control)

Scope: RAM, CPU wake-ups, battery, startup time and disk size for a single-file, self-contained WPF .NET 10 tray app on Windows 11. Researched 2026-10-05.

**About the local measurements.** Several findings below come from a probe app I built and ran on this machine: Windows 11 Home 26200, .NET SDK 10.0.401, Microsoft.WindowsDesktop.App 10.0.12. The probe uses a WPF `Application` with a WinForms `NotifyIcon` and a small window (4 sliders, a combobox, a checkbox and a button), the same shape as ThinkVision Control's UI. The source is in `resource_footprint_probe/` and the raw rows are in `resource_footprint_data.json`. In the JSON, rows with phase `startup_ms` store the milliseconds in the `ws_mb` column. Units:

- "WS" is `Process.WorkingSet64`.
- "Private" is `Process.PrivateMemorySize64`, which is the private commit (Private Bytes).
- "GC heap" is `GC.GetTotalMemory`.

Each scenario ran at least twice. The first run of a variant is sometimes "cold" and slower or larger, and I note that where it matters. The numbers come from one machine with one GPU, so treat them as relative rather than universal. They are cited as [Local measurement](resource_footprint_data.json).

---

## 1. Typical idle memory of a WPF .NET 8/9/10 app (hidden window, tray icon), and what drives it

### Takeaway
A WPF process that has **never shown a window** idles at about **12 MB private / 43 MB working set**. Showing a window **once** raises that to about **41–45 MB private / 90 MB WS**. Hiding the window, or closing it and running the GC, gives back only about 4 MB. The cost is native: the hardware render path (D3D device, render thread, glyph caches). The managed heap is not the cause (GC heap is only about 1.5–2.5 MB). Software rendering cuts about 13 MB private and 7 threads.

### Cited Findings
- **Measured on .NET 10.0.12**, framework-dependent Release build, warm run ([Local measurement](resource_footprint_data.json)):

  | Scenario | WS MB | Private MB | GC heap MB | Threads |
  |---|---|---|---|---|
  | WPF `Application` + NotifyIcon, no window ever created | 43.4 | 12.2 | 0.9 | 12 |
  | Same, no NotifyIcon | 39.2 | 11.5 | 0.9 | 12 |
  | Window shown (hardware rendering) | 91.8 | 44.8 | 2.6 | 25 |
  | Window hidden (`Hide()` + 2×GC) | 94.1 | 45.7 | 1.7 | 27 |
  | Window closed (`Close()`, references dropped, 2×GC) | 89.7 | 41.1 | 1.5 | 27 |
  | Window shown with `RenderMode.SoftwareOnly` | 82.8 | 32.1 | 2.6 | 18 |
  | Closed with `SoftwareOnly` | 80.8 | 28.2 | 1.5 | 20 |

- The WinForms `NotifyIcon` alone costs about **4 MB WS and under 1 MB private** (43.4 vs 39.2 MB WS) ([Local measurement](resource_footprint_data.json)).
- In a dotnet/wpf discussion, a blank WPF app on .NET 8 was reported at **"more than 90MB"**, against **6–7 MB** for an equivalent WinForms app. With `RenderOptions.ProcessRenderMode = RenderMode.SoftwareOnly;` the same app dropped to **about 20 MB**. A WPF contributor (lindexi) attributed the gap to "the memory used by the hardware rendering portion" and warned that Task Manager numbers mislead, recommending VMMap — [dotnet/wpf Discussion #9017](https://github.com/dotnet/wpf/discussions/9017).
- Graphics drivers can inflate this further. Reports exist of an empty WPF app taking **165 MB** at startup on some Intel iGPU drivers, and of an app that normally uses about 40 MB using over 300 MB after launch. Memory was normal with software rendering or with the Intel GPU disabled — [dotnet/wpf #8306](https://github.com/dotnet/wpf/issues/8306); [Intel community thread](https://community.intel.com/t5/Graphics/Feedback-on-Memory-Usage-Issue-with-Intel-Graphics-Driver-on/td-p/1667680).
- Microsoft documents that Windows 11 may ignore timer-resolution requests from a window-owning process that is "fully occluded, minimized, or otherwise non-visible" — [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation).

### Inferences
- About 30 MB of private memory, plus 13 threads, is a one-way cost paid the first time any WPF window renders. In-process, it is never returned. For a background-mode app the biggest single lever is therefore: **do not create or show the main window at login**. Start with only the tray icon and hotkey HWND, and build the window on first use.
- Because closing the window frees only about 4 MB, "close instead of hide" matters much less than "never show". It is still worth doing, because it releases the visual tree and the bindings and stops any UI timers or animations.
- To get back to about 12 MB after the user has opened the UI, the app would have to restart itself after being hidden for a long time. Nothing found documents or recommends this, and it adds complexity. It is listed as an option, not a recommendation.
- `SoftwareOnly` is a credible option for a window this simple (sliders and text). It saves about 13 MB private and 7 threads, and it protects against the driver-specific blowups above. Drawing then moves to the CPU, which is negligible for a static 420×420 panel.

### Gaps
- I found no measured idle numbers for WPF specifically on .NET 9 or 10 in blogs or issues. The .NET 10 numbers here are my own.
- Not verified: whether Mica or acrylic backdrops (via `ThemeMode`/DWM) still render correctly under `RenderMode.SoftwareOnly`. Test before adopting both together.
- Not measured: the separate cost of `SystemEvents` subscriptions and the `RegisterHotKey` HwndSource. Both are small native objects and are expected to cost under 1 MB.

---

## 2. GC and JIT settings for small desktop apps

### Takeaway
For a process whose GC heap is about 2 MB, **the GC knobs make no measurable difference**. Concurrent GC off, `GCConserveMemory=7`, a 1 MB gen0 budget and `TieredPGO=0` all measured within noise of the defaults. Keep workstation GC, which is the default. Do not use server GC. ReadyToRun was the only JIT/startup setting with a visible effect (see Q6).

### Cited Findings
- The default is workstation GC (`System.GC.Server=false`). Background (concurrent) GC is on by default (`System.GC.Concurrent=true`). Setting `false` selects non-concurrent GC. The MSBuild property is `ConcurrentGarbageCollection` — [GC config settings](https://learn.microsoft.com/en-us/dotnet/core/runtime-config/garbage-collector).
- `System.GC.ConserveMemory` takes values 0–9: "the higher the value, the more the garbage collector tries to conserve memory", at the cost of more frequent GCs. Microsoft suggests starting at 5–7. There is no MSBuild property, so set it with `RuntimeHostConfigurationOption` — [GC config settings](https://learn.microsoft.com/en-us/dotnet/core/runtime-config/garbage-collector).
- `HeapHardLimit` is the maximum commit for the GC heap and its bookkeeping. Without a container limit it is unset by default — [GC config settings](https://learn.microsoft.com/en-us/dotnet/core/runtime-config/garbage-collector).
- Since .NET 7 the GC uses **regions** (4 MB SOH regions by default). Microsoft notes: "For processes that have very small GC heaps, changing the region size to be smaller is beneficial for native memory usage from GC's own bookkeeping. The recommendation is 1 MB." In .NET 10 this is configurable as `System.GC.RegionSize` in runtimeconfig — [GC config settings](https://learn.microsoft.com/en-us/dotnet/core/runtime-config/garbage-collector).
- DATAS (dynamic adaptation) is "Enabled by default starting in .NET 9", but it is listed under Server GC settings, so it does not apply to workstation GC — [GC config settings](https://learn.microsoft.com/en-us/dotnet/core/runtime-config/garbage-collector).
- **Measured**, warm run, window closed ([Local measurement](resource_footprint_data.json)):

  | Setting | WS MB | Private MB | Startup ms |
  |---|---|---|---|
  | Defaults | 89.7 | 41.1 | 613 |
  | `DOTNET_gcConcurrent=0` | 89.6–89.8 | 40.8–41.1 | 601–602 |
  | `DOTNET_GCConserveMemory=7` | 89.6–89.7 | 40.4–40.7 | 596–626 |
  | `DOTNET_GCgen0size=0x100000` (1 MB) | 89.7–92.0 | 36.7–41.1 | 618–679 |
  | `DOTNET_TieredPGO=0` | 89.4–89.8 | 40.9–41.2 | 594–598 |

  The thread count stayed at 27 even with concurrent GC off. The background GC thread appears to be created lazily, and a 2 MB heap never triggers a background GC.

### Inferences
- The GC heap is under 5% of the private bytes, so GC tuning cannot move the needle. Of these settings, only `System.GC.Concurrent=false` and `System.GC.RegionSize=1048576` have a plausible theoretical benefit for a tiny heap: no BGC thread, and less bookkeeping. Both are harmless for a tray app, but expect gains of a few hundred KB at most.
- Do **not** set `HeapHardLimit`. It adds out-of-memory risk with no benefit at this heap size.
- `TieredCompilation`/`TieredPGO` changes are not worth it. There is no measurable memory change, and turning tiering off would increase startup JIT cost.

### Gaps
- `System.GC.RegionSize=1MB` was not measured separately. It is new as a runtimeconfig setting in .NET 10.
- I found no Microsoft guidance aimed specifically at desktop or tray apps for GC configuration.

---

## 3. Releasing UI when hidden, and trimming the working set

### Takeaway
Close (destroy) the window rather than hiding it, and drop every reference so the visual tree can be collected. That saves about 4 MB and stops UI work, but the WPF render infrastructure stays. Calling `EmptyWorkingSet`/`SetProcessWorkingSetSize(-1,-1)` makes Task Manager show about 7 MB, but **private commit is unchanged** and pages fault back in on next use. It is cosmetic and is discouraged.

### Cited Findings
- **Measured**: `Hide()` gave 94.1 MB WS / 45.7 MB private. `Close()` plus GC gave 89.7 / 41.1. `EmptyWorkingSet` afterwards gave **7.1 MB WS / 40.2 MB private**. In the "never shown" state it gave 8.8 WS / 12.3 private ([Local measurement](resource_footprint_data.json)).
- `SetProcessWorkingSetSizeEx` documents that passing `(SIZE_T)-1` for both the minimum and maximum "removes as many pages as possible from the working set of the specified process" — [SetProcessWorkingSetSizeEx](https://learn.microsoft.com/en-us/windows/win32/api/memoryapi/nf-memoryapi-setprocessworkingsetsizeex).
- Practitioner guidance: "you hardly ever have to trim working sets ... Windows has way better heuristics to do that automatically for you, skipping pages you frequently use". Trimming too often causes hard page faults — [The Wiert Corner](https://wiert.me/2021/07/07/setprocessworkingsetsize-you-hardly-if-ever-need-to-call-this-from-your-process/), as summarised by search. I could not retrieve the full post text.
- The Microsoft-endorsed alternative is to lower memory priority: `SetProcessInformation(ProcessMemoryPriority)`. The docs say "When the memory manager trims the working set, it trims lower priority pages before higher priority pages", and recommend it for background work — [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation).
- dotnet/wpf maintainers note that Task Manager memory figures are not a reliable measure, and recommend VMMap — [dotnet/wpf Discussion #9017](https://github.com/dotnet/wpf/discussions/9017).

### Inferences
- Recommended pattern for background mode: close the window on hide, set `MainWindow = null`, unsubscribe the view model from the services (DDC queue, SystemEvents), and recreate the window on demand. This keeps the "closed" state clean and prevents leaks through event handlers on long-lived services.
- If the low Task Manager number matters for perception (users compare it with Monitorian or Twinkle Tray), use `ProcessMemoryPriority = MEMORY_PRIORITY_LOW` while hidden rather than `EmptyWorkingSet`. The OS then trims the app first under pressure, with none of the fault storms.

### Gaps
- I could not fetch a primary Raymond Chen or Microsoft blog post on `EmptyWorkingSet` for an authoritative quote.

---

## 4. Windows 11 Efficiency mode / EcoQoS and priority

### Takeaway
Use `SetProcessInformation(ProcessPowerThrottling, EXECUTION_SPEED)` to opt into EcoQoS **while the window is hidden**, and clear it (`ControlMask=EXECUTION_SPEED, StateMask=0`) when the window opens. Optionally add `IGNORE_TIMER_RESOLUTION`. Windows already gives a hidden, window-owning app Low QoS on battery. Explicit EcoQoS goes further: efficient frequency and E-cores even on AC.

### Cited Findings
- With `PROCESS_POWER_THROTTLING_EXECUTION_SPEED` on, the process "will be classified as EcoQoS. The system will try to increase power efficiency through strategies such as reducing CPU frequency or using more power efficient cores ... EcoQoS should not be used for performance critical or foreground user experiences." Without an explicit opt-in, the system infers QoS heuristically — [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation).
- With `PROCESS_POWER_THROTTLING_IGNORE_TIMER_RESOLUTION`: "any current timer resolution requests made by the process will be ignored ... which can improve power efficiency". On Windows 11, occluded or minimized processes may have this applied automatically — [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation).
- The docs include code to turn EcoQoS on, to turn it off (HighQoS), and to reset to system-managed (`ControlMask=0, StateMask=0`). The minimum OS is Windows 8; the EcoQoS level itself exists from Windows 11 ("Prior to Windows 11 ... labeled as LowQoS") — [SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation).
- QoS table — [Quality of Service](https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service):
  - Window "Minimized, or Fully Occluded" maps to **Low**: "On battery, selects most efficient CPU frequency and schedules to efficient core."
  - **Eco** "Always selects most efficient CPU frequency and schedules to efficient cores" (Windows 11).
  - Thread priority also feeds the heuristics: "threads running with reduced thread priority can imply a lower QoS level".
- Microsoft's EcoQoS announcement: "up to a 90% reduction in CPU power consumption, while consuming less than half the CPU energy to complete the same work". Recommended for "background services, updaters, sync engines, indexing services" — [Introducing EcoQoS (Microsoft devblog)](https://devblogs.microsoft.com/performance-diagnostics/introducing-ecoqos/).
- Task Manager's "Efficiency mode" combines EcoQoS with lowered process priority — [TechRepublic](https://www.techrepublic.com/article/ecoqos-gives-windows-11-apps-better-battery-life/); [Windows Latest](https://windowslatest.com/2022/02/22/interesting-details-of-windows-11s-performance-feature-efficiency-mode-revealed). Both are secondary sources, read as search summaries.

### Inferences
- A hotkey press costs microseconds of CPU, so EcoQoS adds no perceptible latency to hotkey or DDC handling. DDC/CI latency is dominated by I²C timing (tens of ms per VCP call), not CPU speed. This makes EcoQoS while hidden low-risk. Turn it off when the window opens so slider dragging and animations get full speed.
- `Process.PriorityClass = BelowNormal` (or Idle) while hidden mirrors what Task Manager's Efficiency mode does. Keep Normal or BelowNormal rather than Idle, so a hotkey-triggered DDC write is not starved under load.
- Task Manager shows the leaf icon only for processes in Efficiency mode. Opting in programmatically may make the app look "eco" to users, which is a small trust signal. This is not verified.

### Gaps
- I found no measured battery numbers for EcoQoS on an idle tray app. The 90% figure is for CPU-bound work.
- Not confirmed whether `SetProcessInformation` EcoQoS alone makes Task Manager show the Efficiency-mode leaf, or whether a priority change is also needed.

---

## 5. Avoiding wake-ups: event-driven detection and timer coalescing

### Takeaway
The 5-second `System.Threading.Timer` reconnect loop is the main CPU, battery and I²C wake-up source when the monitor is absent. Replace it with:

1. `RegisterDeviceNotification(GUID_DEVINTERFACE_MONITOR)` on the existing message-only HwndSource, plus `DisplaySettingsChanged` and `PowerModeChanged(Resume)`, as triggers;
2. a short bounded retry burst after each trigger, because DDC is often not ready right at arrival;
3. if a fallback poll is still needed, exponential backoff, coalescable timers, and no polling at all while suspended or (optionally) while the UI is hidden.

### Cited Findings
- `GUID_DEVINTERFACE_MONITOR` = `{E6F07B5F-EE97-4a90-B076-33F57BF4EAA7}`. "Windows registers a device interface for each monitor that is configured in the operating system" — [GUID_DEVINTERFACE_MONITOR](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/guid-devinterface-monitor).
- Apps register with `RegisterDeviceNotification` using a `DEV_BROADCAST_DEVICEINTERFACE` filter (`DBT_DEVTYP_DEVICEINTERFACE` plus the class GUID) and handle `WM_DEVICECHANGE` / `DBT_DEVICEARRIVAL`. All windows get only basic `WM_DEVICECHANGE` without registering. `CM_Register_Notification` is the newer callback-based alternative that needs no window — [CM_Register_Notification](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification); usage pattern in [Chromium system_message_window_win.cc](https://chromium.googlesource.com/chromium/+/HEAD/content/browser/system_message_window_win.cc).
- `SetCoalescableTimer(hWnd, id, uElapse, proc, uToleranceDelay)`: tolerance `0x1–0x7FFFFFF5` ms. "Applications should set this value to the system default (TIMERV_DEFAULT_COALESCING) or the largest value possible." `TIMERV_NO_COALESCING` should be avoided "unless you are certain that the timer requires no coalescing" — [SetCoalescableTimer](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcoalescabletimer).
- Twinkle Tray's maintainer says the app "should have 0% CPU usage while idle", and that tightening Windows callbacks (for example theme-change reactions) fixed excess CPU and RAM — [twinkle-tray #879](https://github.com/xanderfrangos/twinkle-tray/issues/879).

### Inferences
- The app already owns a message-only HwndSource for hotkeys. Adding `RegisterDeviceNotification` on that HWND costs nothing extra.
- Message-only windows do not receive broadcast messages, but `RegisterDeviceNotification` with a window handle sends directed notifications. Verify this on a message-only HWND; if in doubt, use `CM_Register_Notification`, which needs no window at all.
- A suggested backoff: after a trigger, retry at 0.5 s, 1 s, 2 s, 4 s, 8 s, then stop. With no trigger, poll at most every 60 s, or not at all. This cuts idle wake-ups from 720 per hour to about 60 per hour or fewer.
- `System.Threading.Timer` has no tolerance parameter. If a periodic fallback stays, `SetCoalescableTimer` on the message-only HWND with a tolerance of several seconds lets Windows batch the wake-up with others. This is inference: no measurement of coalescing benefit was found.
- Stop all timers on `PowerModeChanged(Suspend)` and re-probe once on `Resume`.
- `UserPreferenceChanged` fires often (theme, accent, many settings). Only react to the categories that matter (`General`/`Color`/`VisualStyle`), and do no DDC work in that handler.

### Gaps
- No public data on how quickly DDC/CI becomes usable after `DBT_DEVICEARRIVAL` for Lenovo ThinkVision panels. The retry burst parameters need empirical tuning.
- Not confirmed whether `DisplaySettingsChanged` fires on a monitor power-on that keeps the same topology. That is likely the case where the monitor comes back from standby, and there it may not fire.

---

## 6. Publish options: size, memory and startup

### Takeaway
For self-contained single-file, add **`SatelliteResourceLanguages=en`** (−18 MB, no cost). **Do not enable `EnableCompressionInSingleFile`**: it shrinks the exe from 155 MB to 69 MB but adds **about 73 MB private / 124 MB WS** for the whole process lifetime, plus about 100 ms of startup. **ReadyToRun** cut startup by about 100–140 ms in this test, for about +17 MB on disk and +3–4 MB private. Trimming is blocked by the SDK for WPF and WinForms. NativeAOT is not an option. `InvariantGlobalization` saved no disk on Windows.

### Cited Findings
- **Measured exe sizes**, .NET 10.0.12, win-x64 ([Local measurement](resource_footprint_data.json)):

  | Variant | Exe size |
  |---|---|
  | Framework-dependent single-file | 0.18 MB (runtime installed separately; the shared WindowsDesktop + NETCore folders are 96 + 77 MB on disk) |
  | Self-contained single-file, `IncludeNativeLibrariesForSelfExtract` | 172.9 MB |
  | + `SatelliteResourceLanguages=en` | 154.7 MB |
  | + `InvariantGlobalization=true` | 154.7 MB (no change) |
  | + `EnableCompressionInSingleFile` | 69.3 MB |
  | + `PublishReadyToRun` (no compression) | 172.3 MB |
  | + `PublishReadyToRun` + compression | 74.5 MB |
  | `PublishTrimmed=true` | build error NETSDK1175 ("Windows Forms is not supported or recommended with trimming enabled") |

- **Measured runtime**, window shown then closed, 6 runs each ([Local measurement](resource_footprint_data.json)):

  | Variant | Startup to ContentRendered (ms) | Closed WS MB | Closed private MB |
  |---|---|---|---|
  | FD single-file | 548–658 | 88–92 | 38–41 |
  | SC single-file + sat=en | 557–729 | 90.5–93 | 41.7–42.9 |
  | SC + compression | 701–769 | **214** | **115** |
  | SC + R2R | **467–491** (one outlier at 630) | 95.4–96 | 45.2–45.7 |
  | SC + R2R + compression | 752–819 | 184–206 | 128–146 |

- Microsoft: in single-file apps "the managed DLLs are extracted and loaded in memory, avoiding the extraction to a folder". With compression, "On application start, the assemblies must be decompressed into memory, which takes some time. We recommend that you measure both the size change and startup cost" — [Single-file deployment](https://learn.microsoft.com/en-us/dotnet/core/deploying/single-file/overview).
- `IncludeAllContentForSelfExtract` (extract everything to `%TEMP%\.net`) is "not recommended: it's a .NET Core 3.1 compatibility mode" — [Single-file deployment](https://learn.microsoft.com/en-us/dotnet/core/deploying/single-file/overview).
- WPF: "almost no WPF apps are runnable after trimming, so trimming support for WPF is currently disabled in the .NET SDK". The same applies to Windows Forms, because of built-in COM marshalling — [Known trimming incompatibilities](https://learn.microsoft.com/en-us/dotnet/core/deploying/trimming/incompatibilities). The tracking issue is still open with milestone "Future" — [dotnet/wpf #3811](https://github.com/dotnet/wpf/issues/3811).
- Requests to reduce WPF single-file size are long-standing and open — [dotnet/wpf #3070](https://github.com/dotnet/wpf/issues/3070), [#7447](https://github.com/dotnet/wpf/issues/7447).
- Sizes of the main WPF binaries in the 10.0.12 shared framework, measured from the local install: PresentationFramework.dll 15.8 MB, PresentationCore.dll 8.3 MB, System.Windows.Forms.dll 13.7 MB, D3DCompiler_47_cor3.dll 4.7 MB, wpfgfx_cor3.dll 2.0 MB, PresentationNative_cor3.dll 1.2 MB, PresentationFramework.Fluent.dll 0.98 MB, PresentationFramework.Aero2.dll 0.44 MB ([Local measurement](resource_footprint_data.json)).

### Inferences
- Without compression, bundled assemblies are mapped from the exe image. That counts as shareable, file-backed memory, not private commit. Compression turns every loaded assembly into private heap memory. Hence the +73 MB, which is larger than the entire rest of the app. For an app that stays resident, compression is the single worst setting.
- The R2R size jump (+17.6 MB) and startup win suggest some assemblies in the WindowsDesktop runtime pack are not fully precompiled, or get recompiled. This is unverified. Startup matters little for a login-time tray app, so R2R is optional. Choose it if "click tray icon → window" latency on first open matters. Skip it if disk size matters more.
- **Framework-dependent** reduces download size from 155 MB to under 1 MB, with no memory penalty (measured). The cost is a dependency on the .NET 10 Desktop Runtime being installed, and if it is missing the user sees a prompt. Self-contained avoids that dependency.
- `UseWindowsForms` is pulled in only for `NotifyIcon`. Replacing it with a P/Invoke `Shell_NotifyIcon` wrapper (or H.NotifyIcon) would drop System.Windows.Forms.dll (13.7 MB) and its dependencies from the self-contained bundle. This is inference: the exact saving was not measured, and System.Drawing.Common also goes away. It is also a prerequisite if WPF trimming ever becomes supported.
- `InvariantGlobalization` saves nothing on disk on Windows, because .NET uses the OS ICU there. It is not worth the behaviour change.

### Gaps
- NativeAOT for WPF: no primary statement was retrieved in this session. It follows from the trimming block, since NativeAOT requires a trimmable app, but cite [dotnet/wpf #3811](https://github.com/dotnet/wpf/issues/3811) / [#4762 ComWrappers](https://github.com/dotnet/wpf/issues/4762) as the blockers.
- The R2R size-increase mechanism was not confirmed.
- The size saving from dropping WinForms was not measured.

---

## 7. Cost of WPF-UI versus the built-in Fluent theme

### Takeaway
The built-in Fluent theme (`ThemeMode`, in PresentationFramework.Fluent.dll, **0.98 MB**, already part of the shared framework) cost about **+1 MB GC heap, +3 MB WS, about 0 private and +30 ms startup**. **WPF-UI 4.3.0** (Wpf.Ui.dll **6.57 MB**) cost about **+7 MB private, +13 MB WS, +4 MB GC heap and +160 ms warm startup**, and about +700 ms cold.

### Cited Findings
- **Measured**: the WPF-UI 4.3.0 nupkg is 17.2 MB, and `lib/net10.0-windows7.0/Wpf.Ui.dll` is **6,565,888 bytes**. PresentationFramework.Fluent.dll in the 10.0.12 shared framework is **980,776 bytes** ([Local measurement](resource_footprint_data.json)).
- **Measured runtime**, window shown then closed. WPF-UI used `ThemesDictionary` + `ControlsDictionary` merged at app level ([Local measurement](resource_footprint_data.json)):

  | Theme | Warm startup ms | Shown WS / private MB | Closed WS / private MB | GC heap MB | GC committed MB |
  |---|---|---|---|---|---|
  | Aero2 default | 613 | 91.8 / 44.8 | 89.7 / 41.1 | 1.5 | 2.2 |
  | Fluent (`ThemeMode.System`) | 640–682 | 94.3–96.2 / 41.8–46.7 | 92.1–93.9 / 37.5–38.4 | 2.5 | 3.2 |
  | WPF-UI 4.3.0 | 775 (cold 2125, vs default cold 1418) | 106.4 / 53.6 | 102.7 / 48.1 | 5.7 | 8.3 |

  Before any window was created, WPF-UI already added 16 MB WS and 6 MB private (59 WS / 18 private vs 43 / 12), because the dictionaries load at startup.
- Fluent in .NET 9+ is applied via `Application.ThemeMode` or the pack URI `/PresentationFramework.Fluent;component/Themes/Fluent.xaml` — [What's new in WPF .NET 9](https://learn.microsoft.com/dotnet/desktop/wpf/whats-new/net90).
- WPF-UI has had leak reports in long-lived scenarios: [FluentWindow leak #1648](https://github.com/lepoco/wpfui/issues/1648), [ContentDialog leak #1618](https://github.com/lepoco/wpfui/issues/1618), [ListView leak #1242](https://github.com/lepoco/wpfui/issues/1242), [NavigationView page disposal #613](https://github.com/lepoco/wpfui/issues/613). All are closed.

### Inferences
- For a resident tray app, the Fluent theme's cost is within noise. WPF-UI's cost (+7 MB private, +6.5 MB on disk, slower first open) is real but modest. Its main downside is that the dictionaries are merged at `Application` level, so the cost is paid at login even if the window is never opened.
- Mitigation if WPF-UI is adopted: merge its dictionaries into the **window's** resources (or lazily on first window creation) instead of App.xaml, so background-mode idle stays at about 12 MB private. This is inference: it was not measured, and some WPF-UI controls may expect app-level resources.

### Gaps
- No third-party measurements of either theme's resource-dictionary load cost were found. All numbers are local.
- Not measured: the WPF-UI `FluentWindow` with Mica backdrop. The test used a plain `Window` with WPF-UI dictionaries only.

---

## 8. Startup-at-login best practices

### Takeaway
An HKCU `Run` entry, or a Startup-folder shortcut, is the simplest per-user option. Windows already delays these by about 10 s after sign-in to reduce boot contention. Combine it with "start hidden: tray and hotkeys only, no window" (see Q1) and defer non-essential work. That gives a login footprint of about 12 MB private.

### Cited Findings
- Windows runs Startup-folder items "after about a 10 second delay" at sign-in, to improve startup performance. The delay is controlled by `HKCU\Software\Microsoft\Windows\CurrentVersion\Explorer\Serialize\StartupDelayInMSec` and `WaitForIdleState` — [NinjaOne](https://www.ninjaone.com/blog/enable-or-disable-delay-of-running-startup-apps-in-windows-11/); [ElevenForum](https://www.elevenforum.com/t/enable-or-disable-delay-of-running-startup-apps-in-windows-11.9144/). Both are secondary sources.
- Monitorian (WPF, .NET Framework 4.8) keeps startup registration in a dedicated `StartupAgency` project — [Monitorian repo code search](https://github.com/emoacht/Monitorian).

### Inferences
- Avoid Task Scheduler "at logon" tasks with zero delay. They bypass the Explorer startup delay and so compete with the shell. If Task Scheduler is used (for example to set a custom delay), add a 30–60 s delay. The `Run` key plus the built-in delay achieves almost the same with less machinery.
- Pass a `--background` (or `--tray`) argument from the Run entry, so login launches never construct the window even if the user's last state was "window open".

### Gaps
- No Microsoft primary documentation was retrieved on the Run key vs Task Scheduler vs `StartupTask` (MSIX) trade-offs, or on Windows 11 startup-impact scoring in Task Manager.

---

## 9. Reference points: Twinkle Tray, Monitorian, PowerToys

### Takeaway
Twinkle Tray (Electron, 4 processes) idles at about **40 MB per its author**, but users routinely report **80–215 MB**, and up to 600 MB in edge cases. Monitorian (WPF on .NET Framework 4.8) is the author-recommended lighter alternative, but I found no published numbers for it. ThinkVision Control's measured envelope is **12 MB private / 43 MB WS when never opened, and 41 MB private / 90 MB WS after opening**. That already beats typical Twinkle Tray reports.

### Cited Findings
- Twinkle Tray maintainer: "Idle I see it at ~40MB total and while in use it's ~70MB ... if you're concerned about the RAM utilization being in that 40-70MB range, I recommend Monitorian over Twinkle Tray." Users report "215mb", "~80-100MB" idle and "600MB" with 3 screens — [twinkle-tray #879](https://github.com/xanderfrangos/twinkle-tray/issues/879).
- Twinkle Tray runs 4 processes (Node main, network, GUI renderer, GPU) and "typically uses no more than 80MB of RAM ... and ~0% CPU when idle". A user identifies the GPU process as "the biggest elephant in the room" — [twinkle-tray #96](https://github.com/xanderfrangos/twinkle-tray/issues/96).
- Twinkle Tray v1.15.0-beta1 fixed "a bug that could cause excessive CPU/RAM usage at startup" — [release notes via newreleases.io](https://newreleases.io/project/github/xanderfrangos/twinkle-tray/release/v1.15.0-beta1).
- Monitorian targets .NET Framework 4.8 (`<TargetFrameworkVersion>v4.8</TargetFrameworkVersion>` in Source/Monitorian/Monitorian.csproj) — [emoacht/Monitorian](https://github.com/emoacht/Monitorian). Its download "doesn't register even a full megabyte" because it uses the in-box framework — [WindowsForum](https://windowsforum.com/windows-news.4/enhance-windows-11-control-monitor-brightness-with-clickmonitorddc-and-monitorian.342290/?amp=1) (secondary).

### Inferences
- Monitorian's small download comes from the framework-dependent design on in-box .NET Framework, not from WPF itself. Its runtime memory should be in the same WPF hardware-rendering band (roughly 30–60 MB private after the flyout opens). This is an estimate; it was not measured.
- To credibly claim "lighter than Twinkle Tray": the never-opened tray state (12 MB private) is far below it; the opened state (about 41 MB, or about 28 MB with SoftwareOnly) matches or beats Twinkle Tray's best case.

### Gaps
- **No measured idle numbers were found for Monitorian or PowerToys.** Measuring them locally with the probe method (Process WS and private bytes after open and close) would close this gap.

---

## 10. Prioritised checklist for ThinkVision Control (synthesis)

### Takeaway
Ranked by expected impact on a resident tray app. Measured impacts come from [Local measurement](resource_footprint_data.json) unless another source is cited.

### Cited Findings
1. **Never create the main window in background/login launches.** Measured: 12 MB vs about 41–45 MB private, and 12 vs 25+ threads (Q1).
2. **Do not enable `EnableCompressionInSingleFile`.** Measured: +73 MB private, +124 MB WS, +~100 ms startup (Q6; [Single-file docs](https://learn.microsoft.com/en-us/dotnet/core/deploying/single-file/overview)).
3. **Replace the 5 s reconnect poll with device-arrival, display-change and resume events, plus a bounded backoff.** This removes about 720 wake-ups/hour and the I²C bus traffic when the monitor is absent ([GUID_DEVINTERFACE_MONITOR](https://learn.microsoft.com/en-us/windows-hardware/drivers/install/guid-devinterface-monitor), [CM_Register_Notification](https://learn.microsoft.com/en-us/windows/win32/api/cfgmgr32/nf-cfgmgr32-cm_register_notification)). Any remaining timer should be coalescable ([SetCoalescableTimer](https://learn.microsoft.com/en-us/windows/win32/api/winuser/nf-winuser-setcoalescabletimer)) and stopped on suspend.
4. **Close (not hide) the window on dismiss, and drop references.** Measured: −4 MB private; stops UI work and prevents leaks (Q3).
5. **Consider `RenderMode.SoftwareOnly`** for this simple UI. Measured: −13 MB private, −9 MB WS, −7 threads, and protection from Intel-driver blowups ([dotnet/wpf #9017](https://github.com/dotnet/wpf/discussions/9017), [#8306](https://github.com/dotnet/wpf/issues/8306)). Verify backdrop and visual quality first.
6. **EcoQoS while hidden, HighQoS while shown**, via `SetProcessInformation(ProcessPowerThrottling)`. Optionally add `IGNORE_TIMER_RESOLUTION` and `MEMORY_PRIORITY_LOW` while hidden ([SetProcessInformation](https://learn.microsoft.com/en-us/windows/win32/api/processthreadsapi/nf-processthreadsapi-setprocessinformation), [QoS](https://learn.microsoft.com/en-us/windows/win32/procthread/quality-of-service)).
7. **Add `SatelliteResourceLanguages=en`.** Measured: −18 MB exe, no runtime cost (Q6).
8. **Prefer the built-in Fluent theme over WPF-UI.** Measured: Fluent costs about 0 private; WPF-UI costs +7 MB private and +160 ms. If WPF-UI is used, load its dictionaries lazily with the window (Q7).
9. **Optional: `PublishReadyToRun`.** Measured: −100–140 ms startup, +17 MB disk, +3–4 MB private (Q6).
10. **Optional: replace the WinForms `NotifyIcon` with a native `Shell_NotifyIcon` wrapper** to drop the System.Windows.Forms dependency. The disk saving is estimated at over 13.7 MB of dll and was not measured. Measured runtime saving: about 4 MB WS, under 1 MB private.
11. **Do nothing with GC knobs** (`gcConcurrent`, `ConserveMemory`, gen0size, PGO). Measured: no effect at a 2 MB heap (Q2). If anything, `System.GC.Concurrent=false` is harmless.
12. **Do not use `EmptyWorkingSet`/`SetProcessWorkingSetSize(-1,-1)`.** It only lowers the Task Manager number; private commit is unchanged (Q3).
13. **Use a Run key with a `--background` argument** and rely on the about-10 s Explorer startup delay (Q8).

### Inferences
- Items 1, 3 and 5 together target an idle footprint of about **12 MB private / about 40 MB WS / 0 wake-ups per minute** before the UI is opened, and **about 28 MB private** after it has been opened once (with SoftwareOnly). That would put ThinkVision Control well under Twinkle Tray's reported range, and probably on par with or below Monitorian.
- A "hidden for N hours → relaunch self in background mode" trick would recover the about 30 MB post-show overhead. It is undocumented and adds complexity, so park it unless users complain.

### Gaps
- All local numbers come from a single machine and GPU. Re-run `resource_footprint_probe` on an Intel-iGPU laptop to check the driver sensitivity cited above.
- There is no measured battery or wake-up data (for example from `powercfg /energy` or WPR CPU-usage traces) for the polling-vs-event change. It would be worth capturing before and after the refactor.
