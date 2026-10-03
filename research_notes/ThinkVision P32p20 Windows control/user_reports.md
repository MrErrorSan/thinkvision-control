# Real-world user reports: controlling ThinkVision P32p-20 (and siblings) from a PC, and working around failing OSD buttons

Research date: 2026-10-03. Scope: user reports, GitHub issues, gists, forum threads. Each finding is labelled **[P32p-20]** when it is about the P32p-20 itself, or with the sibling or other model it concerns.

Method notes and source-access limits (these matter for how far to trust the gaps below):
- **Reddit could not be searched.** The search tool's crawler is blocked from reddit.com, and the Reddit JSON API returned non-JSON or required a login. There are no Reddit findings in these notes. Reddit is the biggest unsearched source.
- **SuperUser/StackExchange** are also blocked for the search tool's user agent. No SE answers were reviewed directly.
- **Lenovo Community forums** (forums.lenovo.com) render with JavaScript and are now an archived, read-only community. Threads were read by rendering them in a headless browser. Forum search does not work in the archive, so threads were found through web search, and coverage is partial.
- **GitHub** was searched directly with `gh search issues` and `gh search code` across Monitorian, twinkle-tray, ddcutil, ddccontrol-db, MonitorControl, BetterDisplay and display-switch, plus global searches for each model name.
- Walmart P32p-20 reviews were blocked by a CAPTCHA, and Lenovo support article pages returned 403.

---

## Q1. Has anyone controlled a P32p-20 (or close sibling) brightness/contrast/input from Windows? What tool, and did it work?

### Takeaway
Direct, public P32p-20 reports are rare. The clearest one is a small open-source Windows tool (`magcode/mqtt-tools` "moco"). Its sample config has an entry tuned for "Lenovo P32p-20", and it drives brightness through the Python `screen_brightness_control` library over DDC/CI. A commercial vendor page also lists the P32p-20 as tested for brightness. For siblings, capability dumps show standard MCCS brightness/contrast/input codes (P27h-20, P40w-20), and P40w-20 input switching was done by trial and error.

### Cited Findings
- **[P32p-20]** `magcode/mqtt-tools` → `moco` is "a simple tool to control the brightness of your monitor(s) attached to a Windows machine based on a MQTT-connected light sensor". Its `configuration-sample.json` has a per-monitor entry `"Lenovo P32p-20": {"factor": 0.25, "offset": -30}` next to a `default` and `HP1`. The tool calls `sbc.set_brightness(val, display=monitor)` from the `screen_brightness_control` Python package, which uses DDC/CI (VCP 0x10) on external monitors in Windows. The README says it was tested with Python 3.12.8. The repo was last pushed 2025-06-09. — [magcode/mqtt-tools moco config](https://github.com/magcode/mqtt-tools/blob/3538780a6364887c745c2a70834b9c190803a49e/moco/configuration-sample.json); [moco README](https://github.com/magcode/mqtt-tools/tree/main/moco)
- **[P32p-20]** DisplayBuddy, a commercial DDC app, lists "LEN P32p-20" and "P27h-20" among Lenovo monitors it tested for brightness control on Windows. It says DDC/CI is on by default on most Lenovo models but a factory reset can switch it off, and that it sits under "Settings → Other → DDC/CI". *This is vendor marketing, not an independent user report.* — [DisplayBuddy: Lenovo brightness on Windows](https://displaybuddy.app/control-lenovo-monitor-brightness-on-windows)
- **[P32p-20, macOS/Linux context]** The P32p-20 shows up by name in other people's display-automation configs. A Hammerspoon config names `workDisplay = "LEN P32p-20"`, and a sway/xrandr layout script logs "LENOVO P32p-20". Both are layout automation, not DDC. — [lepinkainen/dotfiles hammerspoon config](https://github.com/lepinkainen/dotfiles/blob/6bedf96db888f7a5bfdccc4b53d8e97d9240fb6f/dot_hammerspoon/config.lua); [damien-talos/dotfiles screen_layout_ideal.sh](https://github.com/damien-talos/dotfiles/blob/2c37ab3dbb3a661e6df7b89f0edcbb3f22d3e311/bin/screen_layout_ideal.sh)
- **[P27h-20, Linux, 2020-10-20]** A full `ddccontrol -p -c -d` dump of a P27h-20 (PnP ID `LEN61E9`) shows "DDC/CI supported: Yes". Brightness 0x10 read 60/100 and contrast 0x12 read 75/100. The capability string is `(prot(monitor)type(LCD)model(P27h-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(01 05 06 08 0B 0C 0D) 16 18 1A 52 60(0F 11 13) AC AE B2 B6 C6 C8 CA CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF FD)mswhql(1)asset_eep(40)mccs_ver(2.2))`. The probe also hit some malformed replies ("Invalid response, first byte is 0x00, should be 0x6e"; "length is 9, should be 8 at most") on some codes. — [ddccontrol-db #133 Lenovo P27h-20](https://github.com/ddccontrol/ddccontrol-db/issues/133)
- **[P40w-20, Linux, 2026-09-24]** A ddccontrol 3.3.0 dump of a P40w-20 (PnP `LEN62C1`) gives the capability string `model(ThinkVision P40w-20) ... vcp(02 04 05 08 10 12 14(01 05 06 08 0B 0E 0F) 16 18 1A 52 60(0F 11 31 FF 0F 11 31) 86(02 05) ... D6(01 05) ... mccs_ver(2.2))`. Brightness read 90/100 and contrast 75/100. — [ddccontrol-db #445 Lenovo P40w-20](https://github.com/ddccontrol/ddccontrol-db/issues/445)
- **[P40w-20, macOS, 2023-05-23]** A user found "through trial and error" that the Thunderbolt input's DDC identifier is **49 (0x31)**. BetterDisplay's custom input list only allowed 1–31, so the developer added 32 more entries the same day and labelled 49 "USB-C / TB 4". — [BetterDisplay #1854](https://github.com/waydabber/BetterDisplay/issues/1854)
- **[P24q-20 owner, Lenovo forum, 2021-09]** When a user's OSD navigation buttons failed, a Lenovo support specialist pointed them to Lenovo's "Display Control Center" download as the accepted "solution". The user's reply is not shown as confirming it worked. — [Lenovo forum: P24q-20 OSD buttons no response](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178)

### Inferences
- The P32p-20 almost certainly answers standard MCCS brightness/contrast (0x10/0x12) over DDC/CI from Windows. A Windows DDC tool has a P32p-20-specific tuning entry, and the same-generation P27h-20 and P40w-20 dumps expose the standard codes. No one, however, has posted a P32p-20 capability string publicly.
- Monitorian, Twinkle Tray, ControlMyMonitor and `screen_brightness_control` all use the same Windows `dxva2` high-level/low-level monitor API. A monitor that works with one should generally work with the others.

### Gaps
- No public P32p-20 `capabilities` string (ddcutil/ControlMyMonitor/ddccontrol) was found on GitHub or in the ddccontrol-db issue tracker. Reddit and SuperUser, the most likely places for one, could not be searched.
- No user report of contrast or input switching on the P32p-20 specifically was found.

---

## Q2. Input switching (VCP 0x60) on ThinkVision: which values work for HDMI / DP / USB-C?

### Takeaway
On ThinkVision monitors, DP = **0x0F (15)** and HDMI1 = **0x11 (17)**, consistent with MCCS. The USB-C/Thunderbolt value is non-standard. Two different Lenovo models report **0x31 (49)**. The P27h-20, the closest sibling to the P32p-20 in generation and ports, advertises **0x13 (19)** as its third input in its capability string, which suggests (but does not prove) that 0x13 is its USB-C value. Nobody has published confirmed P32p-20 values.

### Cited Findings
- **[P40w-20]** Capability `60(0F 11 31 ...)`. The user confirmed by trial and error that Thunderbolt = 49 (0x31). — [ddccontrol-db #445](https://github.com/ddccontrol/ddccontrol-db/issues/445); [BetterDisplay #1854](https://github.com/waydabber/BetterDisplay/issues/1854)
- **[P24h-20]** A published cross-platform KVM gist uses `0x11` (17) for HDMI and **`0x31` (49) for USB-C** on a "Lenovo ThinkVision P24h-20", driven by ddcutil on Linux and the BetterDisplay CLI on macOS. The author also notes that "The M3 Pro's native HDMI port is DDC-blind", so the Mac side uses a USB-C connection. — [gist balantrapu87: DDC/CI monitor switching](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)
- **[P27h-20]** The capability string advertises `60(0F 11 13)`, which is three inputs. The P27h-20 has DP-in, HDMI and USB-C inputs (plus DP-out). ddccontrol read the current input as 15 (0x0F, DP) with a nonsensical "max 3". — [ddccontrol-db #133](https://github.com/ddccontrol/ddccontrol-db/issues/133)
- **General:** USB-C has no standard MCCS 0x60 value. Dell uses 0x1B, and the ddcutil maintainer said in 2018 he would "be surprised if there is cross-manufacturer agreement on the use of this code". — [ddcutil #70](https://github.com/rockowitz/ddcutil/issues/70)
- **General (ControlMyMonitor guide):** "the input value for VCP code 'Input Select' is … a different value for each monitor, so you must look it up." — [note.com: ControlMyMonitor input switching](https://note.com/derakov/n/n8b79cb6bcbf3?hl=en)
- **[P24q-10 + daisy chain, 2022-09]** Switching inputs over DDC on a ThinkVision P24q-10 worked. When that monitor was daisy-chaining a second monitor, though, a DDC input switch dropped the downstream monitor and the new PC never detected it. Switching with the OSD worked fine. Another Windows tool behaved the same way, and the display-switch maintainer blamed monitor firmware. — [haimgel/display-switch #113](https://github.com/haimgel/display-switch/issues/113)
- **[P27h-20, Lenovo forum, 2021]** A Lenovo moderator said a P27h-20 "will not switch to the USB-C from the DP, if the DP is already connected to the other computer. You'll need a physical KVM switch". This is about automatic or daisy-chain behaviour, not DDC, and it is the moderator's belief rather than a test. — [Lenovo forum: P27h-20 and daisy-chaining](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/ThinkVision-P27h-20-and-daisy-chaining/m-p/5083133)
- **[P32u-10, Lenovo forum, 2021–2022]** On another ThinkVision, the hardware "One Key Switch" behaved inconsistently between two identical monitors. One user said "The only way I have found to switch reliably between the inputs are via the input-section of the menu". — [Lenovo forum: Configure One Key Switch on P32u-10](https://forums.lenovo.com/t5/General-Discussion/Configure-One-Key-Switch-on-P32u-10-Thinkvision/m-p/5061140)

### Inferences
- For a P32p-20 the candidate values are DP = 0x0F, HDMI = 0x11, and USB-C = **0x13 or 0x31**. Read the monitor's own capability string (`ControlMyMonitor.exe /scomma` or the Monitorian/ClickMonitorDDC capability view) before scripting. Writing an unlisted value to 0x60 is usually ignored, but on some scalers it can leave the monitor on an input with no signal.
- With DP-out daisy-chain (MST) enabled, a DDC input switch may disturb the downstream monitor (P24q-10 precedent).

### Gaps
- No report confirms which 0x60 values the P32p-20 accepts, or whether its firmware honours 0x60 writes at all.
- No report confirms whether 0x13 on the P27h-20 really is USB-C (only the capability string suggests it).

---

## Q3. Reports where DDC/CI failed on ThinkVision (USB-C/Thunderbolt docks, DisplayLink, MST daisy-chain, sleep) and the fixes

### Takeaway
Most DDC failures in user reports come from the path, not the monitor. Docks (especially DP-MST hubs), USB-C-to-HDMI adapters and some Mac ports block or mangle DDC, and the standard fix is a direct cable. On Windows, values or capabilities can also go missing after sleep, and the fix is to restart or rescan the app. The P32p-20 itself has a credible user report of **not waking from standby over USB-C** on stock firmware, and Lenovo offers no firmware updates for it.

### Cited Findings
- **[P32p-20, 2022-12-08]** An owner reported "my monitor never wakes up from standby when it receives a video signal (computer wakes up) from the USB-C port. I have to turn my monitor off and back on for the image to appear". They also said there are no P32p-20 firmware updates on the support site and that "I have seen monitors in my office with older versions of the firmware than mine". Another poster called T32p-20 firmware "LB1.0" "especially buggy" and said "The P32p-20 firmware seems slightly more usable". Lenovo staff only pointed to the support site. — [Lenovo forum: T32p-20 and P32p-20 firmware updates](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/T32p-20-and-P32p-20-firmware-updates/m-p/5155042)
- **[P32p-20, 2022-02 / 2024-02]** P32p-20 owners reported the USB hub (downstream USB-A ports) not working over the bundled USB-C cable. Lenovo staff suggested installing the USB and monitor drivers, but a later user found "no products were found" at the link. The thread shows no resolution. *Relevance: the P32p-20 OSD has a "DDC-CI / USB UFP" related setting, and USB upstream selection may interact with this.* — [Lenovo forum: USB ports on P32p-20 not working](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/USB-ports-on-Lenovo-P32p-20-display-are-not-working/m-p/5128614); OSD item reference: [P32p-20 User Guide (62A2)](https://content.etilize.com/User-Manual/1063692945.pdf)
- **[Lenovo USB-C Dock Gen 2, Linux, 2020-10]** DDC failed through a Lenovo USB-C Gen 2 dock (DP) but worked over direct HDMI. The ddcutil maintainer said "i2c failure on recent docking stations (i.e. using DisplayPort MST) is a well known problem". — [ddcutil #146](https://github.com/rockowitz/ddcutil/issues/146)
- **[ThinkPad Ultra Dock 40AJ, Linux, 2021-02]** Through a dock, one of two identical monitors showed "Invalid display", and phantom I2C buses appeared. The ddcutil maintainer tied this to DRM driver MST I2C handling, worked around it in ddcutil 1.0.x, and said "the same monitor can appear as 2 different /dev/i2c devices, only 1 of which supports DDC". — [ddcutil #181](https://github.com/rockowitz/ddcutil/issues/181)
- **[Lenovo Creator Extreme (not ThinkVision P), macOS, 2021-08]** Brightness worked through a USB-C-to-HDMI adapter but not over a direct USB-C cable. The MonitorControl maintainer said "Lenovo displays tend to have an option to enable or disable DDC, some even by port", and that HDR mode can block brightness changes. — [MonitorControl #514](https://github.com/MonitorControl/MonitorControl/issues/514)
- **[Generic Windows, Monitorian, 2021-12]** After overnight sleep, the contrast slider disappeared for a DP-connected external monitor, and only restarting Monitorian brought it back. The maintainer said it "probably happens because the monitor fails to report its capabilities after resume". The thread does not identify the monitor model. — [Monitorian #263](https://github.com/emoacht/Monitorian/issues/263)
- **[Generic Windows + Lenovo dock, 2023-09]** Monitorian stopped seeing two HP monitors behind a Lenovo Hybrid USB-C dock, a DisplayLink-type hybrid dock. — [Monitorian #510](https://github.com/emoacht/Monitorian/issues/510)
- **[Lenovo vendor claims]** "Some Thunderbolt and USB-C docks drop DDC/CI commands", so try a direct cable. Also: "Sleep/Wake Reset … the monitor drops its last value when the link re-handshakes", and in Windows HDR mode brightness control becomes unresponsive. *Vendor content.* — [DisplayBuddy: Lenovo on Windows](https://displaybuddy.app/control-lenovo-monitor-brightness-on-windows)
- **[Several Lenovo monitors, flashing "DDC/CI Enable/Disable" toast]** Users of Lenovo monitors (L22e-20, L1900p, an unspecified ThinkVision) report the "DDC/CI enable/disable" message flashing continuously. The 10-second-hold DDC/CI toggle is the documented control. The Microsoft Q&A accepted answer (2020-06-09) is to hold the control button 10 s, and one forum workaround is changing the Windows refresh rate (60↔75 Hz), which stops it until reboot. A stuck or faulty button that registers as a long press is a plausible cause, but no one confirmed it. — [Microsoft Q&A](https://learn.microsoft.com/en-us/answers/questions/3764812/ddc-ci-enable-disable-continiously-blinking-on-off); [Lenovo forum: DDC-CI Enable (L22e-20)](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/DDC-CI-Enable/m-p/5191100); [Lenovo forum: DDC/ci enable and disable keeps flashing](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/DDC-ci-enable-and-disable-keeps-flashing-on-monitor/m-p/5235946)
- **[P27h-20 sibling, malformed replies]** The ddccontrol probe of a P27h-20 returned several malformed DDC replies on some VCP codes, but valid replies on brightness, contrast and input. — [ddccontrol-db #133](https://github.com/ddccontrol/ddccontrol-db/issues/133)

### Inferences
- For a P32p-20 on Windows, the most reliable DDC path is likely a **direct DP or HDMI cable from the GPU**. USB-C DP-Alt directly from the laptop is probably fine too, but avoid MST docks and DisplayLink adapters (DisplayLink generally does not pass DDC/CI).
- The P32p-20 USB-C standby-wake bug (2022 report) is a separate problem from DDC. A DDC power command (VCP 0xD6) cannot help, because the monitor is unresponsive while in that state.

### Gaps
- No ThinkVision-specific DisplayLink report was found.
- No report covers the P32p-20 and DDC after sleep specifically.

---

## Q4. P32p-20 OSD/joystick/button failures, and workarounds (software control, factory reset via DDC, repair, warranty)

### Takeaway
No public report of a **P32p-20 joystick hardware failure** was found, so its defect frequency is unknown from public sources. Sibling ThinkVision models do have many "OSD buttons don't work" threads. Most turned out to be firmware or state problems rather than broken hardware:
- OSD lock (long press)
- OSD navigation disabled when there is no signal on the needed input
- Monitor stuck in daisy-chain mode
- Static charge (cleared by power-draining)

The fix in those threads was a power drain, connecting a live source on the right input, or a factory reset, not software. No one was found using DDC VCP 0x04 (factory reset) as a workaround.

### Cited Findings
- **[P32p-20, official guidance]** The P32p-20 user guide documents a 10-second press-and-hold to lock or unlock the OSD and a 10-second hold to enable or disable DDC/CI ("DDC/CI disable" is shown on screen). A failing or bouncing control can therefore lock the OSD or turn DDC/CI off. — [P32p-20 User Guide (62A2)](https://content.etilize.com/User-Manual/1063692945.pdf)
- **[ThinkVision family, Lenovo KB]** Lenovo has a support solution titled "Monitor OSD controls may not respond if pressed too quickly – ThinkVision" (HT514070). The page returned 403, so the affected-model list could not be read. — [Lenovo support HT514070](https://support.lenovo.com/us/en/solutions/ht514070-monitor-osd-controls-may-not-respond-if-pressed-too-quickly-thinkvision)
- **[P27Q-20, 2023-03]** An office with several P27Q-20s found up/down navigation dead on some units. The cause was that "menu navigation only works when there is a signal coming to the required connection, HDMI or Display port". The user plugged a source into DP and could then navigate ("If any Lenovo engineer is reading, please just make the menus work without any connections required!"). Lenovo staff advised a power drain (unplug, hold power 10–15 s). A later user (tagged T24m-20) suggested unplugging everything and holding power for 30 s. — [Lenovo forum: P27Q-20 OSD navigation not working](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/ThinkVision-P27Q-20-monitor-OSD-navigation-controls-not-working/m-p/5214709)
- **[P27h-20, 2020–2021, multiple users]** All OSD options were greyed out and up/down did not work after "Daisy Chain" was turned on, mostly by MacBook users. Several users fixed it by **plugging in a different laptop (e.g., via HDMI)**, after which "suddenly the buttons worked again" and they could factory reset to turn off Daisy Chain. Some users never solved it. — [Lenovo forum: P27h-20 OSD Menu options greyed out](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/Lenovo-ThinkVision-P27h-20-Monitor-OSD-Menu-options-greyed-out/m-p/5023577)
- **[P24q-20, 2021-08]** The menu opened but up/down/OK did nothing, even with no PC attached. A 30 s power drain did not help. The input-select shortcut still worked. Lenovo suggested a 12 s hold on Back and/or Display Control Center software. A second P24h-20 owner said the buttons "are not very responsive … but they still do work if [you] press them with some accuracy", and that brightness/contrast are locked in sRGB/DCI-P3/BT.709 modes. — [Lenovo forum: P24q-20 OSD buttons no response](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178)
- **[T2424p and others, 2019–2024]** All buttons except power appeared dead. The fix was a power drain plus a long press on Menu, which toggled "Menu locked/unlocked". The poster says this "work[s] sometimes but not always and Lenovo is of no help". Another user fixed a frozen menu by restarting the PC attached over USB-C. — [Lenovo forum: T2424p buttons do not work](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/ThinkVision-T2424p-buttons-on-monitor-do-not-work/m-p/4603292)
- **[Brightness locked by picture mode, sibling]** On the P24h/q-20, brightness can't be changed in factory-calibrated colour modes (sRGB, DCI-P3, BT.709). The ddcutil notes for the ThinkVision X1 and L22e-20 likewise say brightness becomes unavailable when Dynamic Contrast is on. In those modes a DDC brightness write may also be refused. — [Lenovo forum P24q-20 thread](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178); [ddcutil monitor notes](https://www.ddcutil.com/archived/monitor_notes/)

### Inferences
- An owner whose P32p-20 joystick fails can, in principle, cover day-to-day needs over DDC/CI: brightness 0x10, contrast 0x12, input 0x60, colour preset 0x14, factory reset 0x04, power 0xD6. All of these appear in sibling capability strings. This only works while DDC/CI is enabled, and a bouncing joystick could toggle DDC/CI off through the 10 s hold.
- Before concluding the joystick is broken, rule out these causes reported on siblings: OSD lock (10 s hold), no live signal on the active input, daisy-chain/MST mode, and static (unplug everything, hold power 15–30 s).

### Gaps
- No P32p-20-specific joystick or button failure reports were found (Reddit was not searchable, and Walmart/Amazon reviews were blocked), so the failure rate cannot be estimated.
- No report of anyone using DDC VCP 0x04 to factory-reset a ThinkVision with dead buttons was found.
- No warranty or repair anecdotes for P32p-20 controls were found.

---

## Q5. Published scripts/gists for ThinkVision (AutoHotkey, PowerShell, ControlMyMonitor batch, Home Assistant, Stream Deck)

### Takeaway
Very few ThinkVision-specific scripts are public. The ones found are the P32p-20 MQTT/light-sensor brightness tool (Windows, Python), a ThinkVision P24h-20 KVM-style input-switch gist (Linux/macOS), and BetterDisplay custom input values for the P40w-20. GitHub code search found no AutoHotkey, PowerShell or ControlMyMonitor batch files mentioning ThinkVision.

### Cited Findings
- **[P32p-20, Windows]** `magcode/mqtt-tools/moco`: a Python tray app (`screen_brightness_control`, `aiomqtt`, `infi.systray`) that sets brightness from an MQTT lux sensor, with per-monitor factor/offset (a P32p-20 entry is included). This is effectively a Home-Assistant-style integration. — [magcode/mqtt-tools](https://github.com/magcode/mqtt-tools/tree/main/moco)
- **[P24h-20, Linux/macOS]** A cross-platform DDC/CI input switching script (ddcutil + BetterDisplay CLI) for a DIY KVM, using 0x11 for HDMI and 0x31 for USB-C. — [gist balantrapu87](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)
- **[P44w-10, Linux]** A gist on updating a ThinkVision P44w-10's firmware under Linux. This is a different model, but it shows that some ThinkVision firmware is user-updatable outside Windows tooling. — [gist chrische: P44w-10 firmware under Linux](https://gist.github.com/chrische/06d949d18af05765868ce7ca1855522a)
- **GitHub code search** found nothing for `ThinkVision setvcp`, `ThinkVision ControlMyMonitor`, `ThinkVision SetValue 60`, `.ahk` files mentioning ThinkVision, or the P32p-20 EDID PnP ID `LEN62A2` in any DDC script. The only `LEN62A2` hits are EDID dumps in linuxhw/EDID. — searched with `gh search code` on 2026-10-03; e.g. [linuxhw/EDID LEN62A2](https://github.com/linuxhw/EDID/tree/master/Digital/Lenovo/LEN62A2)

### Inferences
- The P32p-20's EDID PnP ID is **LEN62A2**, so Windows device instance IDs will look like `DISPLAY\LEN62A2\...`. Use this to target it in ControlMyMonitor (`/SetValue "\\.\DISPLAY1\Monitor0" 10 50`) or Twinkle Tray profiles.

### Gaps
- No Stream Deck, AutoHotkey or PowerShell examples specific to ThinkVision were found. They probably exist on Reddit or in private dotfiles but could not be verified.

---

## Q6. Does Lenovo Display Control Center (ThinkColour) or Lenovo Accessories and Display Manager actually work with the P32p-20?

### Takeaway
Lenovo officially lists the P32p-20 (and the P27u-20 and T32p-20) as supported by Lenovo Display Control Center – ThinkColour. No user report confirming or denying that it works on a P32p-20 was found. Generic LDCC complaints on the Lenovo forum ("unknown" monitor, "This monitor is not supported") exist for other models. No P32p-20 user reports were found for Lenovo Accessories and Display Manager (LADM, an enterprise tool).

### Cited Findings
- **[P32p-20, official]** The P32p-20 spec/PSREF and the user guide list "Lenovo Display Control Center – ThinkColour" support, and the LDCC download page is listed under the T32p-20 and P27u-20 product pages. — [P32p-20 PSREF spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.html?ver=1474e778-9e91-4268-bea6-92fd9e78a6ee); [LDCC download (T32p-20 page)](https://pcsupport.lenovo.com/us/en/products/monitors-and-projectors/lcd-monitors/thinkvision-t32p-20/downloads/ds547223-lenovo-display-control-center-thinkcolor); [LDCC download (P27u-20 page)](https://pcsupport.lenovo.com/us/en/products/monitors-and-projectors/lcd-monitors/thinkvision-p27u-20/downloads/ds547223-lenovo-display-control-center-thinkcolor)
- **[P27h-20 spec]** The P27h-20 PSREF says ThinkColour is supported on Windows 10 only. Newer ThinkColour lists Windows 10/11, macOS and Windows on Arm, with "functions vary across systems". — [P27h-20 PSREF](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P27h_20/ThinkVision_P27h_20_Spec.html?ver=a7e9bb51-f8b4-4410-b6f7-6e1bc9401bb6); [P27u-20 PSREF](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P27u_20/ThinkVision_P27u_20_Spec.html?ver=c2ba1cf0-197a-456d-b418-b6618ee21b53)
- **[Other Lenovo models]** (From search-result excerpts only. These two threads did not render in the browser, so they were not read in full.) Forum threads report LDCC showing the monitor as "unknown" or "This monitor is not supported". Lenovo suggested reinstalling, updating drivers through Vantage, or trying DP instead of HDMI. — [Lenovo forum: lenovo-display-control-center](https://forums.lenovo.com/t5/Pre-Installed-Lenovo-Software-and-Applications/lenovo-display-control-center/m-p/5288638); [Lenovo forum: Y25g-30 LDCC does not recognize monitor](https://forums.lenovo.com/t5/Y-Gaming-Accessories-Curved-Gaming-Monitors/Lenovo-Y25g-30-Monitor-Type-66CC-Lenovo-Display-Control-Center-does-not-recognize-monitor/m-p/5245793)
- **[LADM]** Lenovo Accessories and Display Manager for Enterprise is aimed at IT remote management of monitors and accessories. Its download is listed under models such as the M14t. No user experience reports were found. — [LADM download](https://pcsupport.lenovo.com/uu/en/products/monitors-and-projectors/lcd-monitors/thinkvision-m14t/downloads/ds568567-lenovo-accessories-and-display-manager-artery)
- **[Lenovo staff using LDCC as a button workaround]** On the P24q-20 dead-buttons thread, Lenovo's accepted "solution" was to install Display Control Center. There was no user confirmation that it worked. — [Lenovo forum: P24q-20](https://forums.lenovo.com/t5/Displays-Options-and-Accessories/P24q-20-OSD-buttons-no-response/m-p/5096178)

### Inferences
- LDCC uses DDC/CI underneath, so it needs the same conditions as third-party tools: DDC/CI enabled in the OSD, and a direct, non-MST/non-DisplayLink path. It has no advantage over ControlMyMonitor or Monitorian when the problem is the connection path.

### Gaps
- No first-hand user report of LDCC or LADM working or failing on a P32p-20 was found (the Lenovo forum LDCC threads that could be rendered were for other models, and Reddit was not searchable).
