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
# thinkvision:// links (thinkvision://preset/night, ...) open the app. Without xdg-utils, the same line goes
# straight into mimeapps.list.
if command -v xdg-mime >/dev/null 2>&1; then
    xdg-mime default thinkvision-control.desktop x-scheme-handler/thinkvision || true
else
    apps="${XDG_CONFIG_HOME:-$HOME/.config}/mimeapps.list"
    handler="x-scheme-handler/thinkvision=thinkvision-control.desktop"
    if ! grep -qs '^x-scheme-handler/thinkvision=' "$apps"; then
        if grep -qs '^\[Default Applications\]' "$apps"; then
            sed -i "/^\[Default Applications\]/a $handler" "$apps"
        else
            mkdir -p "$(dirname "$apps")"
            printf '[Default Applications]\n%s\n' "$handler" >> "$apps"
        fi
    fi
fi
# KDE reads launchers from its own cache.
if command -v kbuildsycoca6 >/dev/null 2>&1; then
    kbuildsycoca6 >/dev/null 2>&1 || true
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
    echo "This version updates the monitor access rule ($rules). That needs administrator rights."
else
    echo "The app needs access to the monitor's control channel (/dev/i2c-*). This is set up once, with"
    echo "administrator rights."
fi
# One root script, run with sudo, else pkexec (the desktop's password dialog, or a prompt in this terminal);
# the rule file comes in as $1.
setup='set -e
modprobe i2c-dev
echo i2c-dev > /etc/modules-load.d/i2c-dev.conf
cp "$1" /etc/udev/rules.d/60-thinkvision-i2c.rules
udevadm control --reload
udevadm trigger --action=add --subsystem-match=i2c-dev --subsystem-match=backlight'
later() {
    echo "  su -c 'modprobe i2c-dev && echo i2c-dev > /etc/modules-load.d/i2c-dev.conf && cp \"$PWD/60-thinkvision-i2c.rules\" /etc/udev/rules.d/ && udevadm control --reload && $trigger'"
    echo "(or the same with sudo sh -c in place of su -c)"
}
answer=n   # without a terminal (piped, scripted), print the commands instead of asking for a password
if [ -t 0 ]; then
    printf "Set it up now? It asks for your password (on openSUSE, and on Debian with a root account, root's). [Y/n] "
    read -r answer || answer=n
fi
case "$answer" in
    n|N|no|No)
        echo "Later, run:"
        later
        ;;
    *)
        ok=no
        if command -v sudo >/dev/null 2>&1; then
            sudo sh -c "$setup" sh "$PWD/60-thinkvision-i2c.rules" && ok=yes
        fi
        if [ "$ok" = no ] && command -v pkexec >/dev/null 2>&1; then
            if command -v sudo >/dev/null 2>&1; then echo "sudo didn't work; trying pkexec instead."; fi
            pkexec /bin/sh -c "$setup" sh "$PWD/60-thinkvision-i2c.rules" && ok=yes
        fi
        if [ "$ok" = yes ]; then
            echo "Done. Start ThinkVision Control."
        else
            echo "Couldn't get administrator rights (neither sudo nor pkexec worked). As root, run:"
            later
        fi
        ;;
esac
INSTALL
chmod 755 "$STAGE/install.sh" "$STAGE/uninstall.sh" "$STAGE/$EXE_NAME"

tar -C build --owner=0 --group=0 -czf "build/$NAME.tar.gz" "$NAME"
echo
echo "Built: build/$NAME.tar.gz"
echo "Install: tar xzf $NAME.tar.gz && ./$NAME/install.sh"
