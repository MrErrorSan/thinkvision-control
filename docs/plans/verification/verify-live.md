# ThinkVision Control: live check of the installed release (v0.0.8)

- Date: 2026-11-14, Windows 11 Pro 10.0.26200, Git Bash.
- Program: `C:\Program Files\ThinkVision Control\thinkvision-control.exe` (9,323,008 bytes, dated 29 Oct). Source compared against tag `v0.0.8`. HEAD (`3913f5a`, unreleased) differs in view.rs, controller.rs, engine.rs, link.rs and dxva2.rs.
- Monitor: one Lenovo ThinkVision P32p-20 on DisplayPort.
- The app was **not running** at the start of the test. Every GUI instance listed below was started by me and closed with `taskkill` on its own PID. No instance is running now.
- Timing helper: `scratchpad\run.sh` runs `timeout ${T:-20} "<exe>" args`, then prints the output, the exit code and the elapsed time. When `timeout` reports exit 124, the program was still running at the deadline and was killed.

## 1. Commands run, outputs, exit codes

### 1.1 Help / version

```
$ thinkvision-control.exe --help        -> exit=0  77ms
ThinkVision Control

Usage: thinkvision-control [verb] [--json] [--startup]

Verbs (the first one counts):
  --preset day|night                 Apply the Day or Night preset
  --input hdmi|dp|usbc|this          Switch the input (this = this PC's input)
  --step brightness|contrast=+N|-N   Change brightness or contrast by N (up to 100)
  --get brightness|contrast|input|colour-mode|all
                                     Print current values (always reads the monitor itself)
  --help, -h, /?                     Show this text
  (none)                             Open the window

Flags:
  --json      Print the result as JSON
  --startup   Start in the tray without a window

Exit codes: 0 OK, 1 monitor not found, 2 value out of range, 4 not supported,
            5 monitor not answering (or asleep, or safe mode), 8 timed out.
A verb handed to a running copy exits 0; the result shows on screen there.

Links (thinkvision://):
  thinkvision://open
  ...
  thinkvision://get/all
$ thinkvision-control.exe -h            -> same text, exit=0  60ms
$ thinkvision-control.exe /?            -> same text, exit=0  62ms
$ thinkvision-control.exe --get all --help -> same text (help wins), exit=0  61ms
$ thinkvision-control.exe --version     (no app running)
(no output)
exit=124 elapsed=8107ms        <- the full GUI window opened; killed by timeout
$ thinkvision-control.exe --version     (GUI running)
(no output)
exit=0 elapsed=184ms           <- forwarded; the running window is brought forward
```

The wording is generic. "ThinkVision Control" is the product name, and the help text names no monitor model. There is no `--version` option.

### 1.2 `--get` (no GUI running)

```
$ --get all                 brightness: 0 / contrast: 20 / input: DisplayPort / colourMode: Neutral   exit=0 1400ms
$ --get all                 (same)                                                                      exit=0 1395ms
$ --get all --json          {"brightness":0,"contrast":20,"input":"DisplayPort","colourMode":"Neutral"} exit=0 1375ms
$ --get brightness          brightness: 0        exit=0 1402ms
$ --get contrast            contrast: 20         exit=0 1403ms
$ --get input               input: DisplayPort   exit=0 1399ms
$ --get colour-mode         colourMode: Neutral  exit=0 1408ms
$ --get color-mode          colourMode: Neutral  exit=0 1418ms
$ --get=brightness --json   {"brightness":0}     exit=0 1392ms
$ --GET=Contrast            contrast: 20         exit=0 1396ms
$ thinkvision://get/all     (all four lines)     exit=0 1378ms
$ thinkvision:get/brightness/  brightness: 0     exit=0 1411ms
```

Every `--get` takes about 1.4 s, including single-value reads. It reads every Main and Advanced code (0xE0, 0xEA, 0xCC, 0xEC and others, as the shutdown log lines show), not only the code that was asked for.

### 1.3 Brightness and contrast round trips (no GUI running)

```
$ --get brightness              brightness: 0                                     exit=0 1406ms
$ --step brightness=+10         Brightness 10                                     exit=0 3456ms
$ --get brightness              brightness: 10                                    exit=0 1418ms
$ --step brightness=-10         Brightness 0                                      exit=0 3492ms
$ --get brightness              brightness: 0                                     exit=0 1405ms
$ --get contrast                contrast: 20                                      exit=0 1404ms
$ --step contrast=+15 --json    {"ok":true,"kind":"Ok","message":"Contrast 35"}  exit=0 3474ms
$ --get contrast                contrast: 35                                      exit=0 1408ms
$ --step=contrast=-15           Contrast 20                                       exit=0 3493ms
$ --get all                     brightness 0, contrast 20, DisplayPort, Neutral   exit=0 1415ms
```

Both round trips worked and returned exit 0. A headless `--step` takes about 3.5 s, because it waits 1.5 s for the on-screen display (OSD) to fade. That leaves only about 1.5 s of margin before the 5 s safety timer.

Forwarded round trip (GUI running, PID 18104):
```
$ --step brightness=+10         (no output) exit=0 124ms
$ --get brightness              brightness: 10  exit=0 1404ms
$ --step brightness=-10 --json  (no output) exit=0 118ms     <- --json is silently ignored when forwarded
$ --get brightness              brightness: 0   exit=0 1382ms
```

### 1.4 Invalid arguments

No GUI running (7 s timeout):
```
$ --get bogus               (no output) exit=124 7086ms   GUI window opened, never exits
$ --get                     (no output) exit=124 7080ms
$ --step brightness=+500    (no output) exit=124 7079ms
$ --step brightness=abc     (no output) exit=124 7082ms
$ --step volume=+5          (no output) exit=124 7077ms
$ --preset evening          (no output) exit=124 7076ms
$ --input vga               (no output) exit=124 7084ms
$ --foo                     (no output) exit=124 7081ms
$ thinkvision://bogus       (no output) exit=124 7075ms
$ bogus                     (no output) exit=124 7098ms
```
GUI running:
```
$ --get bogus               (no output) exit=0 119ms   (forwarded as "open window")
$ --version                 (no output) exit=0 184ms
$ (no args)                 (no output) exit=0 192ms
```

### 1.5 `--get` while the GUI is running, and the crash marker

The GUI (window mode) starts its live capability read about 3.7 s after launch. The read holds `risky-operation.txt` for about 2.45 s. Measured undisturbed: armed at 17:47:08.884, removed at 17:47:11.388.

Run with a tight `$EPOCHREALTIME` watcher (fresh GUI, then `--get all` the instant the marker appeared):
```
GUI start                   1794660477.954
marker appears              1794660481.732170   (GUI capability read starts)
--get all launched          1794660481.732159
marker removed              1794660481.762935   <- 31 ms later, by the --get process, not the GUI
--get all output            brightness: 0 / contrast: 20 / input: DisplayPort / colourMode: Neutral
--get exit                  0, after 3.71 s (vs 1.4 s normally; contention with the GUI's read)
log.txt                     "2026-11-14 17:48:01.762 Safe mode: crashed during capability read"
```
The GUI's own capability read still finished without a "Capability read failed" line, and the GUI stayed up.

Concurrency (read-only):
```
4 x --get all in parallel (GUI running)   all exit=0, ~3.77 s each, correct values
8 x --get all --json in parallel          ALL exit=8, ~5.6 s, (no output) — not even {"error":...}
                                           log: 8 x "Read 0xEA|0xCC|0xE0|0xEC failed: connection lost: shutting down"
4 x --get + --step brightness=+10 (GUI running)  step forwarded: exit=0 818ms (no output); write done by GUI
4 x --get + --step brightness=-10 (no GUI)       step: "Brightness 0" exit=0 5084ms (just over the 5 s net)
6 x --get + --step contrast=+10 (no GUI)         step: (no output) exit=8 5411ms; no write (contrast stayed 20)
                                                  5 of 6 --get: exit=8 no output; 1: values, exit=0 5442ms
```

### 1.6 Data files

`log.txt` lines added during the test (all removed again by the restore):
```
2026-11-14 17:43:42.540 Brightness DDC write verified: requested=10, readback=10, max=100.
2026-11-14 17:43:42.540 Cli Some(16) None: Ok Brightness 10
...
2026-11-14 17:43:53.936 Cli Some(18) None: Ok Contrast 35        <- no "verified" line for contrast
2026-11-14 17:48:01.762 Safe mode: crashed during capability read <- false; written by --get
2026-11-14 17:49:38.275 Read 0xEA failed: connection lost: shutting down   (x8, from the timed-out --gets)
...
```
Nothing is logged for any `--get`, for the eight runs that timed out with exit 8, for GUI start or stop, for the version, or for the capability reads that succeeded.

`capabilities.txt` was unchanged: same SHA-256 (`6aa2c79b…`) and mtime (2026-10-28 08:23:47) after three live capability reads by the GUI. `read_capabilities()` rewrites the file only when the live string differs from the cached copy (`engine.rs` around line 633), so the live read matched the cache byte for byte. The content reports `model(ThinkVision P32p-20)` and `mccs_ver(2.2)`.

`settings.json` was unchanged (`a581cfc0…`). Its content compared with `store.rs`:
- `Hotkeys`: all 11 keys are present and equal to the defaults in `hotkey.rs`. The `\u002B` escapes come from the old .NET writer and parse fine.
- `Day` is {Brightness 0, Contrast 75, ColourMode "0x06"}. `Night` is {0, 20, "0x06"}. Both parse.
- `ThisMachineInput` "0x0F" (DP) matches the live input.
- `StartWithWindows` is false and `BackgroundMode` is true. Both parse.
- `AutoDayNight` is **missing**. It loads as Off with 07:00/19:00 defaults, as designed. The app never rewrote the file, so this 0.0.4-era file stays without the field until the user saves settings.
- Odd default worth noting: `ContrastUp = Ctrl+Alt+Left` and `ContrastDown = Ctrl+Alt+Right` (Left raises the value).

## 2. Issues found

| # | Severity | Issue | One-line fix |
|---|----------|-------|--------------|
| 1 | **High** | `--get` started while another copy runs deletes that copy's live crash marker. `main.rs` reads `marker.left_over()` and calls `marker.clear()` before running, and `--get` skips the single-instance check, so it reaches that code even when another copy is running. Reproduced: the marker vanished 31 ms after launch, instead of the normal 2.45 s. A real crash during that capability read would then not trigger safe mode on the next start. | Skip the marker check and clear in `main()` for `--get` (or whenever another instance holds the instance lock), or give each process its own marker file (for example, with the PID in the name). |
| 2 | **High** | The same race makes `--get` log a false `Safe mode: crashed during capability read` and run in safe mode. Values were still printed, but the log now claims a crash that never happened. | Same fix as #1. |
| 3 | **High** | Every malformed verb (`--get bogus`, `--get` without a value, `--step brightness=+500`, `--step volume=+5`, `--preset evening`, `--input vga`, unknown `--foo`, `thinkvision://bogus`, a bare word) silently opens the full GUI and never exits. With no copy running, a script hangs (killed by timeout, exit 124). With a copy running, it exits 0. There is no error text, and exit code 2 ("out of range") can never be produced from the command line. | Make `cli::parse` return an error variant for a malformed verb or an unknown `--flag`, then print the problem plus a usage hint and exit 2 (out-of-range step) or a new usage-error code; reserve Show for no arguments. |
| 4 | Medium | `--version` is not supported. It opens the GUI and hangs with no copy running; with a copy running it exits 0 with no output. | Add `--version` / `-V` that prints `ThinkVision Control <CARGO_PKG_VERSION>` and exits 0. |
| 5 | Medium | The 5 s headless safety timer is too tight under any DDC contention, and when it fires the user gets nothing. 8 parallel `--get` runs all hit exit 8 with no output; `--step contrast=+10` with 6 parallel reads hit exit 8 with no output, even with `--json`. A normal `--step` already takes 3.5 s, and one took 5.08 s. | Print a "timed out" line (or `{"error":"timed out"}`) before quitting, and raise the timer or start it after connect (for example, 5 s after the command is dispatched rather than 5 s after process start). |
| 6 | Medium (found in the code, not reproduced live) | `quit_if_headless` sets exit 8 only while `pending_cli` is non-empty. `pending_cli` is emptied as soon as the first Connected status arrives (`controller.rs` around line 619). So a step, preset or input whose result arrives after the 5 s timer exits **0 with no output**, possibly after the write was sent. This matches the reported "exit 0, no output after 5 s". `exit_code` starts at `Cell::new(0)`. | Track "result received" separately from the pending queue, and exit 8 when the timer fires before a result has arrived. |
| 7 | Medium | Model naming is hard-coded in the released 0.0.8. The header (`set_model_name(profile.display_name)`), the "… isn't connected" text and the tray tooltip (`profile.short_name`) all come from the embedded profile ("ThinkVision P32p-20" / "P32p-20"), not from the device. Device matching also depends on the profile's `edidId` "LEN62A2" and the hint "P32p-20". HEAD (`3913f5a`) already takes the header and messages from the capability string's `model(...)`, but the tray tooltip in HEAD still uses `header_name`, which falls back to generic text, so HEAD is fine; **0.0.8 is not**. | Release HEAD's `monitor_name()` change; keep `profile.display_name` only for the "Monitor profile:" footer. |
| 8 | Low | `--json` is silently ignored on forwarded verbs (exit 0, no output). This is documented for output but not for JSON. A script asking for JSON gets an empty string. | When `--json` is given, run the command headless on its own connection like `--get`, or print `{"forwarded":true}`. |
| 9 | Low | Log lines use Rust debug formatting: `Cli Some(16) None: Ok Brightness 10`, `Window None Some("Day")`. | Format as `CLI 0x10: Ok Brightness 10` / `Window preset Day: Ok`. |
| 10 | Low | The log records too little to diagnose these issues. There is nothing for `--get` runs, timeouts (exit 8), start/exit, the version or successful capability reads. Contrast writes have no "verified" line, unlike brightness. Timed-out processes log only "Read 0xNN failed: connection lost: shutting down". | Log one line per headless run (verb, exit code, duration), one line at start (version, mode), and a verify line for every Range write. |
| 11 | Low | `--get brightness` reads every Main and Advanced code (~1.4 s, about 15 DDC transactions) to return one value. This multiplies contention (#5). | For `--get <single>`, refresh only the requested code(s). |
| 12 | Info | `log.txt` growth is bounded: it rolls to `log.1.txt` past 1 MB (`log.rs`), so at most about 2 MB. `fs::rename` replaces an existing `log.1.txt` on Windows. Two processes can both see "> 1 MB" and race the rename, which is harmless. | None needed. |
| 13 | Info | A tray-only start (`--startup` with BackgroundMode on) skipped the live capability read in 2 of 3 runs, observed over 10 to 30 s. A window start always did it after about 3.7 s. The cache is current, so there is no visible effect today, but a changed capability string (for example, after a firmware update) would not be picked up until the window is opened. | Check that the `caps_requested` 2 s timer fires in tray-only mode; trigger the read from Connected regardless of window state. |
| 14 | Info | `settings.json` has no `AutoDayNight` (0.0.4-era file). It loads as Off and is never rewritten, as designed. The default contrast hotkeys map Left to "up". | Optional: swap the ContrastUp and ContrastDown defaults. |

Things that were checked and found working:
- The help wording is generic.
- Every read option returns correct values with exit 0.
- `--json` output is valid.
- Links (`thinkvision://get/all`, `thinkvision:get/brightness/`) work.
- Case-insensitive and `=` forms work.
- Help wins wherever it appears.
- Both brightness and contrast round trips return exit 0 and the readback matches.
- Forwarding to a running copy works (exit 0, about 120 ms).
- The capability cache matches the live read.
- `--get` during the GUI's capability read did not corrupt that read.

## 3. Restoration

- Backup taken before any test: `C:\Users\MrError\AppData\Local\Temp\claude\9446043aa4c45138\82604929-332d-47a5-aaa4-87d8df35c7cf\scratchpad\appdata-backup\`.
- The app created no new files: no `risky-operation.txt` left behind, no `log.1.txt`, no `settings.json.bak` and no `backup-*.json`. Only `log.txt` had grown, to 4233 bytes. All three originals were copied back with `cp -a`, which keeps their timestamps.

Final listing of `%APPDATA%\ThinkVisionControl`:
```
-rw-r--r-- 1 MrError 197121  443 2026-10-28 08:23:47.054283400 +0500 capabilities.txt
-rw-r--r-- 1 MrError 197121 2497 2026-11-09 17:45:46.931186500 +0500 log.txt
-rw-r--r-- 1 MrError 197121  824 2026-10-28 08:30:50.137653900 +0500 settings.json
6aa2c79b68bcfdab15d188c4405939c220061db7d4ae140d26df2b1fd1605344 *capabilities.txt   (matches backup)
ddece8dcf6ad5bbf63d234b3c58b08e92df7c77a420e00d5073b60d0dfaae3ee *log.txt            (matches backup)
a581cfc0e975f4df358e404f7367c78d2faf5977a93828091ad5a151fac4f475 *settings.json      (matches backup)
```
The folder's own mtime changed (17:48:01), from the marker file being created and deleted. Its contents are identical to the backup.

Monitor state:
- Originals: brightness 0, contrast 20.
- Final `--get all` (17:50, after the last write): `brightness: 0 / contrast: 20 / input: DisplayPort / colourMode: Neutral`, exit 0.
- The last contrast attempt (`+10` under load) timed out before writing, and the readback confirmed contrast stayed at 20.
- Only codes 0x10 and 0x12 were ever written. Input, power, colour mode, language and factory reset were never sent.
- No ThinkVision Control process is running, which matches the state at the start.
