#!/usr/bin/env bash
# Builds OpenClip in release mode and assembles a double-clickable OpenClip.app
# (the app plus the openclip-mcp helper Claude launches). Usage: ./scripts/bundle.sh [--install]
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p openclip -p openclip-core

APP="dist/OpenClip.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/openclip "$APP/Contents/MacOS/OpenClip"
cp target/release/openclip-mcp "$APP/Contents/MacOS/openclip-mcp"

# icon.icns from the 1024px master (see scripts/make-icon.swift)
ICONSET="$(mktemp -d)/OpenClip.iconset"
mkdir -p "$ICONSET"
MASTER=app/icons/icon-1024.png
for s in 16 32 128 256 512; do
  sips -z $s $s "$MASTER" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$MASTER" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/OpenClip.icns"

VERSION="$(sed -n 's/^version = "\(.*\)"/\1/p' Cargo.toml | head -1)"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>OpenClip</string>
  <key>CFBundleDisplayName</key><string>OpenClip</string>
  <key>CFBundleIdentifier</key><string>dev.openclip.app</string>
  <key>CFBundleExecutable</key><string>OpenClip</string>
  <key>CFBundleIconFile</key><string>OpenClip</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>__VERSION__</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSSupportsAutomaticGraphicsSwitching</key><true/>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>Video</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSItemContentTypes</key><array><string>public.mpeg-4</string><string>com.apple.quicktime-movie</string></array>
  </dict></array>
  <key>NSDesktopFolderUsageDescription</key><string>OpenClip reads the videos you choose.</string>
  <key>NSDocumentsFolderUsageDescription</key><string>OpenClip reads the videos you choose and saves your clips.</string>
  <key>NSDownloadsFolderUsageDescription</key><string>OpenClip reads the videos you choose.</string>
  <key>NSMoviesFolderUsageDescription</key><string>OpenClip saves your exported clips to Movies.</string>
</dict>
</plist>
PLIST
sed -i '' "s/__VERSION__/$VERSION/" "$APP/Contents/Info.plist"

# Stable designated requirement (bundle identifier only) so macOS keeps folder-access grants across rebuilds.
codesign --force --sign - --identifier dev.openclip.app.mcp "$APP/Contents/MacOS/openclip-mcp"
codesign --force --sign - --identifier dev.openclip.app \
  --requirements '=designated => identifier "dev.openclip.app"' "$APP"
codesign -d -r- "$APP" 2>&1 | grep designated || true

if [ "${1:-}" = "--install" ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/OpenClip.app"
  cp -R "$APP" "$HOME/Applications/OpenClip.app"
  echo "Installed ~/Applications/OpenClip.app"
fi
echo "Built $APP"
