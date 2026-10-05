# Lenovo ThinkVision P32p-20: DDC/CI / MCCS support and Lenovo's Windows control software

Research date: 2026-10-03. Primary sources: the official P32p-20 User Guide (First Edition, March 2020, machine type 62A2), the Lenovo PSREF spec sheet (revision dated 29 Nov 2024), the Lenovo support download pages for LDCC (DS547223) and LADM (DS566167), viewed live on 2026-10-03, the LADM supported-product list (updated Apr 2026) and the LADM release notes (latest entry 17 Jun 2026), plus community DDC capability dumps for **sibling** ThinkVision models. No published capabilities string was found for the P32p-20 itself (see Gaps).

## 1. Does the P32p-20 support DDC/CI, is it on by default, and how is it toggled?

### Takeaway
Yes. The user guide says DDC/CI is **enabled by default**. There is **no OSD menu item** for it. You toggle it by **pressing and holding the Exit key for 10 seconds**, and the screen then shows "DDC/CI disable". The spec table lists the communications standard as "VESA DDC CI".

### Cited Findings
- Direct-access controls table (Table 2-1): key 1 = "DDC-CI: Press and hold for 10 seconds to enable or disable the DDC-CI function". The same key is also "Exit" and "Input Change (Switches the video input source)". [P32p-20 User Guide, p.2-3](https://content.etilize.com/User-Manual/1063692945.pdf)
- "Enables DDC/CI by default. Use OSD Exit Key, Press and hold for 10 seconds to disable / enable DDC/CI function. The words 'DDC/CI disable' shows on the screen." [P32p-20 User Guide, p.2-5](https://content.etilize.com/User-Manual/1063692945.pdf). The ManualsLib copy has the same text: [ManualsLib](https://www.manualslib.com/manual/2260469/Lenovo-Thinkvision-P32p-20.html)
- The Menu key held for 10 s locks and unlocks the OSD. This is a separate function from the DDC/CI toggle. [User Guide p.2-3/2-5](https://content.etilize.com/User-Manual/1063692945.pdf)
- Spec table 3-1: "Communications: VESA DDC CI". [User Guide p.3-2](https://content.etilize.com/User-Manual/1063692945.pdf)
- The user guide's example OSD screenshot shows the footer "Model: P32p-20 SerialNumber:… Firmware: LB0.3". The OSD footer also shows the live input, resolution, refresh rate and USB-C PD wattage (e.g. "Input Signal: HDMI … USB-C Power Delivery: 90W"). [User Guide p.2-4/2-6](https://content.etilize.com/User-Manual/1063692945.pdf)

### Inferences
- The DDC/CI toggle is a hidden long-press with no visible menu entry, so a monitor that "doesn't respond to DDC" may simply have had DDC/CI switched off by an accidental 10-second Exit press. Check this first when troubleshooting.
- The firmware version shown in the OSD (e.g. LB0.3) may matter for DDC behaviour. No firmware changelog for the P32p-20 was found.

### Gaps
- Whether a factory reset or the "Energy Star" reset re-enables DDC/CI is not documented.
- No Lenovo firmware release notes for the P32p-20 mention DDC fixes. None were found.

## 2. Which MCCS VCP codes does it support (capabilities string, input-source values)?

### Takeaway
No capabilities string **for the P32p-20 itself** was found in ddcutil, ddccontrol-db, monitorcontrol, Twinkle Tray, Monitorian or MonitorControl issues, or by web search. The closest evidence comes from same-generation (2020–21, "-20") ThinkVision siblings:
- **P40w-20** (LEN62C1): its full ddccontrol dump advertises `60(0F 11 31)`.
- **P24h-20**: a user script switches it with `0x11` = HDMI and `0x31` = USB-C.

This strongly suggests the P32p-20 uses **DP = 0x0F, HDMI = 0x11 (HDMI-1), USB-C = 0x31**. 0x31 is Lenovo-specific: Dell uses 0x1B, and MCCS defines no USB-C value. Run `ControlMyMonitor` or `ddcutil capabilities` on the real unit to confirm.

### Cited Findings
- **ThinkVision P40w-20 (sibling, PnP ID LEN62C1), full raw capabilities string from ddccontrol 3.3.0:**
  `(prot(monitor)type(LCD)model(ThinkVision P40w-20)cmds(01 02 03 07 0C E3 F3)vcp(02 04 05 08 10 12 14(01 05 06 08 0B 0E 0F) 16 18 1A 52 60(0F 11 31 FF 0F 11 31) 86(02 05) 95 96 97 98 A4 A5 AC AE B2 B6 C6(...) C8 C9(4C 47) CA CC(02 03 04 05 06 09 0A 0D) D6(01 05) DC(00 01 03 05) DF E0(00 01 02) EA(00 01) EB(00 01) EC(00 01 FF 00 03) F2(00 01 03) F4(00 01 02) F5(00 01 FF 00 01) F6(07) F7(...) F8(...) FA(...) FD)mswhql(1)asset_eep(40)mccs_ver(2.2))`
  The same dump reports these current/max readings: brightness 0x10 = 90/100, contrast 0x12 = 75/100, 0x14 = 5 (max 15), 0x16 = 100/100, 0x04 Restore Factory Defaults, 0x05 Restore Brightness and Contrast, 0x08 Restore Factory Default Color. — [ddccontrol-db issue #445 (Lenovo P40w-20)](https://github.com/ddccontrol/ddccontrol-db/issues/445)
- **ThinkVision P24h-20 (sibling):** `ddcutil setvcp 60 0x11` = HDMI and `ddcutil --bus 3 setvcp 60 0x31` = USB-C. The script does not document a DP value. — [GitHub gist: cross-platform DDC/CI monitor switching script](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)
- **ThinkVision P44w-10 (older sibling):** `vcp(02 04 05 08 10 12 14(01 05 06 08 0B 0C 0D) 16 18 1A 52 60( 0F 11 12 13 14) 86(...) ... CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) ... mccs_ver(2.2))`. The user notes that the monitor has an integrated KVM and PBP/PIP. — [ddccontrol-db issue #121](https://github.com/ddccontrol/ddccontrol-db/issues/121)
- **ThinkVision P27q-10 (older sibling):** `vcp(02 04 05 06 08 10 12 14(01 05 06 08 0B) 16 18 1A 52 60(0F 10 11 12) AC AE B2 B6 C6 C8 CA CC(02 03 04 05 06 09 0A 0D) D6(01 04 05) DF FD) mccs_ver(2.2)`. The user reports that brightness works with the generic VESA profile. — [ddccontrol-db issue #30](https://github.com/ddccontrol/ddccontrol-db/issues/30)
- MCCS defines no USB-C value for 0x60, so vendors differ. Dell uses 0x1B for USB-C on the U3818DW and U2419HC. The ddcutil maintainer said he would be "surprised if there is cross-manufacturer agreement on the use of this code". — [ddcutil issue #70](https://github.com/rockowitz/ddcutil/issues/70)
- The P32p-20's physical video inputs are exactly **HDMI 2.0, DisplayPort 1.2, and USB-C (DP 1.2 Alt Mode, 90 W PD)**. It has **no DP-out/MST port** and no second HDMI. [PSREF P32p-20 spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.PDF). The OSD "Input Signal" options are USB-C / DP / HDMI. [User Guide Table 2-2](https://content.etilize.com/User-Manual/1063692945.pdf)

### Inferences
- **Probable P32p-20 VCP map.** This is inferred from P40w-20, P24h-20 and P27q-10 plus the P32p-20 OSD, and is not verified on a P32p-20:

  | VCP | Function | Likely values on P32p-20 | Basis |
  |---|---|---|---|
  | 0x10 | Brightness | 0–100 (OSD default 75) | all siblings; OSD scale 0–100 |
  | 0x12 | Contrast | 0–100 (OSD default 75) | all siblings; OSD scale 0–100 |
  | 0x60 | Input source | 0x0F = DisplayPort, 0x11 = HDMI, **0x31 = USB-C** | P40w-20 caps, P24h-20 script |
  | 0x14 | Color preset | P40w-20 lists 01 05 06 08 0B 0E 0F | sibling caps |
  | 0x16 / 0x18 / 0x1A | R / G / B gain | 0–100 (matches OSD "Custom Red/Green/Blue 0–100") | sibling caps + OSD |
  | 0x04 | Restore factory defaults | write 1 | sibling caps |
  | 0x05 | Restore brightness/contrast | write 1 | sibling caps |
  | 0x08 | Restore factory color | write 1 | sibling caps |
  | 0x86 | Scaling (Full Screen / Original AR) | P40w-20 lists 02 05 | sibling caps + OSD "Aspect Ratio" |
  | 0xCC | OSD language | 02 03 04 05 06 09 0A 0D | sibling caps. These eight values map, per the standard MCCS language table, to English, French, German, Italian, Japanese, Russian, Spanish and Chinese (Simplified), exactly the eight languages in the P32p-20 OSD |
  | 0xD6 | Power mode | 01 = on, 05 = off (P40w-20); P27q-10 and P44w-10 also list 04 | sibling caps |
  | 0xDC | Display mode | P40w-20 lists 00 01 03 05 | sibling caps |
  | 0x62 | Audio volume | **probably not supported**: the P32p-20 has no speakers, only a 3.5 mm audio-out, and no sibling caps string lists 0x62 | spec + sibling caps |
  | 0xE0 | Over Drive | 00 Off, 01 Normal, 02 Extreme | P32p-20 OSD change + read-only snapshot |
  | 0xEA | Dynamic Contrast | 00 Off, 01 On | P32p-20 OSD change + read-only snapshot |
  | 0xEC | USB Charging | 00 Off, 01 On | P32p-20 OSD change + read-only snapshot |
  | 0xE0–0xFD (other) | Manufacturer-specific | remaining codes not mapped | P40w-20 exposes similar codes |
- **P32p-20 colour mode 0x14, experimentally mapped:** 01 = DCI-P3, 05 = Bluish, 06 = Neutral, 08 = Low Blue Light, 0B = sRGB, 0E = Reddish, 0F = BT.709, 12 = User/Custom. Values were captured by cycling through each OSD mode and reading VCP 0x14. These differ from conventional MCCS meanings for some values.
- **Out-of-range 0x60 values.** The P40w-20 advertises "FF" and repeats the list inside 60(...). This looks like a firmware quirk in the capabilities string. Tools that parse it strictly may show odd entries such as "0xFF". Ignore them.

### Gaps
- No P32p-20 (LEN62A2 / 62A2) capabilities string, ControlMyMonitor dump, ddcutil `getvcp` report or monitorcontrol output was found anywhere. **Highest-value next step:** run `ControlMyMonitor.exe /scomma` or `ddcutil capabilities` on the actual monitor. In Windows you can also read the string with `Get-CimInstance` plus a DDC tool, or with `winddcutil capabilities`.
- No published MCCS value for an Ethernet / "KVM" / USB-upstream selection on the P32p-20 was found.

## 3. Physical inputs and OSD menu options; which are DDC-reachable vs OSD-only?

### Takeaway
**Ports:**
- Video: HDMI 2.0, DP 1.2, USB-C (DP Alt Mode, 90 W PD, USB upstream).
- USB: USB-B upstream, 3x USB-A 3.2 Gen 1 downstream (1x BC 1.2).
- Other: RJ-45 Ethernet, 3.5 mm audio-out.

**OSD tree:**
- Monitor Controls: Brightness, Contrast, Dynamic Contrast, Over Drive, Color Mode.
- Advanced Settings: Aspect Ratio, Button Repeat Rate, Input Signal.
- Port Settings: USB Charging, DP Select, USB Source Selection.
- Menu Settings: Language, Menu Time Out, Menu H/V position, Transparency, Factory Reset.

**DDC reach:** brightness, contrast, input, colour preset/RGB gains, scaling, language, power and factory reset use standard MCCS codes. Read-only OSD-change snapshots on this P32p-20 additionally confirmed vendor codes for Dynamic Contrast (`0xEA`), Over Drive (`0xE0`) and USB Charging (`0xEC`). DP Select and USB Source Selection are disabled on the current HDMI input and remain unmapped. Changing Button Repeat Rate from Default to Slow did not change any readable vendor code; it is not exposed by the app. Menu-layout settings also have no identified VCP code.

### Cited Findings
- Rear ports: 1x USB-C 3.2 Gen 1 (DP 1.2 Alt Mode, up to 90 W PD, USB upstream), 1x USB-B 3.2 Gen 1 upstream, 3x USB 3.2 Gen 1 downstream (1x BC 1.2), 1x HDMI 2.0, 1x DisplayPort 1.2, 1x Ethernet RJ-45 (10/100/1000), power, 1x 3.5 mm audio-out. — [PSREF spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.PDF)
- Panel: 31.5" IPS, 3840x2160, 60 Hz, 350 nits, 10-bit (1.07B colours), 99% sRGB / 90% DCI-P3 / 99% BT.709, 14-bit LUT, factory calibration ΔE<2. Speakers: none built in (optional soundbar 0A36190). Special features include "Lenovo ThinkColour (Lenovo Accessories and Display Manager) Support" and "KVM Switch". — [PSREF spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.PDF)
- Direct-access (hot) keys: USB UFP switch (switches the USB upstream port), DDC-CI / Exit / Input Change, Increase/Up/Brightness (direct brightness access), Decrease/Down, and Menu/Enter (hold 10 s = OSD lock). — [User Guide Table 2-1](https://content.etilize.com/User-Manual/1063692945.pdf)
- KVM behaviour: USB-C and USB-B upstream can be connected to two computers at once. Only one is active at a time, the **default focus is USB Type-B**, and the hotkey switches between them with an OSD pop-up. — [User Guide p.2-3/2-4](https://content.etilize.com/User-Manual/1063692945.pdf)
- OSD functions (Table 2-2; the PDF text extraction is partly scrambled, so the groupings are approximate):
  - Brightness and Contrast: scale 0–100.
  - Dynamic Contrast: Off/On.
  - Over Drive: Off/Normal/Extreme.
  - Color Mode: sRGB, BT.709, DCI-P3, Neutral, Reddish, Bluish, Low Blue Light, Custom (Red/Green/Blue 0–100).
  - Aspect Ratio: Full Screen / Original AR.
  - Button Repeat Rate: Off/Default/Slow.
  - Input Signal: USB-C / DP / HDMI.
  - USB Charging: Off/On.
  - DP Select: DP 1.1 / DP 1.2.
  - USB Source Selections: USB 2.0 / USB 3.0.
  - Language: English, French, Italian, Chinese, Spanish, Japanese, German, Russian.
  - Menu Time Out: 5–60.
  - Menu Horizontal / Vertical: 0–100%.
  - Transparency: 0–100%.
  - Factory Reset: Yes/No.
  - An "Energy Star" option/logo.

  The defaults column reads approximately: 75 / 75 / Off / Normal / … / Full Screen / Default / Auto / Off / DP 1.2 / USB 2.0 / English / 30 / 100% / 0% / 20%. — [User Guide p.2-6](https://content.etilize.com/User-Manual/1063692945.pdf)
- The ENERGY STAR option "resets all user settings to default values". — [User Guide p.2-5](https://content.etilize.com/User-Manual/1063692945.pdf)
- Setup step 7: "Remote Network Management and control of display via RJ-45." — [User Guide p.1-7](https://content.etilize.com/User-Manual/1063692945.pdf)
- Audio-out carries audio from DP, Type-C and HDMI. — [User Guide p.1-8](https://content.etilize.com/User-Manual/1063692945.pdf)

### Inferences
- **USB Source Selection (USB 2.0 vs USB 3.0)** probably controls the USB-C lane split. USB 2.0 leaves all 4 lanes for DP 1.2 (4K60). USB 3.0 leaves 2 DP lanes, which would limit USB-C video to roughly 4K30. This matches the default being USB 2.0. Inference from DP Alt Mode lane maths; not stated in the guide.
- The "KVM" in the spec sheet is the USB-upstream switch (USB-B vs USB-C). It is a hotkey and OSD function, not a standard MCCS code. Switching both video and USB from software would need VCP 0x60 plus an undocumented vendor code, or LDCC/LADM.
- "Input Signal" default "Auto" appears in the defaults column, but "Auto" is not listed as an option. Auto-input-detect behaviour is unclear.
- Confirmed via vendor VCP: Dynamic Contrast (`0xEA`), Over Drive (`0xE0`) and USB Charging (`0xEC`). DP Select and USB Source Selection remain untested because they are disabled on HDMI; Button Repeat Rate changed no readable vendor code, and menu-layout controls remain without identified codes.

### Gaps
- The RJ-45 "remote network management" is not explained in the user guide. It is unclear whether it is just a USB-NIC passthrough to the host or a monitor-management channel. This was not resolved. The LDCC Artery (enterprise network management) guidebook URL returned 404.

## 4. Lenovo software: LDCC, LADM, Lenovo View, Vantage. Is the P32p-20 supported? Status in 2026?

### Takeaway
- **Supported tool:** the P32p-20 is officially supported by **Lenovo Display Control Center (LDCC) – ThinkColour**, a UWP app, last version **3.0.32161.0, 19 Nov 2024**, Windows 10/11.
- **Not supported by LADM:** LDCC's successor, **Lenovo Accessories and Display Manager (LADM)** (current Windows v1.0.13.05, 23 Jun 2026), does **not** list the P32p-20 (MT 62A2) in its April-2026 support list. It does list the siblings P40w-20, P27u-20, P34w-20, T24m-20, T24t-20 and T27q-20.
- **Conflicting spec sheet:** the PSREF spec sheet says "ThinkColour (Lenovo Accessories and Display Manager) Support". This conflicts with the LADM list, so LADM support for the P32p-20 is uncertain or partial.
- **What LDCC can do:** colour temperature/gamut, input source, power on/off and firmware update, via a documented **WMI interface** (`root\Lenovo\Lenovo_Display`, class `DisplayDevice`).

### Cited Findings
- **LDCC – ThinkColour (DS547223):**
  - Version **3.0.32161.0**, 128.04 MB, Windows 10/11 32- and 64-bit, released **19 Nov 2024**.
  - Download: `lenovodisplaycontrolcenter_3_0_32161_0.zip`.
  - Description: "simple, graphical user interface for quick control of display attributes, accurate and easy color setting and calibration, simple management between sources, and smarter technology to maximize productivity."
  - Also offered: a Microsoft Store listing, an "LDCC Enterprise Package" (MECM deployment) and an "LDCC WMI User Guide".
  - Page dates: original publish 29 Oct 2020, last modified 19 Nov 2024.
  - Source: [Lenovo support DS547223](https://support.lenovo.com/us/en/downloads/ds547223-lenovo-display-control-center-thinkcolor)
  - Direct files:
    - [LDCC zip](https://download.lenovo.com/consumer/monitor/lenovodisplaycontrolcenter_3_0_32161_0.zip)
    - [LDCC Enterprise package](https://download.lenovo.com/consumer/monitor/ldccenterprisepackage_3_0_32161_0.zip)
    - [LDCC WMI User Guide PDF](https://download.lenovo.com/consumer/monitor/ldcc_wmi_user_guide.pdf)
    - [Microsoft Store 9N6PGT5DKQFQ](https://www.microsoft.com/store/apps/9N6PGT5DKQFQ)
- The LDCC zip contains only an MSIX/AppX bundle (`LenovoDisplayControlCenter_3.0.32161.0_x86.appxbundle`, about 114 MB), dependencies, `setup.exe` and `Add-AppDevPackage.ps1`. There is no readme or supported-model list inside. — zip directory listing read via HTTP range requests from the [LDCC zip URL](https://download.lenovo.com/consumer/monitor/lenovodisplaycontrolcenter_3_0_32161_0.zip)
- **The P32p-20 supports LDCC – ThinkColour** according to the Lenovo datasheet and the search-indexed product page. — [P32p-20 datasheet (PSREF)](https://psref.lenovo.com/syspool/Sys/PDF/datasheet/ThinkVision%20P32p-20_datasheet_EN.pdf). The LDCC download page also appears under the sibling T32p-20 and P27h-20 product trees: [T32p-20 LDCC page](https://pcsupport.lenovo.com/us/en/products/monitors-and-projectors/lcd-monitors/thinkvision-t32p-20/downloads/ds547223-lenovo-display-control-center-thinkcolor), [P27h-20 LDCC page](https://pcsupport.lenovo.com/ec/en/products/monitors-and-projectors/lcd-monitors/thinkvision-p27h-20-flat-panel-monitor/downloads/ds547223-lenovo-display-control-center-thinkcolor)
- PSREF spec: "Lenovo ThinkColour (Lenovo Accessories and Display Manager) Support. Lenovo ThinkColour supports Windows 10 & 11, Mac OS, and Windows On Arm (functions vary across systems). Lenovo Accessories and Display Manager is formerly Lenovo Display Control Center." — [PSREF spec](https://psref.lenovo.com/syspool/Sys/PDF/ThinkVision/ThinkVision_P32p_20/ThinkVision_P32p_20_Spec.PDF)
- **LDCC WMI interface:**
  - It requires the `LenovoDisplayControlCenterService` service to be installed and running.
  - Namespace `root\Lenovo\Lenovo_Display`, class `DisplayDevice`.
  - Properties: ModelName, MonitorIndex, MachineType, SerialNumber, FirmwareVersion, ManufacturerName, MonitorEDID, CurrentPowerMode, SupportPowerMode, CurrentPortUsing, InputSourceList, CurrentColorMode, ColorGamutList.
  - Methods: `SetColorGamut(name)` (e.g. 'sRGB'), `SetInputSource(name)` (e.g. 'HDMI2', using names from InputSourceList), `SetPowerMode(name)` (e.g. 'Off') and `InstallFWWMIFile(path)` for firmware update.
  - Example query: `Get-WmiObject -Class DisplayDevice -Namespace "root\Lenovo\Lenovo_Display"`.
  - Source: [LDCC WMI User Guide](https://download.lenovo.com/consumer/monitor/ldcc_wmi_user_guide.pdf)
- **LADM (DS566167):**
  - Windows **v1.0.13.05**, released **23 Jun 2026**, Windows 10/11 64-bit.
  - Installer: `lenovo_accessories_and_display_manager_v1_0_13_05_setup.exe`.
  - macOS v1.0.3.27 (15 Jan 2026).
  - Page text: "Monitor supports Lenovo Accessories and Display Manager, formerly Lenovo Display Control Center … LADM Mac version only supports some models."
  - The page links an "LADM Support Product List (Updated Apr-2026)" and an enterprise version.
  - Source: [Lenovo support DS566167](https://support.lenovo.com/us/en/downloads/ds566167)
  - Direct files:
    - [LADM setup exe](https://download.lenovo.com/consumer/monitor/lenovo_accessories_and_display_manager_v1_0_13_05_setup.exe)
    - [Release notes](https://download.lenovo.com/consumer/monitor/lenovo_accessories_and_display_manager_release_notes.txt)
    - [Support list xlsx](https://download.lenovo.com/consumer/options/ladm_support_product_list_20260401.xlsx)
    - [Microsoft Store listing](https://apps.microsoft.com/detail/xp8cjs4gf2r4qs?hl=en-US&gl=US)
    - [LADM for Enterprise (DS568567)](https://pcsupport.lenovo.com/us/en/products/monitors-and-projectors/lcd-monitors/thinkvision-m14t/downloads/ds568567-lenovo-accessories-and-display-manager-artery?category=Software+and+Utilities)
- **LADM support list, Apr 2026 ("Monitors" sheet, 132 entries by machine type):**
  - It does **not** contain the P32p-20 / MT 62A2.
  - Its -20 generation entries are: T24m-20 (62CD), T24t-20 (62C5), T27q-20 (61ED), P40w-20 (62C1), P27u-20 (62CB) and P34w-20 (62CC).
  - It does include the successor **ThinkVision P32p-30 (63D1)**, supported on Windows and macOS.
  - Source: [LADM Support Product List xlsx](https://download.lenovo.com/consumer/options/ladm_support_product_list_20260401.xlsx)
- **LADM release notes:**
  - 1.0.10.03 (30 Nov 2025): added monitor functions "USB Select and Gamma".
  - 1.0.8.04 (30 Jul 2025): removed the system-tray device list and added OSD notifications, among other changes.
  - 1.0.7.07 (30 May 2025): added monitor features such as Magnifier, "True Split upgrade", Multi-sync and "BT.709 color (selectable)".
  - The release notes add new models only. The P32p-20 never appears.
  - Source: [LADM release notes](https://download.lenovo.com/consumer/monitor/lenovo_accessories_and_display_manager_release_notes.txt)
- From the search-result summary of a third-party-hosted ThinkColour guidebook (not fetched): ThinkColour "is a software developed based on the Lenovo Display Control Center platform, offered on a range of select models of ThinkVision monitors", with "desktop partitioning, PiP/PbP". — [ThinkColour Solution Guidebook (manuallib mirror)](https://www.manuallib.com/download//2023-10-12/ThinkColour%20Solution%20Guidebook.pdf). The full text could not be fetched (blocked), so treat this as secondary.

### Inferences
- **Practical recommendation for Windows:**
  - **LDCC 3.0.32161.0** (Store or zip) is the Lenovo tool with documented P32p-20 support. LDCC appears frozen: no update since Nov 2024, and LADM is the successor.
  - LADM may still detect the P32p-20 (the PSREF wording suggests it), but it is not on Lenovo's official LADM list as of Apr 2026. Expect partial or no support. Test on hardware.
- LDCC's WMI `SetInputSource` / `SetPowerMode` / `SetColorGamut` almost certainly wraps DDC/CI VCP 0x60 / 0xD6 / 0x14 under the hood. That means it inherits DDC limits: DDC/CI must be enabled and the GPU or dock path must pass DDC. It also gives a scriptable, Lenovo-named alternative to raw VCP values.
- Lenovo Vantage and Lenovo View are PC-side utilities (system and camera). Nothing found suggests either controls ThinkVision monitor settings. LDCC/LADM is the monitor channel.

### Gaps
- No **LDCC** supported-model list was obtained. The download page has none, and the zip has no readme. P32p-20 LDCC support rests on the Lenovo datasheet/PSREF wording.
- The exact LDCC feature set for the P32p-20 was not verified: which sliders appear, whether it offers input switching, desktop partition/hotkeys, or ThinkColour calibration for this model. The LDCC user guide pages (manuals.plus, device.report) were blocked by Cloudflare/403.
- Lenovo View and Lenovo Vantage were not researched in depth. The statement that they don't control monitors is an inference, not a cited finding.
- Lenovo Display Fleet Manager (LDFM), mentioned in a CES 2025 press context, was not checked for P32p-20 support.

## 5. Known quirks: DDC input switching on Lenovo monitors, USB-C/Thunderbolt passthrough, docks

### Takeaway
No P32p-20-specific DDC quirk reports were found. General and sibling evidence:
1. **Use 0x31 for USB-C on Lenovo -20 models, not Dell's 0x1B.**
2. MCCS has no standard USB-C value, so generic tools may label 0x31 as unknown.
3. Some monitors ignore or revert 0x60 writes depending on which input is active or whether a signal is present.
4. DDC over USB-C/Thunderbolt and docks depends on the host GPU or dock passing the DDC/AUX channel. Some host ports are "DDC-blind".

### Cited Findings
- P24h-20 user: USB-C is `setvcp 60 0x31` and HDMI is `0x11`. The same write-up notes that an Apple M3 Pro's native HDMI port is "DDC-blind" and recommends USB-C instead. — [gist](https://gist.github.com/balantrapu87/269c380e23a29a9f78966e9a950e7418)
- ddcutil: "Feature code x60 is particularly problematic". Monitors vary with multiple hosts attached. Some require setvcp to be issued from the host whose input is currently selected; others accept 0x60 regardless. — [ddcutil issue #100 (search-snippet summary)](https://github.com/rockowitz/ddcutil/issues/100)
- Another ddcutil report describes input switching that "works but switching back after a few seconds" (not Lenovo-specific). This is the auto-input-scan behaviour seen on many monitors. — [ddcutil issue #236](https://github.com/rockowitz/ddcutil/issues/236)
- Some monitors (notably LG) advertise 0x60 but silently discard writes, and need a proprietary code. This is cited as an example of the failure mode; it is not a known Lenovo behaviour. — [deskmux README](https://github.com/klaidliadon/deskmux)
- Twinkle Tray keeps a wiki page on DDC/CI detection and support issues (docks, adapters, DDC disabled in OSD). It also has a feature request for customizable input-source VCP values, because vendors use non-standard values. — [Twinkle Tray wiki](https://github.com/xanderfrangos/twinkle-tray/wiki/Display-Detection-&-Support-Issues), [Twinkle Tray #1156](https://github.com/xanderfrangos/twinkle-tray/issues/1156)
- Twinkle Tray issue: "Turn Off Displays can power off DDC/CI monitors and they sometimes cannot wake (requires hard power-cycle)". This is relevant to writing 0xD6 = 05. — [Twinkle Tray #1213](https://github.com/xanderfrangos/twinkle-tray/issues/1213)
- Other ThinkVision DDC problem reports exist on other models: "DDC communication failed ThinkVision T24d-10" ([ddcutil #281](https://github.com/rockowitz/ddcutil/issues/281)) and "Lenovo ThinkVision M14t not work" ([Twinkle Tray #637](https://github.com/xanderfrangos/twinkle-tray/issues/637)).

### Inferences
- **Recommended test sequence on the P32p-20:**
  1. Read the capabilities string.
  2. Read 0x60 on each input to learn the real codes. Expected: DP = 0x0F, HDMI = 0x11, USB-C = 0x31.
  3. Test whether a 0x60 write to an input with **no active signal** sticks. The monitor may auto-scan back to a live input.
  4. Test DDC over the USB-C cable directly versus through any dock. Many USB-C/TB docks with MST hubs block DDC/CI.
- Because the P32p-20's OSD has an "Auto"-looking input default, auto source-scan could revert a DDC input switch when the target has no signal. This is unverified.
- Avoid 0xD6 = 05 (power off) in automation until it has been tested that DDC can wake the monitor again. The Twinkle Tray report shows some monitors need a physical power press.

### Gaps
- No reports were found of P32p-20 DDC behaviour through Lenovo/ThinkPad docks or Thunderbolt hosts.
- It is unknown whether the P32p-20 needs the active-input host to issue 0x60, or accepts it from any connected host.
- It is unknown whether any vendor VCP code (E0–FF) switches the USB upstream port (the KVM). Without one, a full video plus peripherals switch cannot be done purely via DDC.
