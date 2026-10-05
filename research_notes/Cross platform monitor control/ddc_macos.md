# DDC/CI on Apple Silicon macOS (and calling it from .NET 10)

Research date: 2026-10-05. Sources were fetched on that date. Statements marked **(verified)** were checked against source code or release binaries during this session. Statements marked **(unverified)** come from memory or from a search-engine summary, not from a primary source I read.

## Q1. The IOAVService path: symbols, service discovery, display matching, packet format, timing, reads

### Takeaway
All open-source Apple Silicon DDC tools use the same small private API in IOKit.framework: `IOAVServiceCreateWithService`, `IOAVServiceWriteI2C` and `IOAVServiceReadI2C`. They run it against a `DCPAVServiceProxy` IORegistry entry whose `Location` is `"External"`, linked to a framebuffer entry (`AppleCLCD2` / `IOMobileFramebufferShim`, class `IOMobileFramebuffer`). The DDC/CI frame goes to 7-bit I2C address 0x37 with data/sub-address 0x51. The tools write it twice with about 10 ms gaps, wait about 50 ms, then read an 11-byte reply whose max value is in bytes 6–7 and current value in bytes 8–9 (big-endian). Two current reference implementations are MonitorControl `Arm64DDC.swift` and m1ddc `sources/*.m`. Both are MIT.

### Cited Findings

**Private symbol declarations (verified):**
- MonitorControl declares these in its bridging header. They are linked from IOKit, and the CoreDisplay function from CoreDisplay.framework — [MonitorControl Bridging-Header.h (MIT)](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Bridging-Header.h):
  ```c
  typedef CFTypeRef IOAVService;
  extern IOAVService IOAVServiceCreate(CFAllocatorRef allocator);            // default display only
  extern IOAVService IOAVServiceCreateWithService(CFAllocatorRef allocator, io_service_t service);
  extern IOReturn IOAVServiceReadI2C (IOAVService service, uint32_t chipAddress, uint32_t offset,      void* outputBuffer, uint32_t outputBufferSize);
  extern IOReturn IOAVServiceWriteI2C(IOAVService service, uint32_t chipAddress, uint32_t dataAddress, void* inputBuffer,  uint32_t inputBufferSize);
  extern CFDictionaryRef CoreDisplay_DisplayCreateInfoDictionary(CGDirectDisplayID);
  ```
- m1ddc has identical declarations (`IOAVServiceRef` = `CFTypeRef`). It links with `-framework CoreDisplay` and `@import IOKit` — [m1ddc headers/i2c.h, headers/ioregistry.h, Makefile (MIT, © 2021 waydabber)](https://github.com/waydabber/m1ddc).
- The shipped MonitorControl 4.4.0 binary imports the symbols `IOAVServiceCreateWithService`, `IOAVServiceReadI2C` and `IOAVServiceWriteI2C` by name, so they are ordinary exported dynamic symbols that `dlsym` can resolve. **(Verified** by inspecting the binary in [MonitorControl.4.4.0.dmg](https://github.com/MonitorControl/MonitorControl/releases/download/v4.4.0/MonitorControl.4.4.0.dmg)**.)**

**Service discovery, approach A: MonitorControl ordered registry walk (verified)** — [Arm64DDC.swift `getIoregServicesForMatching` (MIT)](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift)
```
root = IORegistryGetRootEntry(kIOMasterPortDefault)
IORegistryEntryCreateIterator(root, "IOService", kIORegistryIterateRecursively, &it)
current = {}; serviceLocation = 0
loop entry = IOIteratorNext(it):
   name = IORegistryEntryGetName(entry)
   if name contains "AppleCLCD2" or "IOMobileFramebufferShim":      # framebuffer (Shim = M2 Pro/Max/Ultra and later)
       current = {
         edidUUID   = IORegistryEntryCreateCFProperty(entry,"EDID UUID", recursive)
         ioDisplayLocation = IORegistryEntryGetPath(entry, kIOServicePlane)
         DisplayAttributes.ProductAttributes.{ManufacturerID, ProductName, SerialNumber, AlphanumericSerialNumber}
         Transport.{Upstream, Downstream}
       }
       serviceLocation += 1
   elif name contains "DCPAVServiceProxy":                           # the DDC endpoint that follows its framebuffer
       if property(entry,"Location") == "External":
           current.service = IOAVServiceCreateWithService(kCFAllocatorDefault, entry)
       results.append(current)
```
- The `IOMobileFramebufferShim` key was added for "high-end M2 (Pro/Max/Ultra) systems using displays connected via USB-C/ThunderBolt/DisplayPort and HDMI". The PR author was on an M1 and asked for M2 Pro/Max testers — [MonitorControl PR #1404 (2023-08)](https://github.com/MonitorControl/MonitorControl/pull/1404).

**Service discovery, approach B: m1ddc direct framebuffer match (verified, newer and more precise)** — [m1ddc sources/ioregistry.m (MIT)](https://github.com/waydabber/m1ddc/blob/main/sources/ioregistry.m)
```
for displayID in CGGetOnlineDisplayList(MAX_DISPLAYS=4):
    info = CoreDisplay_DisplayCreateInfoDictionary(displayID)     # NULL for Sidecar/AirPlay -> skip
    uuid = info["kCGDisplayUUID"]; ioLocation = info["IODisplayLocation"]  # skip if either NULL
    adapter = IORegistryEntryCopyFromPath(kIOMainPortDefault, ioLocation)  # the framebuffer entry
    edid = IORegistryEntrySearchCFProperty(adapter, kIOServicePlane, "EDID UUID", recursive)
    attrs = IORegistryEntrySearchCFProperty(adapter, ..., "DisplayAttributes")["ProductAttributes"]
    vendor/model/serial = CGDisplayVendorNumber/ModelNumber/SerialNumber(displayID)

getDisplayDDCTransport(display):
    targetID = IORegistryEntryGetRegistryEntryID(display.adapter)
    matches = false
    for svc in recursive IOService-plane iterator from root:
        if IOObjectConformsTo(svc, "IOMobileFramebuffer"):
            matches = (IORegistryEntryGetRegistryEntryID(svc) == targetID); continue
        if !matches or name(svc) != "DCPAVServiceProxy": continue
        av = IOAVServiceCreateWithService(kCFAllocatorDefault, svc)
        if av == NULL or property(svc,"Location") != "External": release; continue
        chip = (parent(svc)["EPICProviderClass"] == "AppleDCPMCDP29XX") ? 0xB7 : 0x37
        return {av, chip}
```
- Built-in HDMI on M1 Mac mini: m1ddc PR #60 (merged 2026-08-02) "Detects `AppleDCPMCDP29XX` display proxies – Uses chip address `0xB7` for MCDP29xx while preserving `0x37` for existing transports … Uses a longer read delay for MCDP29xx". It was tested only on an M1 Mac mini with an LG HDR 4K over built-in HDMI — [m1ddc PR #60](https://github.com/waydabber/m1ddc/pull/60).

**Matching an IOAVService to a CGDirectDisplayID / NSScreen (verified)** — [Arm64DDC.swift `ioregMatchScore`, `getServiceMatches`](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift)
- MonitorControl scores each (displayID, ioreg service) pair using `CoreDisplay_DisplayCreateInfoDictionary(displayID)`:
  - +10 if `kIODisplayLocationKey` (IODisplayLocation) equals the framebuffer's IOService path.
  - +1 for each of 4 EDID-UUID substrings that match: vendor ID (hex, offset 0), product ID little-endian (offset 4), week/year-1990 (offset 19), and image size in cm H/V (offset 30).
  - +1 if `DisplayProductName["en_US"]` matches `ProductName`.
  - +1 if `kDisplaySerialNumber` matches.
  - Assignment is greedy from score 20 down. A displayID or service location is never reused.
- m1ddc selects a display by index, or by `id`, `uuid` (default, `kCGDisplayUUID`), `edid`, `seid` (alphanumeric serial + EDID UUID), `basic` (vendor:model:serial), `ext`, or `full` (adds IODisplayLocation) — [m1ddc README / ioregistry.m](https://github.com/waydabber/m1ddc).
- On arm64, `CGDirectDisplayID` "is no longer semi-unique". MonitorControl 4.0 moved per-display preferences to display strings for that reason — [MonitorControl v4.0.0 PR #629 changelog](https://github.com/MonitorControl/MonitorControl/pull/629).
- To map NSScreen to a display ID, use `NSScreen.deviceDescription["NSScreenNumber"]`, which is the CGDirectDisplayID. **(Unverified** in this session; this is standard AppKit.**)**

**Packet format (verified from both codebases):**
- Constants: `ARM64_DDC_7BIT_ADDRESS = 0x37` ("This works with DisplayPort devices") and `ARM64_DDC_DATA_ADDRESS = 0x51` — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift).
- MonitorControl builds `packet = [0x80 | (len(send)+1), len(send)] + send + [chk]`:
  - Get: `send = [vcp]` gives `[0x82, 0x01, vcp, chk]`.
  - Set: `send = [vcp, hi, lo]` gives `[0x84, 0x03, vcp, hi, lo, chk]`.
  - `chk = XOR(packet[0..n-2])` seeded with `0x6E` for a get, or `0x6E ^ 0x51` for a set.
  - The packet is sent with `IOAVServiceWriteI2C(svc, 0x37, 0x51, packet, len)`, so the I2C hardware itself emits the 0x51 source byte as the "data address" — [Arm64DDC.swift `performDDCCommunication`](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift).
- m1ddc does the same:
  - `prepareDDCRead`: `data = {0x82, 0x01, vcp, 0x6e^0x82^0x01^vcp}`.
  - `prepareDDCWrite`: `{0x84, 0x03, vcp, hi, lo, 0x6E^0x51^0x84^0x03^vcp^hi^lo}`.
  - The 0x51 term in the write checksum was a 2025 fix: "Fix input address used for DDC write checksum calculation (#52)".
  - For VCP 0xF4 (LG alternate input) it uses data address 0x50 instead of 0x51 — [m1ddc sources/i2c.m, headers/i2c.h](https://github.com/waydabber/m1ddc/blob/main/sources/i2c.m); [m1ddc commit history](https://github.com/waydabber/m1ddc/commits/main).
- Reply parsing: read 11 bytes (MonitorControl) or 12 bytes (m1ddc).
  - `max = reply[6]<<8 | reply[7]` and `current = reply[8]<<8 | reply[9]`.
  - MonitorControl validates `reply[10] == 0x50 ^ XOR(reply[0..9])`.
  - m1ddc fixed a wrong max-value offset only in June 2026 ("Fix maximum value read from wrong byte offset in Get-VCP reply (#59)") — [Arm64DDC.swift `read`](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift); [m1ddc i2c.m `convertI2CtoDDC`](https://github.com/waydabber/m1ddc/blob/main/sources/i2c.m).
- Read call differences: MonitorControl reads with `IOAVServiceReadI2C(svc, 0x37, 0 /*offset*/, buf, 11)`. m1ddc uses `offset = 0x51` (or 0x50) and length 12. Both reportedly work — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift); [m1ddc i2c.m](https://github.com/waydabber/m1ddc/blob/main/sources/i2c.m).

**Delays and retries (verified defaults):**
- MonitorControl `performDDCCommunication` defaults:
  - Up to 5 attempts (`numOfRetryAttemps` 4, +1).
  - In each attempt: `numOfWriteCycles = 2` writes, each preceded by `usleep(10_000)`.
  - For a read: `usleep(50_000)`, then read and verify the checksum.
  - On failure: `usleep(20_000)` before retrying.
  - All of these are configurable ("Longer Delay" option in advanced settings) — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift); [Monitor Troubleshooting wiki](https://github.com/MonitorControl/MonitorControl/wiki/Monitor-Troubleshooting).
- m1ddc defaults:
  - `DDC_WAIT 10000` µs, with the comment "Depending on display this must be set to as high as 50000".
  - `DDC_ITERATIONS 2` writes, with the comment "Depending on display this must be set higher".
  - Read waits `DDC_WAIT`, or 50 ms (`DDC_MCDP_READ_WAIT`) for MCDP29xx ("10 ms returned empty MCDP29xx replies in testing") — [m1ddc headers/i2c.h](https://github.com/waydabber/m1ddc/blob/main/headers/i2c.h).
- MonitorControl wiki: polling reads at startup can cause screen flicker on some monitors. It recommends setting Polling Mode to none and assuming the last saved values — [Monitor Troubleshooting wiki](https://github.com/MonitorControl/MonitorControl/wiki/Monitor-Troubleshooting).

**Other reference implementations:**
- [AppleSiliconDDC](https://github.com/waydabber/AppleSiliconDDC) is waydabber's self-contained Swift library for BetterDisplay and MonitorControl. It is MIT and was last pushed 2026-08-31. The m1ddc README says "some features and M1 HDMI support is missing from the open source code" — [m1ddc README](https://github.com/waydabber/m1ddc).
- [Lunar](https://github.com/alin23/Lunar) is MIT and was pushed 2026-07-14. @alin23 "spearheaded M1 DDC support" — [MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md).

### Inferences
- Both projects compute the **Get-VCP request checksum without the 0x51 source byte**: `0x6E ^ 0x82 ^ 0x01 ^ vcp`. The MCCS/DDC-CI rule XORs every byte including 0x51. Most monitors evidently tolerate this. A careful implementation could try the spec-correct checksum (`0x6E^0x51^0x82^0x01^vcp`) first and fall back to the legacy value. **Neither variant has been verified on the Lenovo P32p-20.**
- Neither project validates `reply[2]==0x02` (VCP reply opcode), `reply[3]==0x00` (result code; 0x01 = unsupported VCP) or `reply[4]==vcp`. Adding these checks rejects stale or garbage replies cheaply.
- Writes are fire-and-forget: success only means the I2C transaction was accepted. To confirm a set, read the value back after a delay, or keep the last-written value.
- For the Lenovo P32p-20 (Thunderbolt 3 / USB-C or DP input), chip address 0x37 is the relevant path. The 0xB7 MCDP29xx path matters only for M1-era built-in HDMI.

### Gaps
- I found no public documentation from Apple for any IOAVService symbol. The signatures above are reverse-engineered conventions shared by all the tools.
- I could not see what the v4.4.0 commit (2026-09-15, "macOS 26 and 27 compatibility updates") changed inside Arm64DDC.swift. The release notes list only OSD, Settings and shortcut fixes.
- I did not check whether `IOAVServiceReadI2C` with offset 0 versus 0x51 behaves differently on any hardware.

## Q2. Known limitations: built-in HDMI, DisplayLink, docks, macOS version changes, multiple displays, sleep/wake

### Takeaway
DDC over USB-C/Thunderbolt/DisplayPort Alt-Mode works on all Apple Silicon generations covered by the tools. Built-in HDMI is the weak spot:
- **Unsupported in MonitorControl:** all M1 Macs (MacBook Pro 14/16, Mac mini, Mac Studio) and the entry-level M2 Mac mini.
- **Supported in BetterDisplay.**
- **m1ddc:** only very recently (Aug 2026) gained M1 Mac mini HDMI support.

DisplayLink never supports DDC on macOS. Many docks block DDC or forward it to only one monitor. After sleep/wake the DCPAVServiceProxy entry can change, so the service must be looked up again.

### Cited Findings
- MonitorControl README, "Notable exceptions": "DDC control using the built-in HDMI port of the 2018 Intel Mac mini, the built-in HDMI port of all M1 Macs (MacBook Pro 14" and 16", Mac Mini, Mac Studio) and the built-in HDMI port of the entry level M2 Mac mini are not supported. Use USB-C instead or get BetterDisplay for full DDC control over HDMI with these Macs" — [MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md).
- The MonitorControl wiki also says "M2 Pro/Max Macs (MBP 14", 16", Mac mini) are NOT supported at all (neither USB-C nor HDMI)". It adds that BetterDisplay supports all of these, and Lunar supports them except 2018 Mac mini HDMI — [Monitor Troubleshooting wiki](https://github.com/MonitorControl/MonitorControl/wiki/Monitor-Troubleshooting). **This conflicts with** [PR #1404](https://github.com/MonitorControl/MonitorControl/pull/1404) (2023), which added M2 Pro/Max/Ultra support via `IOMobileFramebufferShim`. The wiki appears stale.
- m1ddc now advertises "displays connected through USB-C/DisplayPort Alt Mode and supported built-in HDMI ports". The HDMI support is the MCDP29xx/0xB7 path, tested on an M1 Mac mini only — [m1ddc README](https://github.com/waydabber/m1ddc); [PR #60](https://github.com/waydabber/m1ddc/pull/60).
- Lunar FAQ: on M1/M2 Mac mini "The HDMI port of these devices is blocking DDC requests without a known cause … Using one of the Thunderbolt ports usually fixes this" — [Lunar FAQ](https://lunar.fyi/faq).
- DisplayLink: "DisplayLink docks and dongles do not allow for DDC control on Macs, only software dimming is available" — [MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md). Also: "DisplayLink uses proprietary drivers and while it has support for DDC, that functionality is not provided on macOS" — [Lunar FAQ](https://lunar.fyi/faq).
- Docks/hubs: "A lot of these hubs block DDC requests completely" and "if the hub supports more than one monitor, it usually forwards DDC requests to only one of them" — [Lunar FAQ](https://lunar.fyi/faq).
- Monitors that implement MCCS over USB or a custom protocol (notably EIZO) get software dimming only — [MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md).
- Cables: "people seem to have the highest success rates with DisplayPort and the lowest with HDMI" — [Monitor Troubleshooting wiki](https://github.com/MonitorControl/MonitorControl/wiki/Monitor-Troubleshooting).
- Sleep/wake: "Brightness Controls Lock After Wake … caused by the DDC port for the monitor not being visible anymore inside the macOS I/O Registry tree … The only way to fix it is to restart Lunar so it can get a new refreshed view of the I/O Registry" — [Lunar FAQ](https://lunar.fyi/faq). A related MonitorControl issue is [#777 "Brightness Control lock-out on MacBook screen wake"](https://github.com/MonitorControl/MonitorControl/issues/777).
- macOS versions:
  - MonitorControl v4.3.2 (2024-10) fixed Sequoia crashes. No DDC protocol change is mentioned — [v4.3.2 release](https://github.com/MonitorControl/MonitorControl/releases/tag/v4.3.2).
  - v4.4.0 (2026-09-15) is a "maintenance update addressing compatibility issues with macOS 27 Golden Gate and macOS 26 Tahoe". The listed changes are OSD, settings and shortcut fixes only — [v4.4.0 release](https://github.com/MonitorControl/MonitorControl/releases/tag/v4.4.0).
  - m1ddc fixed a segfault with Sidecar/AirPlay displays in Jan 2026 (#55) — [m1ddc commits](https://github.com/waydabber/m1ddc/commits/main).
- A separate Tahoe problem affects software (gamma) dimming, not DDC: "CGSetDisplayTransferByTable is broken on macOS Tahoe 26.4 RC (and 26.3.1) with MacBook M5 Pro, Max and Neo" — [Apple Developer Forums thread 819331](https://developer.apple.com/forums/thread/819331). (Title read from a search result only, **unverified** beyond that.)
- Multiple displays:
  - m1ddc caps enumeration with `MAX_DISPLAYS 4` ("Set this to 2 or 4 depending on the Apple Silicon Mac you're using") — [m1ddc ioregistry.h](https://github.com/waydabber/m1ddc/blob/main/headers/ioregistry.h).
  - MonitorControl handles identical monitors by preferring the IODisplayLocation match (+10) — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift).
  - MonitorControl has a hard-coded "dummy" exclusion for `ManufacturerID == "AOC"`, `ProductName == "28E850"`. Its `checkIfDiscouraged` (formerly an HDMI check, cf. "#904 Add check for M1 Pro/Max Macbook Pro HDMI") now returns `false` unconditionally — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift).

### Inferences
- An app should rebuild its display-to-IOAVService map on every display reconfiguration and on system wake rather than caching `IOAVService` handles. That avoids the "restart Lunar" failure mode.
- For the Lenovo P32p-20 on a Mac, the reliable path is USB-C/Thunderbolt directly to the monitor. Through a dock, DDC depends on the dock. Built-in HDMI on an M1 or entry-level M2 should be treated as "probably no DDC" unless the MCDP29xx path is implemented.
- Because DDC is unavailable on some ports, the UI needs a graceful "DDC not available on this connection" state.

### Gaps
- I found no authoritative per-model matrix for M3/M4/M5 built-in HDMI DDC. No source fetched states it explicitly.
- I did not confirm whether macOS supports DP MST daisy-chaining (relevant for docks).
- I found no primary source (Apple release notes or tool changelog) showing that macOS 14 or 15 broke the IOAVService path itself. No such breakage was found; the absence is not proof.

## Q3. Entitlements, SIP, root, Accessibility, sandbox, notarization

### Takeaway
IOAVService DDC needs **no root, no SIP change and no special entitlement**. It runs as a normal user process: m1ddc is a plain CLI, and MonitorControl is a normal app. Accessibility permission is needed only for intercepting media/brightness keys. MonitorControl ships **Developer ID-signed, hardened-runtime, notarized (stapled) and not sandboxed**. The Mac App Store variant (MonitorControl Lite) omits DDC entirely. A sandboxed or App Store build should therefore be assumed not to support DDC.

### Cited Findings
- MonitorControl main-app entitlements contain only `com.apple.security.cs.allow-jit` and no `com.apple.security.app-sandbox` — [MonitorControl.entitlements](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/MonitorControl.entitlements).
- The Xcode project sets `ENABLE_HARDENED_RUNTIME = YES` (team 299YSU96J7) — [project.pbxproj](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl.xcodeproj/project.pbxproj).
- The login-item helper *is* sandboxed: `com.apple.security.app-sandbox = true`. It does not do DDC — [MonitorControlHelper.entitlements](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControlHelper/MonitorControlHelper.entitlements).
- **(Verified, release binary)** MonitorControl 4.4.0:
  - The `.app` is signed "Developer ID Application: Istvan Toth (299YSU96J7)".
  - It has a stapled notarization ticket: `Contents/CodeResources` begins with the `s8ch` ticket magic.
  - The main executable contains no `app-sandbox` entitlement.
  - Source: [MonitorControl.4.4.0.dmg](https://github.com/MonitorControl/MonitorControl/releases/download/v4.4.0/MonitorControl.4.4.0.dmg).
  - It is also distributed as a Homebrew cask — [Homebrew cask](https://github.com/Homebrew/homebrew-cask/blob/master/Casks/m/monitorcontrol.rb).
- Accessibility is "required only if you wish to use the native Apple keyboard brightness and media keys" — [MonitorControl README](https://github.com/MonitorControl/MonitorControl/blob/main/README.md). Lunar likewise ties Accessibility to media keys — [Lunar FAQ](https://lunar.fyi/faq).
- MonitorControl Lite (App Store) lacks "Hardware DDC and Apple display control (brightness, audio, contrast)", omitted "to … conform to the App Store guidelines" — [MonitorControlLite README](https://github.com/MonitorControl/MonitorControlLite).
- m1ddc runs from Terminal as a plain binary (`make`, then `./m1ddc …`). `sudo` is used only for `make install` copying into /usr/local/bin — [m1ddc README / Makefile](https://github.com/waydabber/m1ddc).

### Inferences
- Notarization does not reject private-symbol use (MonitorControl is notarized and imports IOAVService*). App Store review would, and the sandbox may also block the IOKit user-client access.
- MonitorControl's `allow-jit` is not needed for DDC. It is a general hardened-runtime exception.

### Gaps
- I did not test whether `IOAVServiceCreateWithService` or `IOAVServiceWriteI2C` actually fail inside an App Sandbox. MonitorControl Lite cites guidelines, not a technical failure.
- I did not check whether macOS 26/27 adds any TCC prompt for IOAVService. None of the sources mention one.

## Q4. Intel Mac path (brief)

### Takeaway
Intel Macs use the public-but-deprecated IOFramebuffer I2C interface (`IOI2CInterface.h`: `IOFBCopyI2CInterfaceForBus` / `IOI2CInterfaceOpen` / `IOI2CSendRequest`). Tools find the framebuffer either through a private CoreGraphics call or through IORegistry matching. This is a separate code path from IOAVService, and m1ddc does not support Intel.

### Cited Findings
- The MonitorControl bridging header imports `<IOKit/i2c/IOI2CInterface.h>` (Intel path) and declares the private `CGSServiceForDisplayNumber(CGDirectDisplayID, io_service_t*)` to map a display to its framebuffer — [Bridging-Header.h](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Bridging-Header.h).
- "Please note that this tool does not support Intel Macs" — [m1ddc README](https://github.com/waydabber/m1ddc).
- The classic Intel CLI ddcctl is **GPL-3.0** (last pushed 2022) — [kfix/ddcctl](https://github.com/kfix/ddcctl). Do not copy its code into an MIT or proprietary product.

### Inferences
- Since .NET 10 targets arm64 and x64 macOS, an x64 build would need the IOFramebuffer path, or it could ship Apple Silicon only. MonitorControl's `IntelDDC.swift` is the obvious MIT reference. **(I did not read it in this session.)**

### Gaps
- I did not examine the exact Intel request structure (`IOI2CRequest` fields, `kIOI2CDDCciReplyTransactionType`) in this session.

## Q5. Calling it from .NET 10 (P/Invoke versus a helper), licensing, signing and notarization

### Takeaway
Two workable designs:

**(a) Pure P/Invoke from C#** into `IOKit.framework`, `CoreFoundation.framework`, `CoreGraphics.framework` and `CoreDisplay.framework`.
- The private IOAVService and CoreDisplay functions are exported symbols, so `[LibraryImport]` or `NativeLibrary.GetExport` can bind them.
- Only `IntPtr`/`uint`/`byte*` marshalling is needed.

**(b) A small native helper** — a C/Obj-C dylib or CLI based on m1ddc, or a Swift dylib based on AppleSiliconDDC — called via P/Invoke or a process launch.
- Every Mach-O must be signed with Developer ID and hardened runtime, then notarized.

All candidate code to borrow is MIT: m1ddc, MonitorControl, AppleSiliconDDC and Lunar. ddcctl is GPL-3.0.

### Cited Findings
- Licenses (GitHub API, verified):
  - [m1ddc](https://github.com/waydabber/m1ddc/blob/main/LICENSE): MIT ("Copyright (c) 2021 waydabber")
  - [MonitorControl](https://github.com/MonitorControl/MonitorControl/blob/main/License.txt): MIT ("Copyright © 2017")
  - [AppleSiliconDDC](https://github.com/waydabber/AppleSiliconDDC): MIT
  - [Lunar](https://github.com/alin23/Lunar): MIT
  - [ddcctl](https://github.com/kfix/ddcctl): GPL-3.0
- m1ddc's Makefile already has `make lib`, which builds `library/libm1ddc.a` plus a merged `libm1ddc.h`. It is a static library with `getOnlineDisplayInfos`, `selectDisplay`, `getDisplayDDCTransport`, `performDDCRead/Write(AtChipAddress)` and `convertI2CtoDDC` — [m1ddc Makefile, headers](https://github.com/waydabber/m1ddc). It could be wrapped into a dylib with a few `extern "C"` exports.
- .NET apps on macOS, from [Microsoft Learn: macOS notarization issues (2026-04-23)](https://learn.microsoft.com/dotnet/core/install/macos-notarization-issues):
  - "all software built after June 1, 2019, and distributed with Developer ID, must be notarized. This requirement applies to … software created with .NET".
  - The SDK only ad-hoc-signs the appHost.
  - Distributed apps must be signed per Apple guidance.
  - The appHost submitted for notarization should carry at least .NET's default entitlements: `com.apple.security.cs.allow-jit`, `allow-unsigned-executable-memory`, `allow-dyld-environment-variables` and `disable-library-validation`.
- NativeAOT supports macOS x64 and Arm64. It requires Xcode Command Line Tools, is supported on .NET 8+, and puts debug symbols in a `.dSYM` folder — [Microsoft Learn: Native AOT deployment](https://learn.microsoft.com/dotnet/core/deploying/native-aot/).
- Binding details from the reference code ([Bridging-Header.h](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Bridging-Header.h); [m1ddc ioregistry.m](https://github.com/waydabber/m1ddc/blob/main/sources/ioregistry.m)):
  - IOKit functions used: `IORegistryGetRootEntry`, `IORegistryEntryCreateIterator`, `IOIteratorNext`, `IORegistryEntryGetName`, `IORegistryEntryGetPath`, `IORegistryEntryCreateCFProperty`, `IORegistryEntrySearchCFProperty`, `IORegistryEntryCopyFromPath`, `IORegistryEntryGetRegistryEntryID`, `IOObjectConformsTo`, `IORegistryEntryGetParentEntry`, `IOObjectRelease`.
  - CoreFoundation is needed for the CFString keys and the CFDictionary/CFNumber/CFString reads.
  - CoreGraphics functions used: `CGGetOnlineDisplayList`, `CGDisplayVendorNumber`, `CGDisplayModelNumber`, `CGDisplaySerialNumber`.

**C# P/Invoke sketch (untested; signatures derived from the C declarations above):**
```csharp
const string IOKit = "/System/Library/Frameworks/IOKit.framework/IOKit";
const string CF    = "/System/Library/Frameworks/CoreFoundation.framework/CoreFoundation";
const string CG    = "/System/Library/Frameworks/CoreGraphics.framework/CoreGraphics";
const string CD    = "/System/Library/Frameworks/CoreDisplay.framework/CoreDisplay"; // path inferred from m1ddc '-framework CoreDisplay'

[LibraryImport(IOKit)] static partial nint IOAVServiceCreateWithService(nint allocator, uint ioService); // returns CFTypeRef; CFRelease when done
[LibraryImport(IOKit)] static unsafe partial int IOAVServiceWriteI2C(nint svc, uint chip, uint dataAddr, byte* buf, uint len);
[LibraryImport(IOKit)] static unsafe partial int IOAVServiceReadI2C (nint svc, uint chip, uint offset,   byte* buf, uint len);
[LibraryImport(IOKit)] static partial uint IORegistryGetRootEntry(uint mainPort);            // kIOMainPortDefault == 0
[LibraryImport(IOKit)] static partial int  IORegistryEntryCreateIterator(uint entry, [MarshalAs(UnmanagedType.LPUTF8Str)] string plane, uint options, out uint it); // "IOService", kIORegistryIterateRecursively = 1
[LibraryImport(IOKit)] static partial uint IOIteratorNext(uint it);
[LibraryImport(IOKit)] static unsafe partial int IORegistryEntryGetName(uint entry, byte* name128);   // io_name_t = char[128]
[LibraryImport(IOKit)] static partial nint IORegistryEntryCreateCFProperty(uint entry, nint cfKey, nint alloc, uint options);
[LibraryImport(IOKit)] static partial int  IOObjectRelease(uint obj);
[LibraryImport(CD)]    static partial nint CoreDisplay_DisplayCreateInfoDictionary(uint displayId);
[LibraryImport(CG)]    static partial uint CGDisplayVendorNumber(uint displayId);
[LibraryImport(CG)]    static partial uint CGDisplayModelNumber(uint displayId);

// Set VCP (e.g. 0x10 brightness = 50)
byte[] p = { 0x84, 0x03, vcp, (byte)(v >> 8), (byte)v, 0 };
p[5] = (byte)(0x6E ^ 0x51 ^ p[0] ^ p[1] ^ p[2] ^ p[3] ^ p[4]);
for (int i = 0; i < 2; i++) { Thread.Sleep(10); IOAVServiceWriteI2C(svc, 0x37, 0x51, p, 6); }

// Get VCP
byte[] q = { 0x82, 0x01, vcp, 0 }; q[3] = (byte)(0x6E ^ q[0] ^ q[1] ^ q[2]);  // legacy (MonitorControl/m1ddc); spec form also XORs 0x51
write q twice (10 ms gaps) -> Thread.Sleep(50) -> IOAVServiceReadI2C(svc, 0x37, 0, r, 11)
check r[10] == 0x50 ^ r[0]^...^r[9]; max = r[6]<<8|r[7]; cur = r[8]<<8|r[9]; retry up to 5x with 20 ms gaps
```

### Inferences
- **dlopen of system framework paths works even though the binaries are not on disk.** Since macOS 11 they live in the dyld shared cache, and `dlopen("/System/Library/Frameworks/X.framework/X")` still resolves. **(Unverified** in this session; this is Apple's Big Sur behaviour as I recall it, and it is how every .NET macOS interop library binds system frameworks.**)** Binding private exported symbols this way is the same mechanism MonitorControl uses at link time (verified symbol imports).
- **Prefer pure P/Invoke** for a .NET app that already links CoreGraphics for display enumeration:
  - There is no extra Mach-O to sign.
  - It works the same under JIT and NativeAOT.
  - The logic is about 200 lines.
  - `LibraryImport` with `unsafe byte*` avoids marshalling cost.
- **A helper (m1ddc as a dylib) is preferable** if you want to reuse battle-tested code verbatim and track upstream fixes such as the MCDP29xx 0xB7 path and the read-offset bug. In that case:
  - Build it universal (arm64 and x86_64 if needed).
  - Sign it with the same Developer ID Team as the app, so `disable-library-validation` is not required.
  - Keep the MIT notice.
  - The CLI-subprocess variant is simplest, but it costs a process launch per call. That is noticeable for slider drags.
- **NativeAOT** removes the JIT, so `allow-jit` and `allow-unsigned-executable-memory` should not be needed. This is an inference from the entitlement purposes, not stated by Microsoft. The default .NET entitlement set is still the safe baseline for a CoreCLR self-contained app.
- **Notarization requirements** for either approach: all Mach-O files signed with Developer ID, hardened runtime enabled, secure timestamp, then `notarytool submit` and `stapler staple`. This is standard Apple practice and was not re-fetched this session. Proof that private-symbol use passes notarization is MonitorControl 4.4.0.
- **Threading:** run DDC I/O off the UI thread. Each set costs at least about 20 ms and each get at least about 70 ms with default delays. Serialize all I2C traffic per display to avoid interleaved transactions.
- **Licensing:** porting m1ddc or MonitorControl logic into C# is a derivative work under MIT. Include their copyright notices in third-party notices. Do not copy ddcctl (GPL-3.0).

### Gaps
- I did not verify that `CoreDisplay.framework` lives at `/System/Library/Frameworks` rather than `PrivateFrameworks`. m1ddc linking with `-framework CoreDisplay` and no `-F` flag implies the public Frameworks directory.
- No .NET-specific sample of IOAVService P/Invoke was found. The C# code above has not been compiled or run.
- I did not confirm the exact entitlements Apple requires for a NativeAOT .NET binary on macOS 26.

## Q6. Detecting monitor connect/disconnect and reading EDID manufacturer/product (finding LEN62A2)

### Takeaway
Use `CGDisplayRegisterReconfigurationCallback` for display add/remove/reconfigure. It fires once per display before the change (with `kCGDisplayBeginConfigurationFlag`) and once after. Re-enumerate on the post-change callback and on wake. To identify the monitor:
- Use public `CGDisplayVendorNumber` / `CGDisplayModelNumber` / `CGDisplaySerialNumber`. LEN62A2 means vendor 0x30AE and product 0x62A2.
- Or use the ioreg `DisplayAttributes.ProductAttributes` dictionary (`ManufacturerID`, `ProductName`, `SerialNumber`, `AlphanumericSerialNumber`) and `EDID UUID` on the framebuffer entry.

### Cited Findings
- Apple's documented signatures — [Apple: CGDisplayRegisterReconfigurationCallback](https://developer.apple.com/documentation/coregraphics/cgdisplayregisterreconfigurationcallback(_:_:)):
  - `CGDisplayRegisterReconfigurationCallback(CGDisplayReconfigurationCallBack callback, void* userInfo) -> CGError`
  - Callback: `(CGDirectDisplayID display, CGDisplayChangeSummaryFlags flags, void* userInfo)`
  - Documented behaviour: "invoked twice for each display that's added, removed, or currently online—once before the reconfiguration, and once after".
- Apple on the callback timing — [Apple: CGDisplayReconfigurationCallBack](https://developer.apple.com/documentation/coregraphics/cgdisplayreconfigurationcallback):
  - Before the change, `flags` "is always set to `kCGDisplayBeginConfigurationFlag`".
  - After the change, "all display state reported by Core Graphics … will be up to date". For removed displays, "calls into Quartz with the removed display ID will fail".
  - The callback runs when "Your application is listening for events in the event-processing thread", so a run loop must be running.
  - It "should avoid attempting to change display configurations".
  - Remove the registration with `CGDisplayRemoveReconfigurationCallback`.
- Flag values `kCGDisplayAddFlag` (1<<4), `kCGDisplayRemoveFlag` (1<<5), `kCGDisplayBeginConfigurationFlag` (1<<0), `kCGDisplaySetModeFlag` (1<<3), `kCGDisplayEnabledFlag` (1<<8) and `kCGDisplayDisabledFlag` (1<<9) are **unverified bit values from my memory of CGDisplayConfiguration.h**. The flag names are confirmed by [Apple: CGDisplayChangeSummaryFlags](https://developer.apple.com/documentation/coregraphics/cgdisplaychangesummaryflags).
- m1ddc gets vendor/model/serial via `CGDisplayVendorNumber`, `CGDisplayModelNumber` and `CGDisplaySerialNumber`. It gets `ManufacturerID`, `ProductName` and `AlphanumericSerialNumber` from the ioreg `DisplayAttributes` → `ProductAttributes` dictionary on the framebuffer, and `EDID UUID` from the same entry — [m1ddc ioregistry.m](https://github.com/waydabber/m1ddc/blob/main/sources/ioregistry.m).
- MonitorControl compares `manufacturerID == "AOC"`, which shows that `ManufacturerID` is the 3-letter PnP string. It also parses the EDID UUID: vendor hex at offset 0, product ID bytes at offset 4 — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift).
- `CoreDisplay_DisplayCreateInfoDictionary` returns `kDisplayVendorID`, `kDisplayProductID`, `kDisplaySerialNumber`, `DisplayProductName`, `kDisplayYearOfManufacture`/`WeekOfManufacture` and `IODisplayLocation`. It returns NULL for virtual displays (Sidecar/AirPlay) — [Arm64DDC.swift](https://github.com/MonitorControl/MonitorControl/blob/main/MonitorControl/Support/Arm64DDC.swift); [m1ddc ioregistry.m](https://github.com/waydabber/m1ddc/blob/main/sources/ioregistry.m).

### Inferences
- The Lenovo PnP ID "LEN" encodes as 0x30AE: L=12, E=5, N=14, giving (12<<10)|(5<<5)|14 = 12462. So for the P32p-20 (LEN62A2), the checks are:
  - `CGDisplayVendorNumber(id) == 0x30AE`
  - `CGDisplayModelNumber(id) == 0x62A2`
  - or `ProductAttributes.ManufacturerID == "LEN"`

  I computed the arithmetic; I have not checked it against a live Mac.
- Recommended .NET event flow:
  1. Register the CG callback on the main thread. The Avalonia/MAUI/AppKit loop provides the run loop; a console app needs `CFRunLoopRun`. Use an `[UnmanagedCallersOnly]` static method as the function pointer.
  2. Ignore `BeginConfiguration`. On any post-change callback, debounce (about 500 ms–1 s).
  3. Re-enumerate displays and IOAVServices.
  4. Also re-enumerate on wake (`NSWorkspaceDidWakeNotification`).

  Optionally, `IOServiceAddMatchingNotification` on `DCPAVServiceProxy` could detect when the DDC endpoint reappears. This is untested.

### Gaps
- I did not confirm whether the raw EDID bytes (not just `EDID UUID`) are exposed on the `AppleCLCD2` entry, or in `CoreDisplay_DisplayCreateInfoDictionary` (for example `IODisplayEDID`) on Apple Silicon.
- The numeric flag bit values above should be checked against the macOS SDK header `CoreGraphics/CGDisplayConfiguration.h`.
