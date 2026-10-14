#!/usr/bin/env bash
# Builds ClipCraft in release mode and assembles a double-clickable ClipCraft.app
# (the app plus the clipcraft-mcp helper Claude launches). Usage: ./scripts/bundle.sh [--install]
set -euo pipefail
cd "$(dirname "$0")/.."

cargo build --release -p clipcraft -p clipcraft-core

APP="dist/ClipCraft.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/clipcraft "$APP/Contents/MacOS/ClipCraft"
cp target/release/clipcraft-mcp "$APP/Contents/MacOS/clipcraft-mcp"

# icon.icns from the 1024px master (see scripts/make-icon.swift)
ICONSET="$(mktemp -d)/ClipCraft.iconset"
mkdir -p "$ICONSET"
MASTER=src-tauri/icons/icon-1024.png
for s in 16 32 128 256 512; do
  sips -z $s $s "$MASTER" --out "$ICONSET/icon_${s}x${s}.png" >/dev/null
  sips -z $((s * 2)) $((s * 2)) "$MASTER" --out "$ICONSET/icon_${s}x${s}@2x.png" >/dev/null
done
iconutil -c icns "$ICONSET" -o "$APP/Contents/Resources/ClipCraft.icns"

VERSION="$(sed -n 's/.*"version": *"\([^"]*\)".*/\1/p' src-tauri/tauri.conf.json | head -1)"

cat > "$APP/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>ClipCraft</string>
  <key>CFBundleDisplayName</key><string>ClipCraft</string>
  <key>CFBundleIdentifier</key><string>dev.clipcraft.app</string>
  <key>CFBundleExecutable</key><string>ClipCraft</string>
  <key>CFBundleIconFile</key><string>ClipCraft</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>__VERSION__</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>LSMinimumSystemVersion</key><string>13.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>CFBundleDocumentTypes</key>
  <array><dict>
    <key>CFBundleTypeName</key><string>Video</string>
    <key>CFBundleTypeRole</key><string>Viewer</string>
    <key>LSItemContentTypes</key><array><string>public.mpeg-4</string><string>com.apple.quicktime-movie</string></array>
  </dict></array>
  <key>NSDesktopFolderUsageDescription</key><string>ClipCraft reads the videos you choose.</string>
  <key>NSDocumentsFolderUsageDescription</key><string>ClipCraft reads the videos you choose and saves your clips.</string>
  <key>NSDownloadsFolderUsageDescription</key><string>ClipCraft reads the videos you choose.</string>
  <key>NSMoviesFolderUsageDescription</key><string>ClipCraft saves your exported clips to Movies.</string>
</dict>
</plist>
PLIST
sed -i '' "s/__VERSION__/$VERSION/" "$APP/Contents/Info.plist"

# Stable designated requirement (bundle identifier only) so macOS keeps folder-access grants across rebuilds.
codesign --force --sign - --identifier dev.clipcraft.app.mcp "$APP/Contents/MacOS/clipcraft-mcp"
codesign --force --sign - --identifier dev.clipcraft.app \
  --requirements '=designated => identifier "dev.clipcraft.app"' "$APP"
codesign -d -r- "$APP" 2>&1 | grep designated || true

if [ "${1:-}" = "--install" ]; then
  mkdir -p "$HOME/Applications"
  rm -rf "$HOME/Applications/ClipCraft.app"
  cp -R "$APP" "$HOME/Applications/ClipCraft.app"
  echo "Installed ~/Applications/ClipCraft.app"
fi
echo "Built $APP"
