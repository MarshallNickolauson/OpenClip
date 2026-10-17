#!/bin/bash
# OpenClip installer for macOS.
#   curl -fsSL https://getopenclip.pages.dev/install.sh | bash
#
# Downloads the latest OpenClip disk image with curl (so macOS does not tag it as "downloaded from
# the internet"), copies the app to Applications and opens it. Read it first if you like: it only
# downloads the disk image named in https://getopenclip.pages.dev/latest.json and copies the app out of it.
set -euo pipefail

SITE="https://getopenclip.pages.dev"

if [ "$(uname -s)" != "Darwin" ]; then
  echo "OpenClip is macOS only for now." >&2
  exit 1
fi

# the manifest names the exact, versioned file for the newest release
URL="$(curl -fsSL -m 10 "$SITE/latest.json" 2>/dev/null | sed -n 's/.*"url": *"\([^"]*\)".*/\1/p' | head -1 || true)"
[ -n "$URL" ] || { echo "Could not reach the OpenClip website. Check your connection and try again." >&2; exit 1; }

tmp="$(mktemp -d)"
mnt=""
cleanup() {
  [ -n "$mnt" ] && hdiutil detach -quiet "$mnt" 2>/dev/null || true
  rm -rf "$tmp"
}
trap cleanup EXIT

echo "Downloading OpenClip..."
curl -fL --progress-bar -o "$tmp/OpenClip.dmg" "$URL" || { echo "Could not download OpenClip from $URL" >&2; exit 1; }

mnt="$(hdiutil attach -nobrowse -readonly -noautoopen "$tmp/OpenClip.dmg" | awk -F'\t' '/\/Volumes\//{print $NF}' | tail -1)"
[ -d "$mnt/OpenClip.app" ] || { echo "The disk image did not contain OpenClip.app." >&2; exit 1; }

# quit a running copy so it can be replaced
osascript -e 'tell application "OpenClip" to quit' >/dev/null 2>&1 || true
sleep 1

dest="/Applications"
[ -w "$dest" ] || { dest="$HOME/Applications"; mkdir -p "$dest"; }
rm -rf "$dest/OpenClip.app"
cp -R "$mnt/OpenClip.app" "$dest/"
xattr -dr com.apple.quarantine "$dest/OpenClip.app" 2>/dev/null || true

echo "Installed to $dest/OpenClip.app"
open "$dest/OpenClip.app"
