#!/usr/bin/env bash
# Build build/thinkvision-control-<version>-linux-x64.tar.gz: the app binary, launcher entry, icon, udev rule,
# licences and an install.sh that installs for the current user (~/.local).
#
#   ./scripts/make-linux-tarball.sh
#
# Needs Rust (rustup.rs). Run it on x64 Linux; from another OS, cargo needs a linker for x86_64-unknown-linux-gnu.
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
cargo build --release -p tvc-app --target "$TARGET"

echo "==> Assembling $STAGE"
rm -rf "$STAGE"
mkdir -p "$STAGE"
cp "target/$TARGET/release/$EXE_NAME" "$STAGE/"
cp packaging/linux/thinkvision-control.desktop packaging/linux/60-thinkvision-i2c.rules LICENSE \
   crates/tvc-app/ui/fonts/LICENSE-FluentSystemIcons.txt "$STAGE/"
cp assets/brand/svg/app-icon.svg "$STAGE/thinkvision-control.svg"

cat > "$STAGE/install.sh" <<'INSTALL'
#!/bin/sh
# Install ThinkVision Control for the current user (no root needed). Run it again to update.
set -eu
cd "$(dirname "$0")"
dest="$HOME/.local/share/thinkvision-control"
mkdir -p "$dest" "$HOME/.local/bin" "$HOME/.local/share/applications" "$HOME/.local/share/icons/hicolor/scalable/apps"
rm -f "$dest/thinkvision-control"   # replace, not overwrite: the old binary may still be running
cp thinkvision-control "$dest/"
chmod 755 "$dest/thinkvision-control"
ln -sf "$dest/thinkvision-control" "$HOME/.local/bin/thinkvision-control"
cp thinkvision-control.desktop "$HOME/.local/share/applications/"
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
echo "To allow access to the monitor (once), run these, then log out and back in:"
echo "  sudo modprobe i2c-dev"
echo "  echo i2c-dev | sudo tee /etc/modules-load.d/i2c-dev.conf"
echo "  sudo cp \"$PWD/60-thinkvision-i2c.rules\" /etc/udev/rules.d/"
echo "  sudo udevadm control --reload && sudo udevadm trigger"
INSTALL
chmod 755 "$STAGE/install.sh" "$STAGE/$EXE_NAME"

tar -C build --owner=0 --group=0 -czf "build/$NAME.tar.gz" "$NAME"
echo
echo "Built: build/$NAME.tar.gz"
echo "Install: tar xzf $NAME.tar.gz && ./$NAME/install.sh"
