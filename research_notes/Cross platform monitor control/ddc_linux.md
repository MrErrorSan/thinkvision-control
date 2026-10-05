# DDC/CI (MCCS VCP) on Linux, and calling it from a .NET 10 app

Scope: brightness (VCP 0x10), contrast (0x12), input source (0x60) and the capabilities string, sent to an external monitor (target: Lenovo ThinkVision, EDID manufacturer `LEN`, product 0x62A2) on Ubuntu and Fedora, GNOME and KDE, X11 and Wayland, Intel, AMD and NVIDIA GPUs. Researched 2026-10-05.

Marking convention: **[unverified]** means the item comes from general knowledge (kernel headers, the VESA spec as commonly summarized) and was not confirmed against a fetched source in this session.

---

## Q1. Direct /dev/i2c-N access: the kernel module, mapping connector to bus to EDID, the DDC/CI wire protocol, and ioctls from C#

### Takeaway
The DDC/CI path on Linux is userspace I2C through the `i2c-dev` character devices that the GPU driver exposes for each connector's DDC lines. The steps are: open `/dev/i2c-N`, check the EDID at 7-bit address 0x50 to identify the monitor, then send and receive MCCS packets at 7-bit address 0x37 with a 40–50 ms wait between them. Every step can be done from C# with a few libc P/Invokes (`open`, `ioctl`, `read`, `write`, `close`), with no native shim needed.

### Cited Findings

**The i2c-dev kernel module**
- `/dev/i2c-*` nodes only exist if the `i2c-dev` module is loaded: "You need to load module i2c-dev for this." — [Linux kernel i2c dev-interface docs](https://docs.kernel.org/i2c/dev-interface.html)
- Some distributions build `i2c-dev` into the kernel. Check with `grep i2c[-_]dev.ko /lib/modules/$(uname -r)/modules.builtin`. To check whether it is loaded: `lsmod | grep i2c[-_]dev`. To load it at boot, put a file ending in `.conf` (e.g. `i2c-dev.conf`) containing the single line `i2c-dev` in `/etc/modules-load.d/`. This works on both Ubuntu and Fedora because it uses `systemd-modules-load.service`. The names `i2c-dev` and `i2c_dev` are interchangeable. — [ddcutil: kernel module](https://www.ddcutil.com/kernel_module/)
- ddcutil 1.4+ is supposed to handle loading automatically, but older installs may need manual setup. — [ddcutil: config steps](https://www.ddcutil.com/config_steps/)

**Kernel ioctl interface**
- `I2C_SLAVE` changes the slave address: "The address is passed in the 7 lower bits of the argument." `I2C_FUNCS` gets the adapter functionality. `I2C_RDWR` does a "combined read/write transaction without stop in between. Only valid if the adapter has I2C_FUNC_I2C." You can also use plain `read(2)`/`write(2)` after setting the address with `I2C_SLAVE`: "You do not need to pass the address byte." — [kernel i2c dev-interface](https://docs.kernel.org/i2c/dev-interface.html)
- The `I2C_RDWR` argument is `struct i2c_rdwr_ioctl_data { struct i2c_msg *msgs; int nmsgs; }`. Read messages set the `I2C_M_RD` flag. — [kernel i2c dev-interface](https://docs.kernel.org/i2c/dev-interface.html)
- **[unverified]** The ioctl numbers from `<linux/i2c-dev.h>` are: `I2C_SLAVE = 0x0703`, `I2C_TENBIT = 0x0704`, `I2C_FUNCS = 0x0705`, `I2C_SLAVE_FORCE = 0x0706`, `I2C_RDWR = 0x0707`. `I2C_M_RD = 0x0001`, `I2C_FUNC_I2C = 0x00000001`. `struct i2c_msg` is `{ __u16 addr; __u16 flags; __u16 len; __u8 *buf; }`, which is 16 bytes on 64-bit (6 bytes of fields, 2 bytes of padding, an 8-byte pointer).

**A reference C implementation (janouch desktop-tools `ddc-ci.c`)**
- Constants: `DDC_ADDRESS_HOST = 0x50`, `DDC_ADDRESS_DISPLAY = 0x6E` (8-bit form, so the 7-bit I2C address is 0x6E>>1 = 0x37), `DDC_LENGTH_XOR = 0x80`. Opcodes: `DDC_GET_VCP_FEATURE = 0x01`, `DDC_GET_VCP_FEATURE_REPLY = 0x02`, `DDC_SET_VCP_FEATURE = 0x03`. VCP codes: `VCP_BRIGHTNESS = 0x10`, `VCP_CONTRAST = 0x12`, `VCP_INPUT_SOURCE = 0x60`. — [janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)
- Building a request: source byte `0x50|1 = 0x51`, then `0x80 | (args_len+1)`, then the command, then the args. The checksum starts at `0x6E` (the display address) and XORs in every byte sent. — [janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)
- It sends with `I2C_RDWR` using one `i2c_msg { .addr = 0x6E >> 1, .flags = 0, ... }`, waits `wait_ms(40)`, then reads. It validates the reply by checking that the sender byte equals `0x6E` and that `length & 0x80` is set. The reply payload is `0x02`, result code, VCP opcode, type, max (16-bit big-endian), current (16-bit big-endian). — [janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)
- To verify a monitor is present and identify it, it reads the EDID with a write of offset 0 to address 0x50, then a read (`I2C_M_RD`) from 0x50, and checks for the header `00 FF FF FF FF FF FF 00`. It identifies an I2C device by `S_IFCHR` plus `ioctl(fd, I2C_FUNCS)` having `I2C_FUNC_I2C`. — [janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c)
- Reply checksums use 0x50 as the initial XOR value. 0xF3 is the capabilities request opcode. — [DDCCI Perl XS (cpancover)](https://cpancover.com/latest/DDCCI-0.003/DDCCI-xs.html) (search-snippet level)
- Per the spec, the host must wait "40–200 ms (depending on operation) between sending a command to the monitor and reading the response". ddcutil spends about 90% of elapsed time sleeping. — [ddcutil: performance options](https://www.ddcutil.com/performance_options/)

**Exact packets** (bytes as written to or read from the fd after `I2C_SLAVE 0x37`; the 0x6E destination byte is implicit in the I2C address on write)
- Get VCP: `51 82 01 <vc> <chk>`, where `chk = 6E^51^82^01^vc`. Wait at least 40 ms, then read 11 bytes: `6E 88 02 <rc> <vc> <type> <maxH> <maxL> <curH> <curL> <chk>`, where `chk = 50 ^ XOR(bytes[0..9])`. `rc` = 0 means supported and 1 means unsupported VCP. Layout from janouch above. **[unverified]** The rc semantics and the exact 11-byte read length come from the VESA DDC/CI 1.1 spec as commonly summarized.
- Set VCP: `51 84 03 <vc> <valH> <valL> <chk>`. There is no reply. **[unverified]** Wait about 50 ms before the next command (spec value).
- Capabilities: `51 83 F3 <offH> <offL> <chk>`. The reply is `6E <0x80|len> E3 <offH> <offL> <data...> <chk>`. Repeat with offset += data length until a fragment arrives with zero data bytes. The result is an ASCII string like `(prot(monitor)type(LCD)model(...)cmds(...)vcp(10 12 60(0F 11 12) ...)mccs_ver(2.2))`. **[unverified]** The `E3` reply opcode, the zero-length terminator and the roughly 32-byte fragment size come from the spec as commonly summarized. ddcutil treats multi-part reads as a separate retry class (see Q4).
- **[unverified]** A "null message" reply `6E 80 BE` means the display is busy or not ready, or (for some monitors) that the feature is unsupported. Treat it as retryable.

**Mapping a DRM connector to the bus and EDID**
- **[unverified]** `/sys/class/drm/cardN-<CONNECTOR>/` (e.g. `card1-HDMI-A-1`, `card0-DP-2`) contains `status` (`connected`/`disconnected`), `enabled`, `edid` (raw binary EDID) and, on modern kernels with drivers that set `connector->ddc` (amdgpu, i915, nouveau, most KMS drivers), a `ddc` symlink to the I2C adapter. Some drivers instead expose an `i2c-N` child directory under the connector (seen on amdgpu/i915 DP AUX channels). `/sys/bus/i2c/devices/i2c-N/name` gives the adapter name (e.g. `AMDGPU DM i2c hw bus 1`, `i915 gmbus dpb`, `DPMST`, `NVIDIA i2c adapter 3 at 1:00.0`).
- A robust approach that works whatever the driver (and is what ddcutil effectively does): enumerate every `/dev/i2c-N`, skip adapters whose sysfs name is clearly not a GPU bus (SMBus, `i2c-designware`, etc.), read EDID at 0x50, then match the EDID to the target monitor. Then use the connector sysfs `edid` file to link the bus back to a connector name for display in the UI. EDID read pattern source: [janouch ddc-ci.c](https://cgit.janouch.name/desktop-tools/tree/ddc-ci.c).
- **[unverified, standard EDID layout]** EDID bytes 8–9 hold the manufacturer ID as big-endian packed 5-bit letters. `LEN` = (12<<10)|(5<<5)|14 = `0x30AE`, so bytes 8–9 = `30 AE`. Bytes 10–11 hold the product code little-endian, so 0x62A2 appears as `A2 62`. Bytes 12–15 hold the serial (LE32), and the descriptor blocks hold the monitor name and serial strings.

### Inferences
- The protocol is small enough that a managed C# implementation (about 300–500 lines including retries and capabilities parsing) is realistic. It mirrors exactly what the Windows `LowLevelMonitorConfigurationAPI` does internally, so the VCP and capabilities parsing code can be shared across platforms. Only the transport differs.
- **Recommended transport: `I2C_SLAVE` (0x37) + `write()` + sleep + `read()`.** This is simpler to P/Invoke than `I2C_RDWR`: no struct marshalling, no pointer-in-struct pinning. Use `I2C_RDWR` only for the EDID read (offset write + read without a stop), or do the EDID read with `I2C_SLAVE 0x50` + `write([0])` + `read(128 or 256)`, which works on practically all GPU adapters.
- **C# P/Invoke sketch** (`LibraryImport`, AOT- and trim-safe):
  - `[LibraryImport("libc", SetLastError = true, StringMarshalling = Utf8)] static partial int open(string path, int flags);` with `O_RDWR = 2`. **[unverified]** Add `O_CLOEXEC = 0x80000` on x64 and arm64.
  - `[LibraryImport("libc", SetLastError = true)] static partial int ioctl(int fd, nuint request, nint arg);`. `ioctl` is variadic in C, but on Linux x64 (SysV) and Linux arm64 (AAPCS64; variadic args are passed like normal args on Linux, unlike Apple) passing a single pointer-sized integer works. For `I2C_SLAVE`, pass the address directly as `arg`. For `I2C_FUNCS`, pass a pointer to a `ulong`.
  - `read`/`write`: `static partial nint read(int fd, byte* buf, nuint count);` and likewise for `write`. Plus `close(int fd)`.
  - Map errno via `Marshal.GetLastPInvokeError()`. EACCES means permissions (see Q2), ENXIO/EREMOTEIO means no device ACKing at 0x37 (the monitor has DDC/CI disabled in its OSD, or the bus is wrong), and EBUSY may mean another client.
  - Run the blocking, sleep-heavy I/O on a dedicated worker thread with a per-bus `SemaphoreSlim`, never on the UI thread.
  - For `I2C_RDWR`, define `[StructLayout(Sequential)] struct I2cMsg { ushort Addr; ushort Flags; ushort Len; byte* Buf; }` (the runtime inserts the padding before the pointer automatically) and `struct I2cRdwrData { I2cMsg* Msgs; uint NMsgs; }`, then pin the buffers with `fixed`.

### Gaps
- The VESA DDC/CI 1.1 spec itself (exact per-command delays, the null-message semantics, the capabilities fragment size) was not fetched, because it is paywalled or distributed informally. Values marked [unverified] above should be cross-checked against ddcutil source (`src/ddc/ddc_packet_io.c`, `src/i2c/i2c_bus_core.c`) before release.
- Whether `/sys/class/drm/*/ddc` symlinks are present for the NVIDIA proprietary driver (it does not use DRM KMS connector DDC in the same way) is not confirmed. Assume it is absent and fall back to EDID scanning.

---

## Q2. Permissions: default ownership, the i2c group, ddcutil's udev rule, and Flatpak/Snap

### Takeaway
By default, `/dev/i2c-*` is root-only. The normal fix is the udev rule that ddcutil installs (`60-ddcutil-i2c.rules`), which tags GPU I2C devices `uaccess`, so the logged-in seat user gets an ACL automatically with no group membership and no relogin. The fallback is an `i2c` group plus a custom udev rule. Snap effectively cannot do this on desktop. Flatpak needs `--device=all`. Ship the app as a native package or a self-contained tarball/AppImage, and check or install the udev rule yourself.

### Cited Findings
- ddcutil needs read/write access to the `/dev/i2c` devices of the video controllers. The package installs `/usr/lib/udev/rules.d/60-ddcutil-i2c.rules`, which "uses tag **uaccess** to grant read/write permissions to /dev/i2c devices associated with a video controller." Manual setup is needed on eudev systems (no logind/uaccess). ddcutil "can, of course, be run as root." — [ddcutil: I2C permissions](https://www.ddcutil.com/i2c_permissions/)
- The group-based alternative: `sudo groupadd --system i2c` (if the group does not exist), `sudo usermod <user> -aG i2c`, plus `/etc/udev/rules.d/45-myi2c.rules` containing `SUBSYSTEM=="i2c-dev", KERNEL=="i2c-[0-9]*", GROUP="i2c"`. "Depending on your distribution, package ddcutil should depend on package i2c-tools," which typically creates the group. For a one-off test only: `sudo chmod a+rw /dev/i2c-*`. — [ddcutil: permissions using group i2c](https://www.ddcutil.com/i2c_permissions_using_group_i2c/)
- USB-HID-controlled monitors (Eizo ColorEdge, Apple Cinema) need a separate `60-ddcutil-usb.rules` from `/usr/share/ddcutil/data/` copied into `/etc/udev/rules.d`. — [ddcutil: config steps](https://www.ddcutil.com/config_steps/); [ddcutil GitHub README](https://github.com/rockowitz/ddcutil)
- The GNOME "Display Brightness Control using ddcutil" extension tells users to `modprobe i2c-dev`, create the i2c group, add their user, copy the udev rules into `/etc/udev/rules.d`, add `i2c-dev` to `/etc/modules-load.d/i2c.conf`, and reboot. — [daitj/gnome-display-brightness-ddcutil](https://github.com/daitj/gnome-display-brightness-ddcutil)
- **Snap:** the `i2c` interface is not auto-connected ("auto-connect: no"), is gadget-provided, and is "intended to be used with Ubuntu Core". Each plug and slot names one specific `path` (e.g. `/dev/i2c-0`) or `sysfs-name`. It does not apply to classic desktop Ubuntu. — [Snapcraft i2c interface](https://snapcraft.io/docs/reference/interfaces/i2c-interface/)
- **Flatpak:** the device permissions are `dri`, `kvm`, `shm`, `input`, `usb`, and `all` ("All devices, including all of the above except shm"). There is no i2c-specific permission. "While not ideal, `--device=all` can be used to access devices like webcams..." — [Flatpak sandbox permissions](https://docs.flatpak.org/en/latest/sandbox-permissions.html)

### Inferences
- The uaccess rule is the best user experience: it applies to whoever holds the active local session and needs no logout. A user does need logout or login (or `newgrp`) after being added to the `i2c` group. **[unverified]** This is standard Linux group semantics; the ddcutil page does not state it.
- Ubuntu ships `ddcutil` in universe and Fedora in its main repos. Installing the `ddcutil` package (or `libddcutil`) gets the uaccess rule onto the system. So the simplest onboarding instruction is: "install ddcutil from your distro, reboot or replug." Alternatively, the app can ship its own copy of a uaccess rule and install it via a `pkexec` helper on first run. **[unverified]** The exact content of 60-ddcutil-i2c.rules (it matches on the I2C adapter's parent being a display-class PCI device) should be copied from the ddcutil source tree before reusing it.
- First-run diagnostics the app should do: (1) does `/dev/i2c-*` exist? If not, i2c-dev is not loaded. (2) `access(path, R_OK|W_OK)`: if denied, show the exact fix commands. (3) Is the module built in? Use the `modules.builtin` grep.
- **Packaging recommendation:** distribute a .deb/.rpm (or a self-contained tarball/AppImage with a setup script) that depends on or recommends `ddcutil`/`i2c-tools` for the rules. Avoid Snap entirely for this feature. Flatpak is possible only with `--device=all` (a Flathub reviewer concern), and even then host udev permissions still apply inside the sandbox.

### Gaps
- Whether Flathub currently accepts `--device=all` for monitor-control apps was not confirmed (no policy page fetched).
- Whether Ubuntu 24.04/26.04 and Fedora 42+ create an `i2c` group by default without i2c-tools was not confirmed from a primary source.

---

## Q3. libddcutil vs our own implementation (licensing, availability, calling from C#), and the ddcutil CLI as a fallback

### Takeaway
libddcutil is **GPL-2.0 (or later), not LGPL**. Linking it from a non-GPL .NET app, even via dynamic P/Invoke, carries license risk. It is also an extra native dependency with its own threading and init rules. Recommendation: implement DDC/CI in managed C# over `/dev/i2c` (Q1), and optionally shell out to the `ddcutil` CLI (a separate process, so no linking question) as a diagnostic or fallback path.

### Cited Findings
- The ddcutil repository license is "GPL-2.0". It builds both the `ddcutil` CLI and the `libddcutil` shared library. It can also drive USB-HID monitors (Eizo ColorEdge, Apple Cinema). The default branch showed version "3.0.3-dev" when fetched (2026-10). — [rockowitz/ddcutil](https://github.com/rockowitz/ddcutil)
- The ddcutil-lib package (EL8 extras) is listed as GPL-2.0+. Distro listings vary between "GPL-2.0-or-later" and "GPL-2.0-only". None list LGPL. — [getpagespeed ddcutil-lib](https://extras.getpagespeed.com/redhat/8/x86_64/repoview/ddcutil-lib.html); [Alpine ddcutil](https://pkgs.alpinelinux.org/package/edge/community/s390x/ddcutil) (conflict on the -only/-or-later detail noted)
- The Debian/Ubuntu library soname is `libddcutil.so.5` (the 2.x API). openSUSE ships 2.2.1. Debian backports ship 2.2.1. — [Debian ftp-master ddcutil 2.2.1 bpo](https://ftp-master.debian.org/new/ddcutil_2.2.1-5~bpo13%2B1.html); [SUSE PackageHub ddcutil 2.2.1](https://packagehub.suse.com/packages/ddcutil/2_2_1-bp160_1_3/) (search-snippet level)
- The libddcutil API includes `ddca_open_display2()`. A past bug made `ddca_find_display_ref`/`ddca_get_display_ref` return wrong pointer types, causing segfaults or DDCRC_INVALID_DISPLAY. The library has "additional API functions for display change detection". — [openSUSE ddcutil.changes](https://src.opensuse.org/pool/ddcutil/src/branch/factory/ddcutil.changes) (search-snippet level)
- KDE PowerDevil uses libddcutil. Its errors surface as e.g. `ddca_set_non_table_vcp_value -3023`. — [KDE Discuss: brightness slider missing](https://discuss.kde.org/t/brightness-slider-missing-in-plasma-when-powerdevil-has-no-ddc-or-backlight-backend/48004) (search-snippet level)
- The GNOME extension uses the CLI: it calls `ddcutil detect`, `ddcutil getvcp 10` and `ddcutil setvcp 10 <value>`, and caches the detect output because cold-boot detection can hang. — [daitj/gnome-display-brightness-ddcutil](https://github.com/daitj/gnome-display-brightness-ddcutil)

### Inferences
| Option | Pros | Cons |
|---|---|---|
| **Own managed implementation** (recommended) | No license entanglement. No native dependency. Shares the VCP and capabilities parser with Windows. Full control of timing, retries and locking. Works with NativeAOT. | You must handle retries, dynamic sleep tuning, MST phantom buses and the NVIDIA quirks yourself. Smaller compatibility corpus than ddcutil's. |
| libddcutil via P/Invoke | Mature quirk handling (dynamic sleep, retries, phantom display detection, USB HID monitors, display-change callbacks). | GPL linking question for a non-GPL app. The API and soname changed between 1.x and 2.x (`.so.4` to `.so.5`), so probing for versions is needed. Requires the distro package. Process-global init and config file (`ddcutilrc`) behaviour. |
| `ddcutil` CLI subprocess | No linking (an "aggregate" or arm's-length use). Trivial to implement (`--bus N`, `--brief`, `--terse` output parsing). Good diagnostic (`ddcutil environment`, `ddcutil detect`). | Slow: about 1 s+ per invocation because of detection unless `--bus`/`--skip-ddc-checks` is used. Output-format drift between versions. Must be installed. |
- A pragmatic design: a managed implementation as the primary path, plus an optional "Diagnostics: run `ddcutil environment`/`detect`" button that shells out when ddcutil is present. This also gives users a path for monitors the managed code cannot handle.
- License disclaimer: whether dynamic loading of a GPL library from proprietary or permissively licensed code creates a derivative work is legally contested. Treat this as needing legal review, not as settled. **[inference]**

### Gaps
- libddcutil's thread-safety guarantees and the exact names of its 2.x display-change callback API (e.g. `ddca_register_display_status_callback`, `ddca_start_watch_displays`) were not confirmed. The ddcutil.com API pages returned 404 at fetch time. **[unverified]**
- ddcutil's GitHub branch showed `3.0.3-dev`. Whether a 3.0 release has shipped, and whether it changed the soname, is unknown.

---

## Q4. Known issues: NVIDIA, AMD/Intel, docks/MST/USB-C, DisplayLink, Wayland, sleep/resume, slow monitors

### Takeaway
AMD (amdgpu) and Intel (i915/xe) generally work directly. On some cards the NVIDIA proprietary driver needs a modprobe option to make I2C work at all. MST docks produce "phantom" duplicate buses, and some Intel+dock combinations fail entirely. DisplayLink has no usable DDC/CI on Linux. Wayland versus X11 does not matter. Expect some monitors to need longer delays and retries, so build in adaptive sleep and retries.

### Cited Findings
- **NVIDIA proprietary:** "When using NVIDIA's proprietary driver, I2C communication fails on some graphics cards." The workaround is `/etc/modprobe.d/nvidia_i2c.conf` with `options nvidia NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100`, then a reboot. Verify with `grep RegistryDwords /proc/driver/nvidia/params`. An older `xorg.conf.d` method exists. — [ddcutil: NVIDIA](https://www.ddcutil.com/nvidia/)
- **MST/docks:** on recent Intel (i915) with docking stations, I2C communication does not work. Some `/dev/i2c-n` have sysattr name `DPMST`. Probing shows 0x50 and 0x37 as active, but the EDID read fails. — [freedesktop drm/intel #37](https://gitlab.freedesktop.org/drm/intel/issues/37) (search-snippet level)
- With MST, the same monitor's EDID is readable from two `/dev/i2c` devices, but usually only one supports DDC. `ddcutil detect` reports the other as a "phantom display". — [freedesktop drm/amd #1622](https://gitlab.freedesktop.org/drm/amd/-/issues/1622) (search-snippet level)
- Daisy-chained monitors: "Daisy chain doesn't allow running parallel instance of ddcutil." The GNOME extension exposes a sleep-multiplier setting for this case. Docking compatibility issues are referenced to the ddcutil FAQ. — [daitj/gnome-display-brightness-ddcutil](https://github.com/daitj/gnome-display-brightness-ddcutil)
- **DisplayLink:** DDC/CI support for DL-4xxx arrived in driver 8.1, but it appears limited to Windows. On Linux, DisplayLink outputs are evdi virtual connectors with no I2C bus. — [DisplayLink forum](https://www.displaylink.org/forum/showthread.php?p=96961) (search-snippet level; the Linux evdi detail is **[unverified]**)
- **Slow monitors and timing:** `--sleep-multiplier` scales all DDC waits (`.5` halves, `4` quadruples). Some monitors work at `.1`. Dynamic sleep (`--enable-dynamic-sleep`, the default) tunes the multiplier automatically and persists stats in `$HOME/.cache/ddcutil/stats`. Default max tries are write-only 4, write-read 10 and multi-part (e.g. capabilities) 8, with a ceiling of 15. — [ddcutil: performance options](https://www.ddcutil.com/performance_options/)
- **Sleep/resume:** KDE's earlier libddcutil integration failed to restore brightness after resume. ddcutil initialization also made `powerdevil` time out at login, so Arch disabled DDC in powerdevil 5.27.5-3. Upstream made it non-experimental and default-on for Plasma 6. — [Arch FS#78536](https://bugs.archlinux.org/task/78536)

### Inferences
- **Wayland is irrelevant to the I2C path.** `/dev/i2c-N` is a kernel char device, independent of the display server. The only Wayland-specific concern is that the compositor's session daemon (KDE PowerDevil) may also be talking DDC on the same bus (see Q5). Wayland does affect the separate problem of mapping a bus to a desktop "screen" for UI placement, because there is no global screen coordinate API. Use connector names and EDID instead.
- Implementation defaults: 40 ms after a Get before the read, 50 ms after a Set, 50 ms between capability fragments. Retry write-read up to about 5–10 times with growing delay. Keep a per-monitor learned multiplier (persisted) as ddcutil does. Validate every reply (sender byte 0x6E, length bit, checksum, echoed VCP code) and treat a checksum failure as retryable.
- Bus selection on MST: prefer the bus whose EDID read and DDC probe both succeed. If two buses return the same EDID, probe 0x37 with a cheap Get VCP 0x10 and keep the one that answers.
- After resume or hotplug, re-enumerate rather than trusting cached bus numbers. `/dev/i2c-N` numbering can change across driver reloads and dock reconnects. **[unverified, but consistent with ddcutil re-detection behaviour]**
- Laptop internal panels (eDP) do not do DDC/CI. Use `/sys/class/backlight` for those (out of scope).

### Gaps
- Status of the NVIDIA open kernel modules (nvidia-open, default for Turing+ since driver 560) on DDC/I2C: not verified whether the `RMUseSwI2c` workaround is still needed or applies.
- Intel `xe` driver (Lunar Lake+) DDC behaviour: not verified.
- USB-C DP Alt Mode without MST (single monitor through a USB-C cable) generally behaves like native DP, but this was not confirmed by a source in this session.

---

## Q5. How desktop tools do it: ddcui, GNOME extensions, KDE PowerDevil (bus contention)

### Takeaway
Nearly every Linux tool is built on ddcutil. KDE Plasma 6's PowerDevil links libddcutil and controls external monitor brightness by default, so it competes with our app for the same bus and brightness value. GNOME has no built-in DDC; the popular extension shells out to the ddcutil CLI. Our app should detect PowerDevil and warn, or offer to coexist.

### Cited Findings
- KDE Plasma 6 PowerDevil uses libddcutil for external monitor brightness. Plasma 6.2 added multi-screen brightness management, which brought unintended behaviours: PowerDevil initially works, then repeatedly fails to set brightness and eventually drops the display. — [KDE Discuss: brightness slider missing](https://discuss.kde.org/t/brightness-slider-missing-in-plasma-when-powerdevil-has-no-ddc-or-backlight-backend/48004); [Manjaro forum: Plasma 6.2.4](https://forum.manjaro.org/t/monitor-brightness-settings-vanished-not-working-in-plasma-6-2-4/171159) (search-snippet level)
- To disable PowerDevil's DDC: `systemctl --user edit plasma-powerdevil.service`, add `[Service]` `Environment=POWERDEVIL_NO_DDCUTIL=1`, then log out and back in. The ACPI backlight for laptop panels keeps working. — [KDE Discuss: disable KDE monitor brightness control](https://discuss.kde.org/t/disable-kde-monitor-brightness-control/25134)
- History: Arch disabled ddcutil in powerdevil 5.27.5-3 because the applet showed only one brightness slider, ddcutil init caused a service timeout at login, and brightness was not restored after resume. "No longer considered experimental and is enabled by default for Plasma 6." — [Arch FS#78536](https://bugs.archlinux.org/task/78536)
- GNOME "Display Brightness Control using ddcutil" spawns the `ddcutil` CLI (`detect`, `getvcp 10`, `setvcp 10 N`), caches detection in `$XDG_CACHE_HOME/ddcutil_detect`, and was ported to GNOME 45+. — [daitj/gnome-display-brightness-ddcutil](https://github.com/daitj/gnome-display-brightness-ddcutil)
- Other GUI tools mentioned by the community: `gddccontrol` (ddccontrol) and `ddcui` (ddcutil's Qt GUI). — [KDE Discuss thread](https://discuss.kde.org/t/disable-kde-monitor-brightness-control/25134); [ddcutil GitHub](https://github.com/rockowitz/ddcutil)

### Inferences
- **Contention risk:** two processes interleaving DDC transactions on one bus corrupts replies (bad checksums, wrong opcodes), because DDC/CI is stateful between write and read. The kernel serializes individual ioctls or read/write calls but not the write→sleep→read pair. Mitigations: (a) detect a running `org_kde_powerdevil` process, or the PowerDevil DBus brightness interface, and offer the `POWERDEVIL_NO_DDCUTIL=1` fix; (b) on KDE, optionally drive brightness through PowerDevil's DBus API instead of raw DDC; (c) retry on checksum failure. **[unverified]** The exact PowerDevil DBus brightness interface name was not researched.
- Also expect PowerDevil to overwrite brightness (dimming on idle, restoring after resume). That looks like "our setting didn't stick" to users.
- An advisory lock file (e.g. `flock` on `/run/user/$UID/ddc-bus-N.lock`) only coordinates with cooperating tools. ddcutil does not use our lock. **[unverified]** ddcutil 2.x has its own cross-process locking (`--enable-flock`); if so, taking the same `flock` on the `/dev/i2c-N` fd itself would interoperate. Worth checking in the ddcutil source.

### Gaps
- Whether ddcutil's `flock` option exists in 2.x and what it locks (the device fd vs a lock file) was not verified.
- No GNOME core (mutter/gnome-settings-daemon) DDC support was found as of this research. This is absence of evidence, not confirmed.

---

## Q6. Detecting monitor hotplug on Linux from .NET

### Takeaway
Listen for kernel uevents on the `drm` subsystem (`ACTION=change` with `HOTPLUG=1`) via a netlink `NETLINK_KOBJECT_UEVENT` socket or libudev, then debounce and re-enumerate buses and EDIDs. Polling `/sys/class/drm/*/status` every few seconds is a simple, robust fallback.

### Cited Findings
- I found no primary source fetched this session that documents drm uevent payloads. Everything below is **[unverified]** and comes from kernel knowledge:
  - On a connector hotplug the DRM core emits a uevent on the card device (`/devices/.../drm/card0`) with `ACTION=change`, `SUBSYSTEM=drm`, `HOTPLUG=1`, and on newer kernels `CONNECTOR=<id>` and `PROPERTY=<id>`. MST hotplugs also add or remove `card0-DP-N` connector devices (`ACTION=add`/`remove`).
  - I2C adapter nodes appear and disappear as `SUBSYSTEM=i2c-dev` add/remove events (relevant for docks and MST).
  - Sysfs attributes are not inotify-watchable (no change notifications on sysfs reads), so file watchers on `/sys/class/drm/*/status` do not work. Use uevents or polling.
- ddcutil's own display-change detection page (`ddcutil.com/display_notification/`) returned 404 at fetch time, so its approach (udev vs polling) could not be confirmed.

### Inferences
- **Option A, raw netlink in managed code (no native dependency):** create a socket with `socket(AF_NETLINK=16, SOCK_DGRAM|SOCK_CLOEXEC, NETLINK_KOBJECT_UEVENT=15)`, bind with `sockaddr_nl { nl_family=16, nl_pid=0, nl_groups=1 }` (1 = the kernel uevent multicast group), and read datagrams. Each one is `ACTION@DEVPATH\0KEY=VALUE\0...`. Do it via libc P/Invoke (`socket`, `bind`, `recv`) on a background thread. **[unverified]** Whether `System.Net.Sockets.Socket` with `AddressFamily.Netlink` can bind netlink portably on .NET 10 is not confirmed, so the P/Invoke path is safer. Note: listening to group 1 receives raw kernel events before udev has processed them (device nodes or permissions may not be ready yet), so wait about 1–2 s or retry `open()` on EACCES/ENOENT.
- **Option B, libudev P/Invoke:** `udev_new` → `udev_monitor_new_from_netlink(udev, "udev")` → `udev_monitor_filter_add_match_subsystem_devtype(mon, "drm", NULL)` (and `"i2c-dev"`) → `udev_monitor_enable_receiving` → poll `udev_monitor_get_fd` → `udev_monitor_receive_device`. This gets post-udev events, so the uaccess ACLs are already applied. `libudev.so.1` is LGPL-2.1 (part of systemd) and present on all systemd distros. **[unverified licensing detail]**
- **Option C, polling:** every 2–5 s, read `/sys/class/drm/card*-*/status` plus a hash of `edid`, and diff. This is cheap, has no dependencies, and is a fine fallback or the primary path if event plumbing is deferred.
- On any change: debounce about 1–2 s (link training, MST enumeration and udev rule application all take time), invalidate the bus↔monitor map, re-scan EDIDs, and re-probe DDC. Also re-scan on resume. Use logind's `PrepareForSleep(false)` DBus signal on `org.freedesktop.login1.Manager`. **[unverified]** This was not researched further here.

### Gaps
- The exact uevent keys per kernel version, and whether NVIDIA proprietary emits `drm` hotplug uevents identically, were not verified.
- `AddressFamily.Netlink` support in .NET 10 `Socket` was not verified.
