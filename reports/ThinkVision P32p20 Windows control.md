# Control your ThinkVision P32p-20 from Windows

**Yes. Your P32p-20 can be controlled from Windows, and the read-only probe run on it today proves it.** The monitor (EDID `LEN62A2`) answered DDC/CI over the laptop's current HDMI connection. It returned a clean MCCS 2.2 capability string and readable values for brightness, contrast, colour preset, RGB gains, input source, scaling, OSD language and power mode ([local probe](../research_notes/ThinkVision%20P32p20%20Windows%20control/local_probe_raw.txt)). Windows 11 itself cannot do this: no build through 25H2 has a native slider or setting for external-monitor brightness, contrast or input ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/5954664/can-we-get-ddc-ci-support-for-monitors-on-windows)). Free tools fill the gap. **Microsoft PowerToys Power Display** (new in April 2026) gives you a tray flyout, profiles and a command-line tool. **NirSoft ControlMyMonitor** can write any VCP code from a one-line command you can bind to a hotkey. **Twinkle Tray** is a lighter alternative with built-in brightness hotkeys. My recommendation is Power Display for everyday clicking, plus ControlMyMonitor commands on hotkeys for one-press actions in the dark. Software can reach roughly half of the OSD menu. Dynamic Contrast, Over Drive, the USB settings, the USB upstream (KVM) switch and the menu-layout options have no standard code and stay behind the buttons unless Lenovo's vendor-specific codes can be decoded. Others have done this on sibling ThinkVision models, and one small Windows tool ships a P32p-20 brightness profile. However, nobody had published this monitor's capability string or input codes before your probe. That probe settles the USB-C input value as `0x31` (49).

## Your monitor answered: the capability string decoded

The probe opened both displays on the laptop. The built-in AUO panel gave no DDC/CI response, which is normal for an internal screen. The P32p-20 returned this capability string ([local probe](../research_notes/ThinkVision%20P32p20%20Windows%20control/local_probe_raw.txt)):

```
(prot(monitor)type(LCD)model(ThinkVision P32p-20)cmds(01 02 03 07 0C E3 F3)
vcp(02 04(4C 42 31 2E 32) 05 08 10 12 14(01 05 06 08 0B 0E 0F 12) 16 18 1A 52
60(11 0F 31) 86(02 05) AC AE B2 B6 C6(02 01 C3 00 A0 CA 00 F8 00 EA 00 00 DC 00 46 00 00)
C8 C9 CA(01 02) CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF E0(00 01 02)
EA(00 01) EB(00 01) EC(00 01) F6(07) F7(02 04 07 08 0D) F8(02 04 07 08 0D)
FA(10 12 EA 14))mswhql(1)asset_eep(40)mccs_ver(2.2))
```

Three details matter. First, **`60(11 0F 31)` gives the input codes: HDMI = `0x11` (17), DisplayPort = `0x0F` (15), USB-C = `0x31` (49)**, and the monitor currently reads `0x11` because the laptop is on HDMI. Before the probe, the evidence pointed two ways. The P40w-20 advertises `60(0F 11 31)` ([ddccontrol-db #445](https://github.com/ddccontrol/ddccontrol-db/issues/445)), a P40w-20 owner found Thunderbolt = 49 by trial and error ([BetterDisplay #1854](https://github.com/waydabber/BetterDisplay/issues/1854)), and a P24h-20 script uses `0x31` for USB-C ([gist](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)). The closer sibling P27h-20, however, advertises `60(0F 11 13)` ([ddccontrol-db #133](https://github.com/ddccontrol/ddccontrol-db/issues/133)). Your monitor's own string ends that debate in favour of 0x31. Generic tools may label 49 as "unknown" because MCCS defines no USB-C value and Dell, for example, uses `0x1B` ([ddcutil #70](https://github.com/rockowitz/ddcutil/issues/70)).

Second, the string is well-formed, unlike the P40w-20's duplicated `60(0F 11 31 FF 0F 11 31)` list. That matters because PowerToys warns that malformed capability strings can trigger a Windows kernel crash on a small number of monitors ([Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)). The probe already exercised the same Windows capability-read path on this laptop without incident. Third, the bytes attached to code `04` (`4C 42 31 2E 32`) decode as ASCII to **"LB1.2"**. That looks like the firmware version, which would be newer than the "LB0.3" shown in the user guide's sample screenshot ([P32p-20 User Guide](https://content.etilize.com/User-Manual/1063692945.pdf)). This is my reading of the bytes. The OSD footer will confirm it.

The probe also shows how the monitor is set right now. **Brightness is 0 of 100, contrast 70 of 100**, colour preset `0x06`, RGB gains all 100, scaling `0x02`, OSD language `0x02` (English), power mode `0x01` (on). Two codes that are *not* in the capability string still answered reads: `0x62` (audio volume, reads 50) and `0xDC` (display mode, reads 0, max 6). The monitor has no speakers, only a 3.5 mm audio-out ([PSREF spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.PDF)), so what `0x62` controls is unverified. The probe was read-only, so **no write has yet been tested on this unit**. The codes below are "confirmed" in the sense that the monitor advertises them and returned sane values.

## Eight OSD menu items map to confirmed VCP codes; the USB and tuning options do not

The P32p-20 OSD has four groups: Monitor Controls, Advanced Settings, Port Settings and Menu Settings ([P32p-20 User Guide, Table 2-2](https://content.etilize.com/User-Manual/1063692945.pdf)). Lining them up against the probe gives this map. "Confirmed on your monitor" means the code is in your capability string and, where readable, returned a valid value today. Write behaviour still needs a first test.

| OSD function | VCP code | Values on your P32p-20 | Current | Status |
|---|---|---|---|---|
| Brightness | `0x10` | 0–100 | 0 | **Confirmed on your monitor** |
| Contrast | `0x12` | 0–100 | 70 | **Confirmed on your monitor** |
| Color Mode (sRGB, BT.709, DCI-P3, Neutral, Reddish, Bluish, Low Blue Light, Custom) | `0x14` | `01 05 06 08 0B 0E 0F 12` | `06` | **Confirmed on your monitor**. Eight values for eight modes. `01` = sRGB and `0B` = User 1/Custom by MCCS convention; the other six assignments are unverified |
| Custom Red / Green / Blue | `0x16` / `0x18` / `0x1A` | 0–100 | 100 / 100 / 100 | **Confirmed on your monitor** |
| Input Signal (USB-C / DP / HDMI) | `0x60` | `0x11` HDMI (17), `0x0F` DP (15), `0x31` USB-C (49) | `0x11` | **Confirmed on your monitor** |
| Aspect Ratio (Full Screen / Original AR) | `0x86` | `02`, `05` | `02` | **Confirmed on your monitor**. Which value is which option is unverified |
| Language (8 languages) | `0xCC` | `02 03 04 05 06 09 0A 0D` (English, French, German, Italian, Japanese, Russian, Spanish, Simplified Chinese per the MCCS table) | `02` | **Confirmed on your monitor** |
| Power (on / standby / off) | `0xD6` | `01` on, `04` off/standby, `05` off | `01` | **Confirmed on your monitor**. See the warnings section |
| Factory Reset | `0x04` (all), `0x05` (brightness + contrast), `0x08` (colour) | write 1 | n/a (write-only) | **Advertised on your monitor**, not tested |
| Display mode | `0xDC` | reads 0, max 6 | 0 | Readable but not advertised. Meaning unknown |
| Audio volume | `0x62` | reads 50 | 50 | Readable but not advertised. Effect unknown (no speakers) |
| Dynamic Contrast (Off/On) | none standard | candidates: `EA`/`EB`/`EC` (`00 01`) | — | **OSD-only / unknown** |
| Over Drive (Off/Normal/Extreme) | none standard | candidate: `E0` (`00 01 02`, three values like the three OSD options) | — | **OSD-only / unknown** |
| USB Charging, DP Select (1.1/1.2), USB Source Selection (2.0/3.0) | none standard | candidates among `EA`/`EB`/`EC`, `F6`, `F7`, `F8`, `FA` | — | **OSD-only / unknown** |
| USB upstream (KVM) switch | none standard | unknown | — | **OSD/hotkey-only / unknown** |
| Button Repeat Rate, Menu Time Out, Menu H/V position, Transparency | none | — | — | **OSD-only** |
| OSD lock | none (Menu key, 10 s hold) | `CA(01 02)` is advertised; in MCCS this is the OSD enable/disable control | — | **OSD-only**. Do not write `CA` |
| DDC/CI on/off | none (Exit key, 10 s hold) | — | — | **Physical button only** |

The vendor codes (`E0`, `EA`, `EB`, `EC`, `F6`, `F7`, `F8`, `FA`) are the only route to the rest of the menu, and nobody has published their meaning for any ThinkVision. The P40w-20 exposes a similar set with no documentation ([ddccontrol-db #445](https://github.com/ddccontrol/ddccontrol-db/issues/445)). You could map them safely once without writing anything. Open ControlMyMonitor's GUI, note the value of each vendor code, change one OSD setting (for example Over Drive) with the buttons, refresh, and see which code moved. Writing guessed values to unknown codes is the riskier half of that experiment and is not worth doing blind. One caveat from the siblings applies to brightness: P24q/h-20 owners report that brightness is locked in the factory-calibrated sRGB, DCI-P3 and BT.709 modes ([Lenovo forum: P24q-20](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178)). If a brightness write is ignored, check the colour preset first.

## Windows has no native control, but three free tools cover the gap

Microsoft's own position, given by a moderator in July 2026, is that "Windows 11 does NOT include universal built-in brightness control for external DDC/CI-capable monitors", and users must rely on third-party or vendor software ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/5954664/can-we-get-ddc-ci-support-for-monitors-on-windows)). The Settings and Quick Settings slider drives only the panel the OEM tagged as internal, and 24H2 and 25H2 shipped without changing that ([XDA Developers](https://www.xda-developers.com/powertoys-monitor-brightness-windows-still-cant/)). Your laptop's Fn brightness keys will therefore keep changing only the laptop screen. WMI brightness methods share the same limit. The closest thing to a first-party answer is a separate install from Microsoft rather than part of Windows.

| Tool | Cost / licence | What it covers on the P32p-20 | Scripting | Maintenance (Oct 2026) |
|---|---|---|---|---|
| **PowerToys Power Display** | Free, open source | Brightness, contrast, input, colour preset, power state, profiles, Light Switch (dark/light mode) profiles, custom input names, VCP capability viewer | `PowerToys.PowerDisplay.Cli.exe` | New in v0.99.0, 28 Apr 2026 ([release](https://github.com/microsoft/PowerToys/releases/tag/v0.99.0)) |
| **NirSoft ControlMyMonitor** | Freeware, portable | Every VCP code, read and write, including scaling, language, RGB gains, resets and vendor codes | Full CLI, no background service | v1.43 ([NirSoft](https://www.nirsoft.net/utils/control_my_monitor.html)) |
| **Twinkle Tray** | Free, MIT | Brightness (per monitor or linked), schedules, idle dimming, brightness hotkeys, input switching in the flyout, any VCP via CLI | CLI (app must be running) | Last stable v1.17.2, Dec 2023; v1.18 betas in 2024 ([releases](https://github.com/xanderfrangos/twinkle-tray/releases)) |
| Monitorian | Free core, MIT | Brightness and contrast only; no input switching | Hotkeys and CLI are paid add-ons | v4.16, Sept 2026 ([GitHub](https://github.com/emoacht/Monitorian)) |
| DisplayBuddy | $18.99 | Brightness, contrast, input, presets, schedules, hotkeys | Hotkeys | Commercial ([DisplayBuddy](https://displaybuddy.app/monitor-brightness-control)) |
| Lenovo Display Control Center – ThinkColour | Free | Officially lists the P32p-20; WMI methods for colour gamut, input and power | WMI `root\Lenovo\Lenovo_Display` | Frozen at 3.0.32161.0, Nov 2024 ([Lenovo DS547223](https://support.lenovo.com/us/en/downloads/ds547223-lenovo-display-control-center-thinkcolor)) |

Power Display's controls follow whatever the monitor reports over DDC/CI. **Its input-source, colour-temperature and power-state controls are off by default** and ask for confirmation when you turn them on. Its "max compatibility mode" probes every code and can blank the screen or change settings, so leave it off: your capability string is clean and does not need it ([Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)). Its one gap for your use case is that it has no direct brightness-up/down hotkeys yet, only a shortcut that opens the flyout ([PowerToys #48784](https://github.com/microsoft/PowerToys/issues/48784)). ControlMyMonitor fills that gap.

Lenovo's own route is weaker than it looks. The P32p-20 datasheet lists LDCC – ThinkColour support ([PSREF datasheet](https://psref.lenovo.com/syspool/Sys/PDF/datasheet/ThinkVision%20P32p-20_datasheet_EN.pdf)), and LDCC's WMI interface exposes `SetInputSource`, `SetPowerMode` and `SetColorGamut` ([LDCC WMI User Guide](https://download.lenovo.com/consumer/monitor/ldcc_wmi_user_guide.pdf)). However, the probe found that namespace **not installed on your laptop**, LDCC has not been updated since November 2024, and its successor, Lenovo Accessories and Display Manager, omits the P32p-20 (machine type 62A2) from its April 2026 support list ([LADM support list](https://download.lenovo.com/consumer/options/ladm_support_product_list_20260401.xlsx)). LDCC also talks DDC/CI underneath, so it cannot reach anything the open tools cannot unless Lenovo wired the vendor codes into it. Nobody has reported whether it does for this model. It is worth one install from the [Microsoft Store](https://www.microsoft.com/store/apps/9N6PGT5DKQFQ) only if you want to see whether it exposes Over Drive or the USB settings.

## Others have done this on siblings; your probe is the first P32p-20 map

Public prior art for this exact monitor is thin but real. The open-source Windows tool `magcode/mqtt-tools` ("moco") adjusts monitor brightness from an MQTT light sensor over DDC/CI, and its sample config includes a tuning entry for `"Lenovo P32p-20"` ([moco config](https://github.com/magcode/mqtt-tools/blob/3538780a6364887c745c2a70834b9c190803a49e/moco/configuration-sample.json)). DisplayBuddy lists "LEN P32p-20" among the Lenovo monitors it has tested for Windows brightness control. That is vendor marketing rather than an independent report ([DisplayBuddy: Lenovo on Windows](https://displaybuddy.app/control-lenovo-monitor-brightness-on-windows)). Sibling work fills in the rest. There are full capability dumps for the P27h-20 and P40w-20 ([ddccontrol-db #133](https://github.com/ddccontrol/ddccontrol-db/issues/133), [#445](https://github.com/ddccontrol/ddccontrol-db/issues/445)) and a P24h-20 input-switching "DIY KVM" script ([gist](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)). On a P24q-20 with dead OSD buttons, Lenovo support's accepted answer was to install Display Control Center, though the owner never confirmed it worked ([Lenovo forum](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178)). A GitHub code search found no AutoHotkey, PowerShell or ControlMyMonitor scripts naming ThinkVision or `LEN62A2`, and no public P32p-20 capability string ([linuxhw/EDID LEN62A2](https://github.com/linuxhw/EDID/tree/master/Digital/Lenovo/LEN62A2)). Reddit and SuperUser could not be searched, so a forum post there may exist unseen.

The failing buttons deserve one check before you write them off. On sibling ThinkVisions, "dead" OSD buttons often turned out to be an OSD lock (10-second Menu hold), navigation disabled because no live signal reached the expected input, or static that a power drain cleared ([Lenovo forum: P27Q-20](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/ThinkVision-P27Q-20-monitor-OSD-navigation-controls-not-working/m-p/5214709)). Lenovo also has a support note that ThinkVision OSD controls "may not respond if pressed too quickly" ([Lenovo HT514070](https://support.lenovo.com/us/en/solutions/ht514070-monitor-osd-controls-may-not-respond-if-pressed-too-quickly-thinkvision)). Unplugging everything and holding power for 15–30 seconds costs nothing.

## Recommended setup: Power Display for the flyout, ControlMyMonitor on hotkeys

The primary path for you is **PowerToys Power Display as the everyday interface, plus ControlMyMonitor commands bound to hotkeys**. Power Display is Microsoft-maintained, covers brightness, contrast, input, colour preset and power in one flyout, and its Light Switch integration can apply a dark-mode profile automatically, which suits a monitor you use in the dark. ControlMyMonitor is a portable exe with no service to keep running. It reaches every code in your table, so a single keypress can set a night or day brightness without opening any UI. If you would rather have one app with built-in brightness hotkeys and time-of-day schedules, Twinkle Tray is the alternative, with the caveat that its last stable release is from 2023.

### ControlMyMonitor (scripts and hotkeys)

ControlMyMonitor takes the VCP code in hex and the value in decimal. It identifies a monitor by its Windows device path (`\\.\DISPLAY2\Monitor0`), its name, its serial number or its short monitor ID such as `LEN62A2` ([NirSoft](https://www.nirsoft.net/utils/control_my_monitor.html)). The short ID is the most robust choice because the `DISPLAYx` number can change when you re-plug or dock. First list the monitors and back up the current settings:

```bat
ControlMyMonitor.exe /smonitors monitors.txt
ControlMyMonitor.exe /SaveConfig "%USERPROFILE%\p32p20-backup.cfg" "LEN62A2"
ControlMyMonitor.exe /scomma p32p20-vcp.csv "LEN62A2"
```

Then use the confirmed codes. If you prefer the device path, replace `"LEN62A2"` with the `\\.\DISPLAYx\Monitor0` string from `monitors.txt`, for example `/SetValue "\\.\DISPLAY2\Monitor0" 60 15`.

```bat
:: Brightness (0x10) and contrast (0x12)
ControlMyMonitor.exe /SetValue "LEN62A2" 10 0          :: night: brightness 0 (your current value)
ControlMyMonitor.exe /SetValue "LEN62A2" 10 60         :: day: brightness 60
ControlMyMonitor.exe /ChangeValue "LEN62A2" 10 10      :: brightness +10
ControlMyMonitor.exe /ChangeValue "LEN62A2" 10 -10     :: brightness -10
ControlMyMonitor.exe /SetValue "LEN62A2" 12 70         :: contrast 70

:: Input source (0x60) - read the warnings below first
ControlMyMonitor.exe /SetValue "LEN62A2" 60 17         :: HDMI       (0x11)
ControlMyMonitor.exe /SetValue "LEN62A2" 60 15         :: DisplayPort (0x0F)
ControlMyMonitor.exe /SetValue "LEN62A2" 60 49         :: USB-C      (0x31)

:: Colour preset (0x14), RGB gains (0x16/0x18/0x1A), scaling (0x86), language (0xCC)
ControlMyMonitor.exe /SetValue "LEN62A2" 14 1          :: sRGB
ControlMyMonitor.exe /SetValue "LEN62A2" 14 11         :: 0x0B = User/Custom (then RGB gains apply)
ControlMyMonitor.exe /SetValue "LEN62A2" 16 95         :: red gain 95
ControlMyMonitor.exe /SetValue "LEN62A2" 86 5          :: the other aspect-ratio option (current is 2)
ControlMyMonitor.exe /SetValue "LEN62A2" CC 2          :: OSD language English

:: Read a value (returned as the exit code)
ControlMyMonitor.exe /GetValue "LEN62A2" 10
echo %errorlevel%
```

In PowerShell, call it as `& "C:\Tools\ControlMyMonitor\ControlMyMonitor.exe" /SetValue "LEN62A2" 10 30` and read `/GetValue` results from `$LASTEXITCODE`. To map the six unverified colour presets, step through `14 5`, `14 6`, `14 8`, `14 14`, `14 15` and `14 18` (decimal for `0x05`, `0x06`, `0x08`, `0x0E`, `0x0F`, `0x12`) and note which OSD mode name appears. Your current mode is `0x06`, so return to `14 6` afterwards. For one-key control, an AutoHotkey v2 script can wrap these commands. This one uses Ctrl+Alt+arrows for brightness steps and Ctrl+Alt+N/D for night and day presets:

```autohotkey
#Requires AutoHotkey v2.0
cmm := "C:\Tools\ControlMyMonitor\ControlMyMonitor.exe"
^!Up::   Run(cmm ' /ChangeValue "LEN62A2" 10 10',  , "Hide")
^!Down:: Run(cmm ' /ChangeValue "LEN62A2" 10 -10', , "Hide")
^!n::    Run(cmm ' /SetValue "LEN62A2" 10 0',      , "Hide")
^!d::    Run(cmm ' /SetValue "LEN62A2" 10 60',     , "Hide")
```

### PowerToys Power Display CLI

Install PowerToys (`winget install Microsoft.PowerToys`), turn on Power Display, and in its settings enable the input-source, colour-temperature and power-state controls you want. The CLI needs Power Display running and is in the PowerToys install folder ([Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)). Run `list` first to find which number the P32p-20 gets; the examples assume 1.

```bat
PowerToys.PowerDisplay.Cli.exe list
PowerToys.PowerDisplay.Cli.exe capabilities --monitor-number 1 --setting input-source
PowerToys.PowerDisplay.Cli.exe get --monitor-number 1 --setting brightness
PowerToys.PowerDisplay.Cli.exe set --monitor-number 1 --brightness 30
PowerToys.PowerDisplay.Cli.exe set --monitor-number 1 --contrast 70
PowerToys.PowerDisplay.Cli.exe up --monitor-number 1 --brightness --step 10
PowerToys.PowerDisplay.Cli.exe down --monitor-number 1 --brightness --step 10
PowerToys.PowerDisplay.Cli.exe set --monitor-number 1 --color-temperature 0x01
PowerToys.PowerDisplay.Cli.exe set --monitor-number 1 --input-source 0x0F
PowerToys.PowerDisplay.Cli.exe set --monitor-number 1 --input-source 0x31
PowerToys.PowerDisplay.Cli.exe profiles
PowerToys.PowerDisplay.Cli.exe apply-profile <id>
```

In Power Display's settings, give the input values friendly names (`0x11` HDMI, `0x0F` DisplayPort, `0x31` USB-C) using its custom VCP name mapping, so the flyout shows readable names instead of "unknown" for USB-C.

### Twinkle Tray CLI

Install with `winget install xanderfrangos.twinkletray`. The CLI talks to the running app, and `--VCP` writes any code ([Twinkle Tray README](https://github.com/xanderfrangos/twinkle-tray)). The executable is normally under `%LocalAppData%\Programs\twinkle-tray\`. Use `--List` to get the monitor number.

```bat
"Twinkle Tray.exe" --List
"Twinkle Tray.exe" --MonitorNum=1 --Set=30 --Overlay
"Twinkle Tray.exe" --MonitorNum=1 --Offset=-10 --Overlay
"Twinkle Tray.exe" --MonitorNum=1 --VCP="0x12:70"
"Twinkle Tray.exe" --MonitorNum=1 --VCP="0x60:15"
"Twinkle Tray.exe" --MonitorNum=1 --VCP="0x60:49"
```

## Four ways to lock yourself out, and how to avoid them

**Switching input away from the laptop usually ends software control.** Your laptop feeds HDMI. Writing `60 15` (DP) or `60 49` (USB-C) shows whatever is on that port, or black if nothing is. Microsoft's own documentation warns that "software control may be unavailable until you change inputs again using the monitor's physical buttons" ([Microsoft Learn: Power Display](https://learn.microsoft.com/en-us/windows/powertoys/power-display)). Some monitors keep accepting `0x60` from a non-active host and others do not ([ddcutil #100](https://github.com/rockowitz/ddcutil/issues/100)); nobody has tested the P32p-20. If you run a second machine on DP or USB-C, put a "switch to me" command on *each* machine (for example `/SetValue "LEN62A2" 60 17` on whatever needs to bring HDMI back). A toggle on one machine can switch away but not back. A switch to an input with no live signal can also be reverted by the monitor's auto-scan ([ddcutil #236](https://github.com/rockowitz/ddcutil/issues/236)). The physical fallback is the Exit key, whose short press is also Input Change ([P32p-20 User Guide](https://content.etilize.com/User-Manual/1063692945.pdf)).

**Power-off via `0xD6` may need the physical power button to undo.** Values `04` and `05` blank the monitor. Twinkle Tray users report DDC/CI monitors powered off this way that "sometimes cannot wake (requires hard power-cycle)" ([Twinkle Tray #1213](https://github.com/xanderfrangos/twinkle-tray/issues/1213)). A P32p-20 owner also reports the monitor never waking from standby over USB-C until it is power-cycled ([Lenovo forum: P32p-20 firmware](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/T32p-20-and-P32p-20-firmware-updates/m-p/5155042)). Test `D6 1` recovery once while you can reach the power button, before relying on it. Power Display requires `--confirm-power-off` for this reason.

**A 10-second hold on the Exit key toggles DDC/CI off, with no menu entry to turn it back on.** The screen shows "DDC/CI disable" ([P32p-20 User Guide](https://content.etilize.com/User-Manual/1063692945.pdf)). With a failing button that bounces or sticks, an accidental long press is a real risk, and owners of other Lenovo monitors have reported "DDC/CI enable/disable" toasts flashing on their own ([Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/3764812/ddc-ci-enable-disable-continiously-blinking-on-off)). If every tool suddenly stops responding, hold Exit for 10 seconds again before debugging anything else. The same holds for Menu, whose 10-second hold locks the OSD. Leave `0xCA` alone, since writing it could disable the on-screen menu you still need as a fallback.

**Avoid factory reset (`0x04`) unless you mean it.** It resets every OSD setting, and DisplayBuddy reports that a factory reset can switch DDC/CI off on Lenovo models ([DisplayBuddy](https://displaybuddy.app/control-lenovo-monitor-brightness-on-windows)). That would cut off software control and leave you dependent on the failing buttons to restore it. The narrower `0x05` (brightness and contrast only) and `0x08` (colour only) do less damage, and your `/SaveConfig` backup restores everything else with `/LoadConfig`. Two smaller hazards: Monitorian users have seen controls vanish after sleep until the app restarts ([Monitorian #263](https://github.com/emoacht/Monitorian/issues/263)), and DisplayLink adapters and MST docks often block DDC ([ddcutil #146](https://github.com/rockowitz/ddcutil/issues/146)). Your direct HDMI link is the reliable path.

## Conclusion

The failing buttons turn out to matter less than they seemed. Everything you adjust often (brightness, contrast, input, colour mode) is software-reachable today, with codes read from your own monitor rather than guessed from siblings. The buttons now matter for exactly three things: recovering from an input switch away from the laptop, waking from a DDC power-off, and the DDC/CI toggle itself. That last one is the single point of failure for the whole software setup. The practical priority is to protect the buttons from accidental long presses, not to use them for daily adjustments.

The open question is the vendor block. Eight undocumented codes (`E0`, `EA`–`EC`, `F6`–`F8`, `FA`) are the likeliest home for Over Drive, Dynamic Contrast and the USB settings, and possibly the USB upstream (KVM) switch. Decoding them with the observe-only method above would move the rest of the OSD into software, and would be the first public map of these codes for any ThinkVision. Lenovo's own software is unlikely to do it for you, because it has stopped listing this model.
