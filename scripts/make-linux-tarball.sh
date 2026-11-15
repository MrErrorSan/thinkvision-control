#!/usr/bin/env bash
# Build build/thinkvision-control-<version>-linux-x64.tar.gz: the app binary, launcher entry, icon, udev rule,
# licences, third-party notices, and install.sh / uninstall.sh for the current user (~/.local).
#
#   ./scripts/make-linux-tarball.sh
#
# Needs Rust (rustup.rs). Run it on x64 Linux; from another OS, cargo needs a linker for x86_64-unknown-linux-gnu.
# THIRD-PARTY-NOTICES.txt needs cargo-about (cargo install cargo-about --locked --features cli); a local build without it leaves the
# file out, CI fails instead.
set -euo pipefail

TARGET="x86_64-unknown-linux-gnu"
EXE_NAME="thinkvision-control"

# Run from the repository root, wherever the script is called from.
cd "$(dirname "$0")/.."
PATH="$HOME/.cargo/bin:$PATH"

VERSION=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$VERSION" ] || { echo "error: no version under [workspace.package] in Cargo.toml" >&2; exit 1; }
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not found. Install Rust from https://rustup.rs" >&2; exit 1; }

NAME="$EXE_NAME-$VERSION-linux-x64"
STAGE="build/$NAME"

echo "==> Building (release, $TARGET)"
cargo build --release --locked -p tvc-app --target "$TARGET"

echo "==> Third-party notices"
mkdir -p build
rm -f build/THIRD-PARTY-NOTICES.txt
if cargo about --version >/dev/null 2>&1; then
    cargo about generate --fail -o build/THIRD-PARTY-NOTICES.txt about.hbs
elif [ -n "${CI:-}" ]; then
    echo "error: cargo-about not found" >&2
    exit 1
else
    echo "warning: cargo-about not found; the tarball has no THIRD-PARTY-NOTICES.txt" >&2
fi

echo "==> Assembling $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "target/$TARGET/release/$EXE_NAME" "$STAGE/"
cp packaging/linux/thinkvision-control.desktop packaging/linux/60-thinkvision-i2c.rules packaging/linux/uninstall.sh \
   LICENSE LICENSE-GPL-3.0.txt crates/tvc-app/ui/fonts/LICENSE-FluentSystemIcons.txt "$STAGE/"
[ ! -e build/THIRD-PARTY-NOTICES.txt ] || cp build/THIRD-PARTY-NOTICES.txt "$STAGE/"
cp assets/brand/svg/app-icon.svg "$STAGE/thinkvision-control.svg"

cat > "$STAGE/install.sh" <<'INSTALL'
#!/bin/sh
# Install ThinkVision Control for the current user (no root needed). Run it again to update.
# To remove it: ~/.local/share/thinkvision-control/uninstall.sh
set -eu
cd "$(dirname "$0")"
dest="$HOME/.local/share/thinkvision-control"
mkdir -p "$dest" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/scalable/apps"
rm -f "$dest/thinkvision-control"   # replace, not overwrite: the old binary may still be running
cp thinkvision-control uninstall.sh "$dest/"
chmod 755 "$dest/thinkvision-control" "$dest/uninstall.sh"
ln -sf "$dest/thinkvision-control" "$HOME/.local/bin/thinkvision-control"
# An absolute Exec: the session's PATH may not include ~/.local/bin.
sed "s|^Exec=thinkvision-control|Exec=\"$dest/thinkvision-control\"|" thinkvision-control.desktop \
    > "$HOME/.local/share/applications/thinkvision-control.desktop"
cp thinkvision-control.svg "$HOME/.local/share/icons/hicolor/scalable/apps/"
if command -v update-desktop-database >/dev/null 2>&1; then
    update-desktop-database "$HOME/.local/share/applications" || true
fi
# thinkvision:// links (thinkvision://preset/night, ...) open the app.
if command -v xdg-mime >/dev/null 2>&1; then
    xdg-mime default thinkvision-control.desktop x-scheme-handler/thinkvision || true
fi

echo "Installed. Start \"ThinkVision Control\" from your app launcher, or run: thinkvision-control"
case ":$PATH:" in *":$HOME/.local/bin:"*) ;; *) echo "Note: ~/.local/bin is not on your PATH." ;; esac
echo
# Monitor access: the i2c-dev module and a udev rule that gives the logged-in user /dev/i2c-*. Set up once, and
# again when a newer version brings a changed rule.
rules=/etc/udev/rules.d/60-thinkvision-i2c.rules
trigger="udevadm trigger --action=add --subsystem-match=i2c-dev --subsystem-match=backlight"
if cmp -s 60-thinkvision-i2c.rules "$rules" && [ -e /etc/modules-load.d/i2c-dev.conf ]; then
    echo "Monitor access is already set up."
    exit 0
fi
if [ -e "$rules" ]; then
    echo "This version updates the monitor access rule ($rules). That needs sudo."
else
    echo "The app needs access to the monitor's control channel (/dev/i2c-*). This is set up once, with sudo."
fi
answer=n   # without a terminal (piped, scripted), print the commands instead of running sudo
if [ -t 0 ]; then
    printf "Set it up now? [Y/n] "
    read -r answer || answer=n
fi
case "$answer" in
    n|N|no|No)
        echo "Later, run:"
        echo "  sudo modprobe i2c-dev"
        echo "  echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf"
        echo "  sudo cp \"$PWD/60-thinkvision-i2c.rules\" /etc/udev/rules.d/"
        echo "  sudo udevadm control --reload && sudo $trigger"
        ;;
    *)
        sudo modprobe i2c-dev
        echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf >/dev/null
        sudo cp 60-thinkvision-i2c.rules /etc/udev/rules.d/
        sudo udevadm control --reload && sudo $trigger
        echo "Done. Log out and back in once, then start ThinkVision Control."
        ;;
esac
INSTALL
chmod 755 "$STAGE/install.sh" "$STAGE/uninstall.sh" "$STAGE/$EXE_NAME"

tar -C build --owner=0 --group=0 -czf "build/$NAME.tar.gz" "$NAME"
echo
echo "Built: build/$NAME.tar.gz"
echo "Install: tar xzf $NAME.tar.gz && ./$NAME/install.sh"
