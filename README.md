# ThinkVision Control

Controls a Lenovo ThinkVision P32p-20 from the computer instead of its buttons: brightness, contrast, input, colour mode, colour gains, day/night presets and more. It talks to the monitor over DDC/CI, from Windows, macOS (Apple Silicon) and Linux, and lives in the tray or menu bar with global hotkeys.

This is an unofficial project. It is not affiliated with or endorsed by Lenovo.

## Status

| App | Project | Runs on | Status |
|---|---|---|---|
| WPF app | `src/ThinkVisionControl` | Windows | Stable. The Windows app for now |
| Avalonia app | `src/ThinkVisionControl.Avalonia` | macOS, Linux; Windows as a preview | New |

Windows moves to the Avalonia app once it matches the WPF app on startup time, memory and looks. Until then, build the WPF app on Windows.

There are no installers. You build from source with the commands below.

## Requirements

- The [.NET 10 SDK](https://dotnet.microsoft.com/download/dotnet/10.0) (install steps per OS below) and `git`.
- The monitor connected directly (USB-C, DisplayPort or HDMI), with **DDC/CI enabled**. It is on by default. Holding the monitor's **Exit** key for 10 seconds toggles it, and the screen shows "DDC/CI disable" or "DDC/CI enable".

All commands below run from the repository folder after cloning. Replace `<you>` in the clone URL with the real account.

## Windows

```powershell
winget install Microsoft.DotNet.SDK.10
winget install Git.Git        # if you don't have git
git clone https://github.com/<you>/thinkvision-control.git
cd thinkvision-control
```

Run it:

```powershell
dotnet run --project src/ThinkVisionControl
```

Build a single `.exe` you can keep anywhere:

```powershell
dotnet publish src/ThinkVisionControl -c Release -r win-x64 --self-contained -p:PublishSingleFile=true -o publish
```

The result is `publish\ThinkVisionControl.exe`. Copy it where you like and start it. "Start with Windows" is in App settings.

Optional, to try the Avalonia preview on Windows:

```powershell
dotnet run --project src/ThinkVisionControl.Avalonia
```

## macOS (Apple Silicon)

```bash
brew install --cask dotnet-sdk
brew install git              # or use the git from: xcode-select --install
git clone https://github.com/<you>/thinkvision-control.git
cd thinkvision-control
```

Run it from the terminal:

```bash
dotnet run --project src/ThinkVisionControl.Avalonia
```

Build a menu-bar app (no Dock icon) and install it:

```bash
./scripts/make-macos-app.sh
ditto "build/ThinkVision Control.app" "/Applications/ThinkVision Control.app"
open "/Applications/ThinkVision Control.app"
```

- `codesign` comes with the Xcode command line tools (`xcode-select --install`). The script signs the app ad hoc, which Apple Silicon needs before it runs local builds.
- An app you built yourself is not quarantined, so Gatekeeper doesn't prompt and no Apple developer account is needed.
- Connect the monitor by **USB-C, Thunderbolt or DisplayPort**. The built-in HDMI port on M1 and base M2 Macs often can't carry DDC/CI.
- Starting at login is a switch in App settings.

## Linux (Ubuntu and Fedora)

### 1. Install the .NET 10 SDK and clone

```bash
sudo apt install dotnet-sdk-10.0 git      # Ubuntu
sudo dnf install dotnet-sdk-10.0 git      # Fedora
```

If your release doesn't have the `dotnet-sdk-10.0` package yet, use Microsoft's install script instead (no root needed):

```bash
curl -sSL https://dot.net/v1/dotnet-install.sh | bash -s -- --channel 10.0
echo 'export DOTNET_ROOT=$HOME/.dotnet; export PATH=$PATH:$HOME/.dotnet' >> ~/.bashrc
source ~/.bashrc
```

On Ubuntu, the `ppa:dotnet/backports` PPA is another source for newer SDKs.

```bash
git clone https://github.com/<you>/thinkvision-control.git
cd thinkvision-control
```

### 2. Allow access to the monitor (once)

The app talks to the monitor through `/dev/i2c-*`, which needs the `i2c-dev` kernel module and permission for your user.

```bash
sudo modprobe i2c-dev
echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf
sudo cp packaging/linux/60-thinkvision-i2c.rules /etc/udev/rules.d/
sudo udevadm control --reload && sudo udevadm trigger
```

Instead of copying the rule, you can install ddcutil, which ships an equivalent one: `sudo apt install ddcutil` or `sudo dnf install ddcutil`.

Log out and back in. The app's App settings page shows a "System setup" card with the fix if access is still missing.

### 3. Run or install

Run it:

```bash
dotnet run --project src/ThinkVisionControl.Avalonia
```

Install it for your user:

```bash
dotnet publish src/ThinkVisionControl.Avalonia -c Release -r linux-x64 --self-contained -o ~/.local/share/thinkvision-control
mkdir -p ~/.local/bin
ln -sf ~/.local/share/thinkvision-control/thinkvision-control ~/.local/bin/thinkvision-control
```

Then start it with `thinkvision-control`. If the shell can't find it, open a new terminal; `~/.local/bin` is added to `PATH` only when it exists at login.

Add it to the app launcher, with Day, Night and "Switch monitor to this computer" in its right-click menu:

```bash
mkdir -p ~/.local/share/applications ~/.local/share/icons/hicolor/scalable/apps
cp packaging/linux/thinkvision-control.desktop ~/.local/share/applications/
cp assets/brand/svg/app-icon.svg ~/.local/share/icons/hicolor/scalable/apps/thinkvision-control.svg
update-desktop-database ~/.local/share/applications
```

If the launcher entry does nothing, your desktop session doesn't have `~/.local/bin` on its `PATH`. Point the entry at the full path:

```bash
sed -i "s|^Exec=thinkvision-control|Exec=$HOME/.local/share/thinkvision-control/thinkvision-control|" ~/.local/share/applications/thinkvision-control.desktop
```

Starting at login is a switch in App settings. It writes `~/.config/autostart/thinkvision-control.desktop`.

### Desktop notes

- **GNOME tray icon:** GNOME needs the AppIndicator extension. Ubuntu has it already. On Fedora:

  ```bash
  sudo dnf install gnome-shell-extension-appindicator
  gnome-extensions enable appindicatorsupport@rgcjonas.gmail.com   # after logging out and back in
  ```

- **KDE Plasma 6:** Plasma's power manager (PowerDevil) also controls external monitor brightness over DDC/CI. The two fight over the same bus, and brightness changes don't stick. Turn off PowerDevil's DDC:

  ```bash
  systemctl --user edit plasma-powerdevil.service
  ```

  Add these lines in the editor, save, then restart it (or log out and back in):

  ```ini
  [Service]
  Environment=POWERDEVIL_NO_DDCUTIL=1
  ```

  ```bash
  systemctl --user restart plasma-powerdevil.service
  ```

  Laptop screen brightness keeps working.

- **NVIDIA proprietary driver:** on some cards I²C doesn't work until you add a driver option, then reboot:

  ```bash
  echo 'options nvidia NVreg_RegistryDwords=RMUseSwI2c=0x01;RMI2cSpeed=100' | sudo tee /etc/modprobe.d/nvidia-i2c.conf
  ```

  Check after the reboot with `grep RegistryDwords /proc/driver/nvidia/params`. AMD and Intel graphics need nothing extra.

- **Global hotkeys:** they work on X11 and on KDE Plasma (Wayland too, through the GlobalShortcuts portal; Plasma asks you to confirm them once). GNOME 48 and later also use the portal. Older GNOME on Wayland, such as Ubuntu 24.04, has no way for apps to register hotkeys, so bind desktop shortcuts to the commands below instead.
- **Default hotkeys:** `Ctrl+Alt+F1`–`F3` switch virtual consoles on Linux, and GNOME uses `Ctrl+Alt+arrows` for workspaces. Pick other keys for those actions in App settings → Keyboard shortcuts.

## Command line

The same options work on every OS. If the app is already running, the command is passed to it and the new process exits; otherwise it does the action and exits.

| Option | Does |
|---|---|
| `--preset day` / `--preset night` | Applies the Day or Night preset |
| `--input hdmi` / `dp` / `usbc` / `this` | Switches the monitor's input. `this` is the input chosen under "This PC" in App settings |
| `--step brightness=+5` / `--step contrast=-5` | Changes brightness or contrast by a step (-100 to +100) |
| `--startup` | Starts in the tray or menu bar with no window, as the login launch does |

The program to call:

| OS | Command |
|---|---|
| Windows | `publish\ThinkVisionControl.exe --preset night` |
| macOS | `"/Applications/ThinkVision Control.app/Contents/MacOS/thinkvision-control" --preset night` |
| Linux | `thinkvision-control --preset night` (after the install step above) |

### Desktop shortcuts on Linux

**GNOME:** Settings → Keyboard → View and Customise Shortcuts → Custom Shortcuts → `+`. Use the full path in the command, for example `/home/you/.local/bin/thinkvision-control --step brightness=+5`.

Or from a terminal. This adds to your existing custom shortcuts rather than replacing them:

```bash
add_shortcut() {   # id, command, keys
  local base=/org/gnome/settings-daemon/plugins/media-keys/custom-keybindings
  local path="$base/thinkvision-$1/"
  local list
  list=$(gsettings get org.gnome.settings-daemon.plugins.media-keys custom-keybindings)
  case "$list" in
    *"$path"*) ;;
    "@as []") list="['$path']" ;;
    *) list="${list%]}, '$path']" ;;
  esac
  gsettings set org.gnome.settings-daemon.plugins.media-keys custom-keybindings "$list"
  local schema="org.gnome.settings-daemon.plugins.media-keys.custom-keybinding:$path"
  gsettings set "$schema" name "ThinkVision: $1"
  gsettings set "$schema" command "$2"
  gsettings set "$schema" binding "$3"
}
TVC="$HOME/.local/bin/thinkvision-control"
add_shortcut brightness-up   "$TVC --step brightness=+5" '<Super><Alt>Up'
add_shortcut brightness-down "$TVC --step brightness=-5" '<Super><Alt>Down'
add_shortcut day             "$TVC --preset day"         '<Super><Alt>Home'
add_shortcut night           "$TVC --preset night"       '<Super><Alt>End'
add_shortcut switch-to-me    "$TVC --input this"         '<Super><Alt>m'
```

**KDE Plasma 6:** System Settings → Keyboard → Shortcuts → Add New → Command or Script. Enter the command, for example `thinkvision-control --step brightness=+5`, then set the key. You only need this if the app's own hotkeys are unavailable.

## Hardware test checklist

Copy into an issue or a note and tick as you go.

**Ubuntu under WSL (no monitor access)**
- [ ] Every page renders, in light and dark mode
- [ ] Not found and permission states show the right message and setup card
- [ ] No see-through window; icons visible

**Linux on the laptop (live USB, Ubuntu or Fedora), P32p-20 on HDMI**
- [ ] The monitor is found on the right I²C bus
- [ ] Reads and changes work (brightness, contrast, input, colour mode)
- [ ] The permission card shows before the udev rule, and disappears after it plus a re-login
- [ ] Unplug and replug: the app reconnects
- [ ] GNOME tray icon with the AppIndicator extension; the setup card without it
- [ ] Hotkeys work, or the shortcut setup card shows

**KDE Plasma (live USB or a KDE machine)**
- [ ] Tray icon and its menu
- [ ] Hotkeys through the portal
- [ ] The PowerDevil card shows while PowerDevil's DDC is on, and disappears after the override

**Apple Silicon Mac, monitor on USB-C or Thunderbolt**
- [ ] The monitor is found by vendor and model
- [ ] Reads and changes work; note which checksum mode the log reports
- [ ] Menu-bar item and its flyout; no Dock icon
- [ ] Hotkeys (no permission prompt)
- [ ] On-screen display on change
- [ ] Starting at login survives a restart
- [ ] After sleep and wake, controls still work

**Windows**
- [ ] Avalonia app: Mica on the main window
- [ ] Avalonia app: time to first window ≤ 0.6 s (WPF is about 0.5 s)
- [ ] Avalonia app: private memory after a login launch ≤ 24 MB
- [ ] Side-by-side screenshot review against the WPF app

## Troubleshooting

- **Monitor not found:** check the cable goes straight to the computer. Docks, hubs and KVMs often block DDC/CI, and DisplayLink adapters never pass it on any OS. On a Mac, use a USB-C or Thunderbolt port rather than HDMI.
- **DDC/CI is off:** if the monitor shows up but nothing changes, hold the monitor's **Exit** key for 10 seconds until it shows "DDC/CI enable". An accidental long press is the most common cause.
- **Permission denied (Linux):** redo step 2 of the Linux section, then log out and back in. `ls -l /dev/i2c-*` should list devices; if there are none, `i2c-dev` isn't loaded. `getfacl /dev/i2c-N` should list your user.
- **Brightness jumps back (KDE):** turn off PowerDevil's DDC as shown in Desktop notes.
- **Slow or flaky changes (Linux with NVIDIA):** add the NVIDIA driver option in Desktop notes.
- **More detail:** the app writes a log in its data folder: `%APPDATA%\ThinkVisionControl` on Windows, `~/.config/ThinkVisionControl` on macOS and Linux.

## Project layout

| Path | What it is |
|---|---|
| `src/ThinkVisionControl.Core` | Shared logic: control service, safety rules, monitor profile, settings, command line, DDC packet codec. No OS calls |
| `src/ThinkVisionControl.Ddc.Windows` | Windows monitor link (dxva2) |
| `src/ThinkVisionControl.Ddc.Linux` | Linux monitor link (`/dev/i2c-*`) |
| `src/ThinkVisionControl.Ddc.MacOS` | macOS monitor link (IOAVService) |
| `src/ThinkVisionControl` | Windows WPF app |
| `src/ThinkVisionControl.Avalonia` | Cross-platform Avalonia app |
| `tests/ThinkVisionControl.Tests` | Unit tests (`dotnet test tests/ThinkVisionControl.Tests`) |
| `scripts/make-macos-app.sh` | Builds the macOS `.app` bundle |
| `packaging/linux/` | udev rule and launcher entry |
| `assets/brand/` | App icon (SVG, PNG, ICO, ICNS) |
| `docs/` | Specs, plans, design mockups |
| `research_notes/`, `reports/` | Background research |

## Licences and credits

- The Avalonia app bundles the [Fluent UI System Icons](https://github.com/microsoft/fluentui-system-icons) font (`src/ThinkVisionControl.Avalonia/Assets/Fonts/`), © Microsoft, MIT licence.
- The macOS and Linux monitor code is our own. It was informed by [MonitorControl](https://github.com/MonitorControl/MonitorControl) (MIT), [m1ddc](https://github.com/waydabber/m1ddc) (MIT) and the [ddcutil](https://www.ddcutil.com/) documentation. No code is copied from GPL projects.
- Lenovo and ThinkVision are trademarks of Lenovo.
