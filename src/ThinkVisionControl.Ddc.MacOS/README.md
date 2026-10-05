# ThinkVisionControl.Ddc.MacOS

DDC/CI for Apple Silicon Macs through the private IOAVService I²C calls (`IOAVServiceCreateWithService`, `IOAVServiceWriteI2C`, `IOAVServiceReadI2C`). The approach follows m1ddc and MonitorControl (both MIT). No ddcctl (GPL) code is used.

- `IoAvMonitorLink`: the `IMonitorLink`.
- `AvServiceLocator`: finds the display and its `DCPAVServiceProxy`.
- `MacDdcRules`: the pure decisions (target match, service choice, checksum fallback, port class).
- `Native/`: the P/Invoke declarations, each with its C prototype.

## What is verified and what is not

**Verified:**
- `dotnet build` and `dotnet build -r osx-arm64` both finish with 0 errors and 0 warnings. The checks ran on Windows.
- The packet bytes come from Core's `DdcPacket`. They match m1ddc and MonitorControl byte for byte: Get VCP in the legacy form, Set VCP in the spec form.

**Unverified (nothing has run on a Mac yet):**
- Every native call.
- Every P/Invoke signature against the real ABI.
- The CoreDisplay framework path. The code tries `Frameworks/` first, then `PrivateFrameworks/`.
- The matching steps:
  1. `IODisplayLocation` → registry ID.
  2. `DisplayAttributes.ProductAttributes` (`ManufacturerID` / `ProductID`).
  3. The single-external-service fallback.
- The `Transport.Downstream` port hint.
- The timing: 10 ms before each of the two writes, 50 ms before the read, 5 attempts 20 ms apart.
- Which checksum form the P32p-20 accepts.
- The MCDP29xx HDMI path (chip 0xB7, read offset 0x51, 12 bytes, 100 ms).
- Capabilities fragment reads (38 bytes each).
- Behaviour after sleep/wake.

## First run on the Mac

The probe is a .NET 10 file-based app. It only reads, unless you pass `--set-brightness`.

1. Connect the P32p-20 to a USB-C/Thunderbolt port directly, with no dock, and turn DDC/CI on in the monitor's OSD. Then run `dotnet run src/ThinkVisionControl.Ddc.MacOS/probe/MacDdcProbe.cs`.
2. **Displays:** the P32p-20 should be listed as `vendor 0x30AE model 0x62A2 builtin False`, with a non-empty `IODisplayLocation`. If the location is `(none)`, CoreDisplay did not load or returned nothing. The link can still match through step 2 or 3 of the matching list above.
3. **Connect:** expect `TryConnect True`, `LastFailure None`, `match DisplayLocation` and `port DisplayPort`, plus at least 1 external service. If you see `match SingleExternal` or `ProductAttributes`, the m1ddc match missed. Save the output and `ioreg -l -w0 | grep -E "AppleCLCD2|IOMobileFramebufferShim|DCPAVServiceProxy|Location|Transport"`.
4. **Read 0x10:** the probe should print `current N / max 100`, matching the OSD brightness. The last line names the checksum form that worked: spec (`0x51` included) or legacy. Write it down. If you see `detail: ... no valid reply`, the service was found but the monitor stayed silent.
5. **Write:** run the probe again with `-- --set-brightness 40`. The screen should dim and the read-back should say `now 40`. Restore your usual value afterwards.
6. **Capabilities:** run it again with `-- --caps`. Expect a `(prot(monitor)type(...)...vcp(...)...)` string. Then sleep and wake the Mac and re-run step 4: the lookup must succeed again. Also try a built-in HDMI port if the Mac has one. Either `LastFailure UnsupportedPort` or a working `chip 0xB7` is correct there.
