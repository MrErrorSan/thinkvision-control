#!/usr/bin/env bash
# Build "ThinkVision Control.app" (a menu-bar app, no Dock icon) for Apple Silicon.
#
#   ./scripts/make-macos-app.sh
#
# Output: build/ThinkVision Control.app  (copy it to /Applications)
# Needs: Rust (rustup.rs, plus: rustup target add aarch64-apple-darwin) and the Xcode command
# line tools for codesign (xcode-select --install). Written for the bash 3.2 that ships with macOS.
set -euo pipefail

APP_NAME="ThinkVision Control"
BUNDLE_ID="com.thinkvisioncontrol.app"
TARGET="aarch64-apple-darwin"
EXE_NAME="thinkvision-control"            # [[bin]] name in crates/tvc-app/Cargo.toml
ICON_SRC="assets/brand/icns/ThinkVisionControl.icns"
ICON_NAME="ThinkVisionControl.icns"

# Run from the repository root, wherever the script is called from.
cd "$(dirname "$0")/.."
PATH="$HOME/.cargo/bin:$PATH"

VERSION=$(sed -n '/^\[workspace\.package\]/,/^\[/s/^version *= *"\(.*\)"/\1/p' Cargo.toml)
[ -n "$VERSION" ] || { echo "error: no version under [workspace.package] in Cargo.toml" >&2; exit 1; }

if [ "$(uname -s)" != "Darwin" ]; then
    echo "error: this script builds a macOS app bundle; run it on a Mac." >&2
    exit 1
fi
command -v cargo >/dev/null 2>&1 || { echo "error: cargo not found. Install Rust from https://rustup.rs" >&2; exit 1; }
command -v codesign >/dev/null 2>&1 || { echo "error: codesign not found. Install the tools with: xcode-select --install" >&2; exit 1; }

EXE_PATH="target/$TARGET/release/$EXE_NAME"
APP_DIR="build/$APP_NAME.app"

echo "==> Building (release, $TARGET)"
cargo build --release -p tvc-app --target "$TARGET"

echo "==> Assembling $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp "$EXE_PATH" "$APP_DIR/Contents/MacOS/"
cp "$ICON_SRC" "$APP_DIR/Contents/Resources/$ICON_NAME"
cp LICENSE crates/tvc-app/ui/fonts/LICENSE-FluentSystemIcons.txt "$APP_DIR/Contents/Resources/"
chmod +x "$APP_DIR/Contents/MacOS/$EXE_NAME"

# LSUIElement = menu-bar app: no Dock icon and no app menu, from the first frame of launch.
cat > "$APP_DIR/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key>
    <string>$APP_NAME</string>
    <key>CFBundleDisplayName</key>
    <string>$APP_NAME</string>
    <key>CFBundleIdentifier</key>
    <string>$BUNDLE_ID</string>
    <key>CFBundleExecutable</key>
    <string>$EXE_NAME</string>
    <key>CFBundleIconFile</key>
    <string>$ICON_NAME</string>
    <key>CFBundlePackageType</key>
    <string>APPL</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleShortVersionString</key>
    <string>$VERSION</string>
    <key>CFBundleVersion</key>
    <string>$VERSION</string>
    <key>LSMinimumSystemVersion</key>
    <string>13.0</string>
    <key>LSUIElement</key>
    <true/>
    <key>NSHighResolutionCapable</key>
    <true/>
    <key>CFBundleURLTypes</key>
    <array>
        <dict>
            <key>CFBundleURLName</key>
            <string>ThinkVision Control link</string>
            <key>CFBundleURLSchemes</key>
            <array>
                <string>thinkvision</string>
            </array>
        </dict>
    </array>
</dict>
</plist>
PLIST

# Ad-hoc signature ("-s -" = no certificate). Apple Silicon refuses to run arm64 code that
# carries no signature at all. The linker signs the bare binary; the bundle itself (Info.plist,
# icon) also needs a signature, so the whole bundle is signed here (--deep).
# A locally built app has no quarantine attribute, so Gatekeeper does not prompt; no
# Developer ID or notarization is needed for your own machine.
echo "==> Signing (ad-hoc)"
codesign --force --deep -s - "$APP_DIR"

echo "==> Installing to /Applications"
ditto "$APP_DIR" "/Applications/$APP_NAME.app"
open "/Applications/$APP_NAME.app"
echo "Installed and started: /Applications/$APP_NAME.app (look for it in the menu bar)"
