# ThinkVision Control: live check of the 0.1.0 build

- Date: 2026-11-15, Windows 11 Pro 10.0.26200, Git Bash.
- Program under test: the bare release build `scratchpad\exe-0.1.0\thinkvision-control.exe` (10,896,896 bytes) and the console launcher `tvc.exe` (304,128 bytes), built from `release/0.1.0` at `b520e85`. It is not installed.
- Monitor: one Lenovo ThinkVision P32p-20 on DisplayPort, 3840×2160 at 125 % (3072×1728 logical).
- Compared against: [verify-live.md](verify-live.md) (the installed 0.0.8, 2026-11-14) and the [issue register](../2026-11-14-issues.md).
- Isolation:
  - Every 0.1.0 process ran with `APPDATA` pointed at a scratch copy of the user's data folder (`scratchpad\live-appdata\ThinkVisionControl`, copied from `appdata-backup-010`).
  - The user's installed 0.0.8 (PID 6708, bare command line, started 10:19:47) was ended for the test and started again afterwards.
  - The registry keys `HKCU\Software\Classes\thinkvision` and `HKCU\…\CurrentVersion\Run` were exported before the first run.
- Monitor writes: only 0x10 (brightness) and 0x12 (contrast), by `--step`. `--input` was run only without `--yes`; it was refused headless, and the on-screen confirm was never answered.
- Helper: `scratchpad\r10.sh <gui|tvc> args` runs `timeout ${T:-20} "<exe>" args` with the scratch `APPDATA`, then prints the output, the exit code and the elapsed time. Exit 124 means that `timeout` killed the program.
- Original monitor values, read first: **brightness 0, contrast 75**, DisplayPort, Neutral. That is the user's Day preset; the last test read contrast 20.

## 1. Commands run, outputs, exit codes

### 1.1 Help and version

```
$ tvc.exe --help                  full help (below)                       exit=0  490ms (first, cold)
$ tvc.exe --version               thinkvision-control 0.0.8               exit=0   94ms
$ tvc.exe -V                      unknown option '-V' + full help         exit=2   78ms
$ tvc.exe /?                      help                                    exit=0  121ms
$ tvc.exe --get all --help        help (help still wins)                  exit=0   ~80ms
$ thinkvision-control.exe --help  help                                    exit=0   79ms
$ thinkvision-control.exe --version   thinkvision-control 0.0.8           exit=0   67ms
$ thinkvision-control.exe -h      help                                    exit=0  103ms
```
- The help now documents:
  - `--version`, `--display name|id|all`, `--yes` and `tvc`;
  - exit codes 2 (bad arguments) and 3 (input switch needs `--yes`);
  - `thinkvision://display/<…>/…` links.
- **The version prints 0.0.8**, because `Cargo.toml` hasn't been bumped yet. The log's `Start:` lines say 0.0.8 too.
- In Git Bash, `tvc.exe` passes on stdout and the exit code (0 and 2 both seen). Running the GUI exe directly from Git Bash also works.

### 1.2 `--get` (no copy running)

```
$ tvc --get all                 brightness: 0 / contrast: 75 / input: DisplayPort / colourMode: Neutral  exit=0 1441ms
$ tvc --get all                 (same)                                                                    exit=0 1450ms
$ tvc --get all --json          {"brightness":0,"contrast":75,"input":"DisplayPort","colourMode":"Neutral"} exit=0 1404ms
$ tvc --get brightness          brightness: 0         exit=0  208ms
$ tvc --get brightness          brightness: 0         exit=0  224ms
$ tvc --get contrast            contrast: 75          exit=0  213ms
$ tvc --get input               input: DisplayPort    exit=0  197ms
$ tvc --get colour-mode         colourMode: Neutral   exit=0  206ms
$ tvc --get color-mode          colourMode: Neutral   exit=0  224ms
$ tvc --get=brightness --json   {"brightness":0}      exit=0  204ms
$ tvc --GET=Contrast            contrast: 75          exit=0  211ms
$ tvc thinkvision://get/all     (all four lines)      exit=0 1412ms
$ tvc thinkvision:get/brightness/  brightness: 0      exit=0  214ms
$ gui --get all                 (all four lines)      exit=0 1402ms
$ gui --get brightness --json   {"brightness":0}      exit=0  196ms
```
**APP-23 / LIVE-11 fixed:** a single value takes about 0.2 s (it was 1.4 s). The log shows about 105 ms from "attached" to the result. `--get all` still takes about 1.4 s, the same as 0.0.8.

`--display` (new):
```
$ tvc --display LEN62A2-SVNA600D7 --get brightness   brightness: 0      exit=0 230ms
$ tvc --display "len p32p-20" --get contrast --json   {"contrast":75}    exit=0 228ms
$ tvc --display all --get brightness                  brightness: 0      exit=0 221ms
$ tvc --display all --get all --json                  {single object}    exit=0 1430ms  (one display, so one object, as documented)
```

### 1.3 Brightness and contrast round trips (no copy running)

```
$ tvc --step brightness=+5           Brightness 5                                        exit=0 3474ms
$ tvc --get brightness               brightness: 5                                       exit=0  211ms
$ tvc --step brightness=-5 --json    {"ok":true,"kind":"Ok","message":"Brightness 0"}    exit=0 3466ms
$ tvc --get brightness               brightness: 0                                       exit=0  195ms
$ tvc --step contrast=+5 --json      {"ok":true,"kind":"Ok","message":"Contrast 80"}     exit=0 3480ms
$ tvc --get contrast                 contrast: 80                                        exit=0  190ms
$ tvc --step=contrast=-5             Contrast 75                                         exit=0 3458ms
$ tvc --get all                      0 / 75 / DisplayPort / Neutral                      exit=0 1409ms
```
A headless step still takes about 3.5 s: about 1.9 s to the result, then 1.5 s for the OSD to fade. Log for one step:
```
12:02:33.577 Start: thinkvision-control 0.0.8, command line --step contrast=+5
12:02:33.579 Hub: LEN62A2-SVNA600D7 attached (External, Ddc)
12:02:33.580 Language: en
12:02:33.581 Monitor LEN62A2-SVNA600D7 attached
12:02:35.468 CLI 0x12 [LEN62A2-SVNA600D7]: Ok, Contrast 80
12:02:35.468 CLI --step contrast=+5: exit 0, Contrast 80
12:02:36.970 Exit 0
```
Brightness steps log `[LEN62A2-SVNA600D7] Brightness DDC write verified: requested=5, readback=5, max=100.`; **contrast steps still log no "verified" line**.

### 1.4 Invalid arguments (no copy running, 7 s timeout)

```
$ tvc --get bogus              'bogus' is not something --get can read (brightness, contrast, input, colour-mode, all)  exit=2 89ms
$ tvc --get                    '--get' needs a value (brightness, contrast, input, colour-mode, all)                   exit=2 68ms
$ tvc --step brightness=+500   step +500 is out of range (-100 to +100)                                               exit=2 70ms
$ tvc --step brightness=abc    'abc' is not a number like +5 or -5                                                     exit=2 69ms
$ tvc --step volume=+5         'volume' can't be stepped (brightness, contrast)                                        exit=2 68ms
$ tvc --preset evening         'evening' is not a preset (day, night)                                                  exit=2 75ms
$ tvc --input vga              'vga' is not an input (hdmi, dp, usbc, this, 1, 2, 3)                                   exit=2 77ms
$ tvc --foo                    unknown option '--foo'                                                                  exit=2 71ms
$ tvc bogus                    unexpected argument 'bogus'                                                             exit=2 74ms
$ tvc thinkvision://bogus      'thinkvision://bogus' is not a known thinkvision: link (see --help)                     exit=2 84ms
$ tvc --display nonexistent --get all
                               no display is called 'nonexistent' (attached: LEN P32p-20 (LEN62A2-SVNA600D7))          exit=2 81ms
$ gui --get bogus                                                                                                       exit=2 56ms
```
- No window opened, and no process was left running.
- Each message is followed by the full help (about 40 lines).
- With a resident running, `--step brightness=+500` is answered by the resident: the same text, exit 2, 88 ms.

### 1.5 With a resident copy; the crash marker; `--input`

The resident was started in window mode (PID 18564).

Forwarded commands:
```
$ tvc --get all                    0 / 75 / DisplayPort / Neutral        exit=0 494ms
$ tvc --get brightness --json      {"brightness":0}                      exit=0 146ms
$ tvc --step brightness=+5         Brightness 5                          exit=0 604ms
$ tvc --get brightness             brightness: 5                         exit=0 145ms
$ tvc --step brightness=-5 --json  {"ok":true,"kind":"Ok","message":"Brightness 0"}   exit=0 605ms
$ tvc --get brightness             brightness: 0                         exit=0 152ms
$ tvc --step contrast=+5 / -5      Contrast 80 / Contrast 75             exit=0 629 / 595ms
$ tvc --version                    thinkvision-control 0.0.8             exit=0  89ms
log: "CLI --step brightness=-5 (forwarded): exit 0, Brightness 0"
```
**LIVE-08 / APP-07 fixed:** forwarded verbs return the resident's real output and exit code, and honour `--json`. Forwarded steps take about 0.6 s, compared with 3.5 s headless.

Crash marker:
- The marker is now per process and per monitor: `monitors/LEN62A2-SVNA600D7/risky-operation-pid-<pid>.txt`, containing `capability read`. It brackets only the `capabilities()` DDC call (engine.rs:795-797).
- **Race, run with a FileSystemWatcher** (`scratchpad\race.ps1`; the resident was PID 17804):
  ```
  gui start                         0 ms
  marker risky-operation-pid-17804  created at 3934 ms
  tvc --get all started             3945 ms   (11 ms later, while the marker was armed)
  marker deleted (by the resident)  6241 ms   (held 2.3 s, like 0.0.8's 2.45 s)
  --get all                         exit=0 at 6684 ms: 0 / 75 / DisplayPort / Neutral   (forwarded, waited for the read)
  log                               "CLI --get all (forwarded): exit 0, …"; no "Safe mode" line
  ```
- A first attempt with a bash poller (`mwatch.sh`) reported the marker gone after about 110 ms. That poller was unreliable; the FileSystemWatcher numbers above are the reliable ones.
- **Planted marker:** I wrote `risky-operation-pid-<resident pid>.txt` myself, then ran `tvc --get all` (exit 0, 456 ms) and `tvc --get brightness --json` (exit 0, 120 ms). Both forwarded. The planted file was untouched afterwards, and no "Safe mode" line was logged.
- **LIVE-01/02 (APP-03, CORE-12) fixed.** "Safe mode" never appears in the scratch log over the whole run.

`--input dp` without `--yes`:
```
headless (no copy running):
$ tvc --input dp          Switching the input needs confirmation: run it again with --yes   exit=3 2927ms
$ tvc --input dp --json   {"ok":false,"kind":"NeedsConfirmation","message":"Switching the input needs confirmation: run it again with --yes"}  exit=3 2926ms
$ tvc --get input         input: DisplayPort                                                 exit=0  214ms
forwarded (tray resident PID 20468):
$ tvc --input dp          (no output)   exit=124 8091ms  <- waiting for the on-screen answer; killed by my timeout
```
- The resident opened its window with the confirm "Switch to DisplayPort?", body "You won't be able to control the monitor from this PC until you switch back.", and the buttons Switch / **Cancel** (Cancel is the default). Screenshot: `scratchpad\live010-input-confirm.png`.
- I didn't click anything; I ended the resident by its PID. The log then shows `CLI --input dp (forwarded): exit 3, The input switch wasn't confirmed on screen`.
- The input stayed DisplayPort.

### 1.6 Data files (scratch copy)

The first headless run (`--get all`, 12:01:15) migrated the folder:
```
12:01:15.214 Start: thinkvision-control 0.0.8, command line --get all
12:01:15.216 Hub: LEN62A2-SVNA600D7 attached (External, Ddc)
12:01:15.217 Hub: moved capabilities.txt to LEN62A2-SVNA600D7
12:01:15.218 Language: en
12:01:15.219 Monitor LEN62A2-SVNA600D7 attached
12:01:15.220 Settings: LEN62A2-SVNA600D7 takes the 0.0.8 input and colour modes
12:01:16.553 CLI --get all: exit 0, brightness: 0, contrast: 75, input: DisplayPort, colourMode: Neutral
12:01:16.554 Exit 0
```
Resulting tree:
```
log.txt
settings.json                                    (rewritten, 824 -> 1353 bytes)
monitors/LEN62A2-SVNA600D7/capabilities.txt      (443 bytes, sha256 6aa2c79b…, mtime kept 2026-10-28 08:23:47)
monitors/LEN62A2-SVNA600D7/identity.json
```
- **The id uses the EDID serial (`-S`).** `identity.json` reads `{"id":"LEN62A2-SVNA600D7","edidId":"LEN62A2","serial":"VNA600D7","connector":"5&1e1bbfa8&0&UID28932"}`.
  - The registry EDID (`HKLM\…\Enum\DISPLAY\LEN62A2\<inst>\Device Parameters\EDID`) has the serial descriptor `VNA600D7` on both instances. The 32-bit serial is 0 on UID28932 and 16843009 (0x01010101) on the stale UID28931.
  - So the descriptor was read, and the connector is the live instance.
- No backups or overlay existed to move. No `settings.json.bak` was written before the migration rewrote settings.json.
- **settings.json:**
  - The user's values are kept: all 11 hotkeys, now written as `Ctrl+Alt+Up` instead of `\u002B`; Day 0/75/0x06; Night 0/20/0x06; ThisMachineInput 0x0F; StartWithWindows false; BackgroundMode true.
  - New fields: `AutoDayNight` {Mode "Off", 07:00, 19:00, no location}, `LastAppliedPhase` null and `HotkeyTarget` "All".
  - `Monitors` → `LEN62A2-SVNA600D7` {Name null, ThisMachineInput 0x0F, Day, Night, Included true, Offset 0}.
  - `LegacyAdopted` true and `Language` "System".
- **log.txt:** one `Start:` line per process (version and mode: `command line …`, `window`, `tray`), one `CLI <verb>: exit N, <result>` line per verb (`(forwarded)` when relayed) and an `Exit N` line.
  - Write lines read `CLI 0x12 [LEN62A2-SVNA600D7]: Ok, Contrast 80`, with no Rust debug formatting.
  - Gaps:
    - contrast writes have no "verified" line;
    - a successful capability read logs nothing;
    - the `command line` field is the normalised verb, not the arguments: `thinkvision://get/all` is logged as `--get all`, and `--json` / `--display` are dropped.

### 1.7 Start-up time

| Measure | 0.0.8 (verify-live.md) | 0.1.0 |
| --- | --- | --- |
| Window start: spawn → main window handle | not measured; window opened | 195–309 ms (4 runs) |
| Spawn → `Start:` log line → `Monitor … attached` | — | ~70 ms → +20–65 ms |
| Window start → live capability read (marker armed) | ~3.7 s, held 2.45 s | 3.9–4.1 s, held ~2.3 s |
| Tray start (`--startup`) → capability read | skipped in 2 of 3 runs | 3.4 s, held 2.2 s (1 run) |
| Tray start → answering forwards | — | `Monitor attached` 21 ms after `Start:`; a forwarded `--get brightness` 6 s later took 113 ms |
| Headless `--get all` | 1.40 s | 1.39–1.45 s |
| Headless `--get brightness` | 1.40 s | 0.19–0.23 s |
| Headless `--step` | 3.5 s | 3.5 s |

### 1.8 OSD (APP-16)

`scratchpad\wins.ps1` enumerates the resident's top-level windows:
```
'ThinkVision OSD'       vis=True  ex=0x80801B8 [TOOLWINDOW, NOACTIVATE, TOPMOST, TRANSPARENT, LAYERED]  320×64 logical, bottom centre of the primary
'ThinkVision Control'   vis=True  ex=0x40110   [APPWINDOW]   (the main window, in window mode)
'thinkvision-control'   vis=False ex=0x40110   [APPWINDOW]   (hidden helper)
global_hotkey_app, ThinkVisionControl.MonitorEvents, Winit Thread Event Target: hidden or tool windows
```
- **No taskbar button, no focus theft.** TOOLWINDOW keeps the OSD off the taskbar and Alt+Tab, and NOACTIVATE stops it taking focus.
- I sampled the foreground window every 250 ms for 2 s during a forwarded `--step`. It stayed the user's browser (brave, PID 11312) throughout.
- **The OSD draws.** A CAPTUREBLT capture 400 ms after a step shows the "Brightness 5" pill at the bottom centre of the primary display, above the taskbar.
  - A plain `CopyFromScreen` misses the OSD, because it is layered.
  - The full-screen captures were deleted, because they showed the user's browser.
- The OSD window is created lazily, by the first command, and then kept at alpha 0 (`IsWindowVisible` is always true).
- In tray mode, no visible app window exists, only the OSD and tool windows.

### 1.9 Language

- I stopped the resident, set `"Language": "de"` in the scratch settings.json and started the GUI (PID 11616).
- The log shows `12:09:44.613 Language: de`.
- `PrintWindow` capture (`scratchpad\live010-lang-de.png`): Bildschirm / Monitor / Einstellungen; Eingang (HDMI, DisplayPort "Dieser PC", USB-C); Helligkeit und Kontrast; Voreinstellungen (Tag / Nacht, "Helligkeit 0, Kontrast 75, Neutral"); Farbe / Farbmodus; "Eigene Farbe – Verfügbar im Farbmodus Benutzer / Eigene". Everything is translated.
- Afterwards, the key was set back to `"Language": "System"`, its post-migration value.

### 1.10 Concurrency (read-only)

```
8 × tvc --get all --json in parallel (no copy running):
  7 × {"brightness":0,"contrast":75,"input":"DisplayPort","colourMode":"Neutral"}  exit=0  1.8 s … 4.4 s (forwarded, one after another)
  1 × (no output)  exit=124  30076 ms   <- the first one; killed by timeout
3 × thinkvision-control.exe --get brightness in parallel (no copy running):
  2 × brightness: 0  exit=0  265 / 366 ms (forwarded)
  1 × (no output)    exit=124  15063 ms   <- the first one
log: 12:11:53.175 CLI --get brightness: brightness: 0          <- no "exit N" and no "Exit" line: it never printed or exited
     12:11:53.284 CLI --get brightness (forwarded): exit 0, brightness: 0
     12:11:53.394 CLI --get brightness (forwarded): exit 0, brightness: 0
```
- The 0.0.8 failure mode is gone: 8 parallel `--get` runs all ended at exit 8 with no output. There is a new one; see N1.
- No "Safe mode" lines were logged.

### 1.11 Stopping a copy with a plain `taskkill`

- A resident that had already created its OSD exited cleanly on `taskkill /PID` and logged `Exit 0` within 3 s. This happened four times.
- The German-language resident had run no command, so it had no OSD. It only hid its window to the tray, which is correct for BackgroundMode. It needed `/F`.
- This is by design (osd.rs:100-106: closing the OSD quits in order). Windows sign-out and Restart Manager go through `WM_ENDSESSION` (monitor_events.rs:371).

### 1.12 Hotplug

**Pending.** It needs someone to pull and replug the cable, so it wasn't run.

## 2. Results

### (a) Pass/fail against the 0.0.8 issues and the new items

| # | 0.0.8 issue | 0.1.0 result |
| --- | --- | --- |
| 1 | `--get` deletes the running copy's crash marker | **Pass.** The markers are per PID (`monitors/<id>/risky-operation-pid-<pid>.txt`) and `--get` forwards. In the race, `--get` started 11 ms after the marker appeared, and the resident removed its own marker 2.3 s later. A planted marker for a live PID survived two `--get` runs. |
| 2 | False "Safe mode: crashed during capability read" | **Pass.** No "Safe mode" line in the whole run. |
| 3 | Malformed verbs open the window or hang | **Pass.** All 10 cases plus `--display nonexistent`: exit 2 in 56–89 ms, with a message, and no window. Through a resident: exit 2 as well. |
| 4 | No `--version` | **Pass.** `thinkvision-control 0.0.8`, exit 0. The version isn't bumped yet; `-V` isn't supported (exit 2). |
| 5 | 5 s headless timer too tight; exit 8 with no output | **Changed / not reproduced.** 8 parallel reads no longer time out (exit 8): they forward to the first one. The timer path wasn't triggered live. The first process now hangs instead (N1). |
| 6 | Exit 0 with no output when the result is late | **Not tested live.** No way to delay the monitor safely. |
| 7 | Model naming hard-coded | **Pass as far as visible.** The header reads "ThinkVision P32p-20" with "Firmware LB1.2". The display list uses the EDID name "LEN P32p-20" and the id `LEN62A2-SVNA600D7`. Live, I can't tell the capability string's `model()` from the profile. |
| 8 | `--json` dropped on forwarded verbs | **Pass.** Forwarded verbs return the real output, the exit code and JSON. |
| 9 | Rust debug formatting in the log | **Pass.** `CLI 0x12 [LEN62A2-SVNA600D7]: Ok, Contrast 80`. |
| 10 | Log too thin | **Partial.** Start (version, mode), every CLI verb with its exit code, and Exit are now logged. Still missing: the contrast "verified" line and successful capability reads, and the raw arguments (N4). |
| 11 | `--get brightness` reads every code | **Pass.** 0.19–0.23 s (was 1.40 s). `--get all` is still 1.4 s. |
| 12 | Log roll-over (Info) | Not retested. |
| 13 | Tray start skips the capability read | **Pass (1 sample).** Read at 3.4 s after a `--startup` start. |
| 14 | settings.json without AutoDayNight (Info) | **Changed.** The migration writes `AutoDayNight` {Mode Off}. ContrastUp is still Ctrl+Alt+Left; those are the user's own saved keys. |
| New | Migration to `monitors/<id>/` | **Pass.** `LEN62A2-SVNA600D7` with the S-serial from the EDID descriptor (matches the registry), the live connector, the user's values kept, and `Monitors`, `LegacyAdopted` and `Language` added. |
| New | `--display` | **Pass.** Id, case-insensitive name and `all` all work. |
| New | `--input` without `--yes`: headless | **Pass.** Exit 3 with a message and JSON; no switch. |
| New | `--input` without `--yes`: forwarded | **Pass.** It asks on screen with Cancel as the default. Unanswered, the caller waits (N2). |
| New | Start-up time | **Pass.** Window in 0.2–0.3 s; capability read 3.4–4.1 s, as before. |
| New | OSD (APP-16) | **Pass.** TOOLWINDOW, NOACTIVATE and TOPMOST; focus stayed on the user's app; drawn at the bottom centre of the primary display. |
| New | Language | **Pass.** `Language: de` logged; the UI is fully German. |
| New | `tvc.exe` relay in Git Bash | **Pass.** stdout and exit codes 0, 2 and 3 come through. |
| New | Hotplug | **Pending.** It needs a cable pull. |

### (b) New issues

| # | Severity | Issue | Suggested fix |
| --- | --- | --- | --- |
| N1 | **High** | **When CLI runs overlap, the first one never returns.** If a second run starts while the first headless run is still working, the first one is promoted to the resident app (`on_forwarded`, controller.rs:1017, when BackgroundMode is on). Its own job then takes the non-headless branch of `finish` (controller.rs:886), which only logs. The caller gets no output and no exit code, and the process stays alive with no window and no end. Reproduced twice (8 × `tvc --get all`, 3 × `--get brightness`); the first one ran until my 30 s and 15 s timeouts. A step leaves a 3.5 s window for this, so a status-bar poller plus a hotkey script will hit it. | Don't promote on a forwarded verb: answer it and exit once the own job and the forwarded replies are done. Promote only for `Show`. At least print the own result and set the exit code before promoting, and say in the log that the process is now resident. |
| N2 | Low | A forwarded `--input` without `--yes` blocks the calling script until someone answers on screen, with no timeout and no output. I killed it at 8 s. | Document it, or time the confirm out (for example after 60 s, Cancel, exit 3). |
| N3 | Low | The input confirm always shows the profile's body, "You won't be able to control the monitor from this PC until you switch back". It showed when switching to DisplayPort, which is this PC's input (`ThisMachineInput` 0x0F) and the current one. | Use that body only when the target isn't this PC's input. Otherwise show none, or skip the confirm for a no-op switch. |
| N4 | Low | The log still lacks a contrast "verified" line and successful capability reads. The `command line` field shows the normalised verb: `thinkvision://get/all` is logged as `--get all`, and `--json` / `--display` are dropped. | Log a verify line for every Range write, plus `Capabilities read (live, N bytes, matches cache)`. Log the raw arguments. |
| N5 | Low | A usage error prints the message and then the whole ~40-line help. Through a resident, both go to stdout, in one reply. Not checked whether the direct case uses stderr (`2>&1` was merged). | Print the message and `Run with --help for usage.` on stderr. |
| N6 | Info | The version is still 0.0.8 (`--version` and the log's `Start:` line). | Bump before tagging. |
| N7 | Info | Any 0.1.0 start that runs resident re-points `HKCU\Software\Classes\thinkvision` at its own exe. This is by design, and 0.0.8 does the same (controller.rs:272 at v0.0.8). Running a test or portable copy takes over links until the installed copy starts again. | None; it's documented in url_scheme.rs. Testers should export and re-import the key. |
| N8 | Info | The migration rewrites settings.json without keeping the 0.0.8 file, and moves capabilities.txt out of the folder root. Downgrading to 0.0.8 still works: extra fields are ignored and the cache is read live again. | Optional: write `settings.0.0.8.json` once before the first rewrite. |
| N9 | Info | A plain `taskkill` (WM_CLOSE) exits a copy only after its OSD exists. Before that, it just hides the window to the tray (§1.11). Restart Manager uses WM_ENDSESSION, so installers aren't affected. | None needed. |

## 3. Restoration

- **Monitor:**
  - Originals: brightness 0, contrast 75, DisplayPort, Neutral.
  - The final headless `tvc --get all` at 12:12, after the last write: `brightness: 0 / contrast: 75 / input: DisplayPort / colourMode: Neutral`, exit 0.
  - Only 0x10 and 0x12 were written, always in +5/-5 pairs and read back. Input, power, colour mode, language and factory reset were never sent. `--yes` was never passed.
- **Real data folder** `%APPDATA%\ThinkVisionControl` (never used by 0.1.0), checked before and after 0.0.8's restart:
  ```
  6aa2c79b68bcfdab15d188c4405939c220061db7d4ae140d26df2b1fd1605344 *capabilities.txt   (matches backup)
  fd831a1089cd6e4d7c27e5ed3acd659e9d89274df6a56f6113a412534582d5d1 *log.txt            (matches backup; 0.0.8 hadn't appended yet)
  a581cfc0e975f4df358e404f7367c78d2faf5977a93828091ad5a151fac4f475 *settings.json      (matches backup)
  ```
  The folder still holds only those three files. Its mtime (10:19:54) is unchanged, and no `monitors/` folder was created.
- **Registry:**
  - The 0.1.0 GUI had re-pointed `HKCU\Software\Classes\thinkvision` (DefaultIcon and `shell\open\command`) at `scratchpad\exe-0.1.0\thinkvision-control.exe`.
  - I restored it with `reg import scratchpad\reg-thinkvision.reg`. A fresh export is byte-identical to the original (`cmp` clean), pointing at `C:\Program Files\ThinkVision Control\thinkvision-control.exe`.
  - `HKCU\…\CurrentVersion\Run` had no ThinkVisionControl value before and after (StartWithWindows is false). Its export is byte-identical.
- **0.0.8 restarted:**
  - PID 10852: `"C:\Program Files\ThinkVision Control\thinkvision-control.exe" --startup`, with the real APPDATA. It's running in the tray.
  - The original PID 6708 had a bare command line (no `--startup`), and the Run key has no value. I used `--startup` as instructed, so the app runs in the tray instead of opening a window.
  - PID 6708 ignored a plain `taskkill` for more than 10 s and was ended with `/F`.
- **No 0.1.0 process is running.** I started 6 GUI instances (PIDs 18564, 19172, 17564, 17804, 20468 and 11616), each stopped by its own PID. The hung headless hosts in §1.10 were killed by `timeout`. The last `Get-Process thinkvision-control,tvc` lists only 10852 (Program Files).
- Scratch files kept: `scratchpad\live-appdata\` (the migrated copy and its log), `reg-*.reg`, `live010-input-confirm.png` and `live010-lang-de.png`.

## 4. Re-check after the review fixes (2026-11-15)

- Build: `scratchpad\exe-0.1.0c\thinkvision-control.exe` (10,904,576 bytes) and `tvc.exe` (304,128 bytes), after `5217032`.
- Read-only: only `--get`, `--help`, `--version` and two bad-argument runs. No `--step`, `--input` or `--preset`.
- Scratch data: `scratchpad\live-appdata2\ThinkVisionControl`, a fresh copy of `appdata-backup-010`. BackgroundMode is true in that copy. Helper: `scratchpad\r11.sh` (like `r10.sh`, but with a 60 s `timeout` and an optional separate stderr file).
- The installed 0.0.8 (PID 10852, `--startup`) exited 1 s after a plain `taskkill //PID`; `/F` wasn't needed.
- The monitor now reads **contrast 80**. The user set it with a hotkey at 13:12 (0.0.8's log: `Hotkey Some(18) None: Ok Contrast 80`). This check wrote nothing.

### 4.1 N1: overlapping one-shot runs

```
3 × tvc --get all in parallel:   all exit=0 with 4 value lines   1465 / 2978 / 4475 ms
8 × tvc --get all in parallel:   all exit=0 with 4 value lines   1540 / 2838 / 3978 / 5550 / 6842 / 8010 / 9581 / 11068 ms
3 × thinkvision-control.exe --get brightness in parallel:   all exit=0, brightness: 0   249 / 491 / 735 ms
```
- The runs take turns. Each logs its own `Start:` … `CLI --get all: exit 0, …` … `Exit 0`, about 1.33 s apart for `--get all`. None was forwarded, and none was promoted to a resident.
- The log has no "busy" or "forward" line for these runs. The waiting isn't logged, only each run's own work.
- No `thinkvision-control` or `tvc` process was left after any batch.
- **N1 fixed.**

### 4.2 Mixed run

```
tvc --get all --json   {"brightness":0,"contrast":80,"input":"DisplayPort","colourMode":"Neutral"}   exit=0 1454 ms
tvc --get brightness   brightness: 0                                                                   exit=0 1757 ms (waited for the first)
```
The JSON parses. **Pass.**

### 4.3 Forward to a running GUI

The resident was started with `--startup` (tray, PID 16832). It logged `Capabilities read (live, 443 bytes, matches the cache)` after 5.7 s.
```
tvc --get all           4 value lines            exit=0 492 ms
tvc --get all --json    {single object}          exit=0 482 ms
tvc --get colour-mode   colourMode: Neutral      exit=0 165 ms
tvc --get bogus         message + hint (stderr)  exit=2 107 ms
log: CLI --get all (forwarded): exit 0, …  /  CLI --get all --json (forwarded): exit 0, …  /  CLI --get colour-mode (forwarded): exit 0, …
```
A plain `taskkill //PID 16832` stopped it in 1 s; it logged `Exit 0`. **Pass.**

### 4.4 Spot checks

- `--get colour-mode` / `--get color-mode --json` (headless): `colourMode: Neutral` / `{"colourMode":"Neutral"}`, exit 0, about 0.22–0.25 s, for both exes. **Pass.**
- `--version` (both exes): `thinkvision-control 0.0.8`, exit 0. **The version is still 0.0.8** (N6 is still open). The log's `Start:` lines say 0.0.8 too.
- Usage errors (`--get bogus`, `--foo`, both exes):
  - stdout is empty;
  - stderr has two lines, for example `'bogus' is not something --get can read (brightness, contrast, input, colour-mode, all)` and then `Run with --help for usage.`;
  - exit 2, in 88–113 ms.
  - **N5 fixed.**
- Logging: the `command line` field now keeps `--json` (`command line --get all --json`), and a successful capability read is logged. These are parts of N4.
- No "Safe mode" line appears in the scratch log.

### 4.5 Restoration

- **Real data folder:** the hashes match the snapshot taken before this check, both before and after 0.0.8's restart. `capabilities.txt` and `settings.json` also match `appdata-backup-010`. `log.txt` (3,035 bytes) starts with the backup's 2,801 bytes. The extra lines are from 0.0.8 between 12:33 and 13:12, before this check began: DisplayOff, DisplayOn, Awake, and the contrast hotkey. The restarted 0.0.8 added nothing.
  ```
  6aa2c79b68bcfdab15d188c4405939c220061db7d4ae140d26df2b1fd1605344 *capabilities.txt
  4700625213b313544ece4ca45ecde8561ebd432a9b9e0dc91b90c707e9e4eb88 *log.txt
  a581cfc0e975f4df358e404f7367c78d2faf5977a93828091ad5a151fac4f475 *settings.json
  ```
- **Registry:** the resident had re-pointed `HKCU\Software\Classes\thinkvision` at `exe-0.1.0c`, as N7 describes. After `reg import scratchpad\rc2\reg-thinkvision.reg`, both the `thinkvision` and `Run` exports are byte-identical to the ones taken before the check.
- **0.0.8 restarted** with no arguments, as the user normally starts it: PID 8976, `"C:\Program Files\ThinkVision Control\thinkvision-control.exe"`, started 14:02:20.
- **No 0.1.0 process is running.** The last process list shows only PID 8976.
