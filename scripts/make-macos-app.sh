#!/usr/bin/env bash
# Build "ThinkVision Control.app" (a menu-bar app, no Dock icon) for Apple Silicon.
#
#   ./scripts/make-macos-app.sh
#
# Output: build/ThinkVision Control.app  (copy it to /Applications)
# Needs: the .NET 10 SDK (brew install --cask dotnet-sdk) and the Xcode command line
# tools for codesign (xcode-select --install). Written for the bash 3.2 that ships with macOS.
set -euo pipefail

APP_NAME="ThinkVision Control"
BUNDLE_ID="com.thinkvisioncontrol.app"
VERSION="0.0.2"
EXE_NAME="thinkvision-control"            # AssemblyName of src/ThinkVisionControl.Avalonia
ICON_SRC="assets/brand/icns/ThinkVisionControl.icns"
ICON_NAME="ThinkVisionControl.icns"

# Run from the repository root, wherever the script is called from.
cd "$(dirname "$0")/.."

if [ "$(uname -s)" != "Darwin" ]; then
    echo "error: this script builds a macOS app bundle; run it on a Mac." >&2
    exit 1
fi
command -v dotnet >/dev/null 2>&1 || { echo "error: dotnet not found. Install it with: brew install --cask dotnet-sdk" >&2; exit 1; }
command -v codesign >/dev/null 2>&1 || { echo "error: codesign not found. Install the tools with: xcode-select --install" >&2; exit 1; }

PUBLISH_DIR="build/publish-osx"
APP_DIR="build/$APP_NAME.app"

echo "==> Publishing (Release, osx-arm64, self-contained)"
rm -rf "$PUBLISH_DIR"
dotnet publish src/ThinkVisionControl.Avalonia -c Release -r osx-arm64 --self-contained -o "$PUBLISH_DIR"

if [ ! -f "$PUBLISH_DIR/$EXE_NAME" ]; then
    echo "error: $PUBLISH_DIR/$EXE_NAME was not produced. Check AssemblyName in the Avalonia project." >&2
    exit 1
fi

echo "==> Assembling $APP_DIR"
rm -rf "$APP_DIR"
mkdir -p "$APP_DIR/Contents/MacOS" "$APP_DIR/Contents/Resources"
cp -R "$PUBLISH_DIR/." "$APP_DIR/Contents/MacOS/"
cp "$ICON_SRC" "$APP_DIR/Contents/Resources/$ICON_NAME"
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
</dict>
</plist>
PLIST

# Ad-hoc signature ("-s -" = no certificate). Apple Silicon refuses to run arm64 code that
# carries no signature at all. The SDK only signs the app host; the native libraries from
# NuGet packages and the bundle itself (Info.plist, icon) also need a signature, so the whole
# bundle is signed here, nested files included (--deep).
# A locally built app has no quarantine attribute, so Gatekeeper does not prompt; no
# Developer ID or notarization is needed for your own machine.
echo "==> Signing (ad-hoc)"
codesign --force --deep -s - "$APP_DIR"

echo
echo "Built: $APP_DIR"
echo "Install: ditto \"$APP_DIR\" \"/Applications/$APP_NAME.app\""
echo "Start:   open \"/Applications/$APP_NAME.app\""
