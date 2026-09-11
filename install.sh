#!/bin/sh
# axur installer
#   curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | sh
#
# Downloads the release binary for this platform and verifies it against the
# SHA256SUMS published with the release before installing. An unverifiable
# download is discarded: this script runs with sudo, so a silent mismatch would
# be a privileged install of an unknown file.

set -eu

REPO="ahmed6ww/ax"
BIN="axur"

RED='\033[0;31m'; GREEN='\033[0;32m'; CYAN='\033[0;36m'; DIM='\033[2m'; NC='\033[0m'

say()  { printf '%s%s%s\n' "$CYAN" "$1" "$NC"; }
ok()   { printf '%s%s%s\n' "$GREEN" "$1" "$NC"; }
dim()  { printf '%s%s%s\n' "$DIM" "$1" "$NC"; }
die()  { printf '%s%s%s\n' "$RED" "$1" "$NC" >&2; exit 1; }

need() {
  command -v "$1" >/dev/null 2>&1 || die "Required command not found: $1"
}

need curl

OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS-$ARCH" in
  linux-x86_64)           ASSET="axur-linux-x64" ;;
  linux-aarch64|linux-arm64) ASSET="axur-linux-arm64" ;;
  darwin-x86_64)          ASSET="axur-macos-x64" ;;
  darwin-arm64)           ASSET="axur-macos-arm64" ;;
  *)
    printf '%sUnsupported platform: %s-%s%s\n' "$RED" "$OS" "$ARCH" "$NC" >&2
    echo "On Windows, use install.ps1. Otherwise: cargo install axur" >&2
    exit 1
    ;;
esac

say "→ Platform: $OS-$ARCH"

# Resolve the latest tag. Failing closed matters: the previous version fell back
# to a hardcoded old version, so a rate-limited API quietly installed something
# other than what the user asked for.
say "→ Resolving latest release"
VERSION=$(curl -fsSL "https://api.github.com/repos/$REPO/releases/latest" \
  | grep '"tag_name"' | head -1 | sed -E 's/.*"([^"]+)".*/\1/') || true

[ -n "${VERSION:-}" ] || die "Could not determine the latest release. Set AXUR_VERSION to install a specific tag."
VERSION="${AXUR_VERSION:-$VERSION}"

say "→ Installing $BIN $VERSION"

BASE="https://github.com/$REPO/releases/download/$VERSION"
TMP=$(mktemp -d)
trap 'rm -rf "$TMP"' EXIT INT TERM

curl -fsSL "$BASE/$ASSET"     -o "$TMP/$ASSET"     || die "Download failed: $BASE/$ASSET"
curl -fsSL "$BASE/SHA256SUMS" -o "$TMP/SHA256SUMS" || die "Could not fetch SHA256SUMS — refusing to install unverified."

# Verify before anything is made executable or moved into place.
say "→ Verifying checksum"
EXPECTED=$(grep " \{1,2\}\*\{0,1\}$ASSET\$" "$TMP/SHA256SUMS" | awk '{print $1}' | head -1)
[ -n "$EXPECTED" ] || die "$ASSET is not listed in SHA256SUMS — refusing to install."

if command -v sha256sum >/dev/null 2>&1; then
  ACTUAL=$(sha256sum "$TMP/$ASSET" | awk '{print $1}')
elif command -v shasum >/dev/null 2>&1; then
  ACTUAL=$(shasum -a 256 "$TMP/$ASSET" | awk '{print $1}')
else
  die "Neither sha256sum nor shasum is available — cannot verify the download."
fi

if [ "$EXPECTED" != "$ACTUAL" ]; then
  printf '%sChecksum mismatch — refusing to install.%s\n' "$RED" "$NC" >&2
  echo "  expected $EXPECTED" >&2
  echo "  actual   $ACTUAL" >&2
  exit 1
fi
ok "✓ Checksum verified"
dim "  $ACTUAL"

chmod +x "$TMP/$ASSET"

if [ -w /usr/local/bin ]; then
  DEST=/usr/local/bin
  mv "$TMP/$ASSET" "$DEST/$BIN"
elif command -v sudo >/dev/null 2>&1; then
  DEST=/usr/local/bin
  sudo mv "$TMP/$ASSET" "$DEST/$BIN"
else
  DEST="$HOME/.local/bin"
  mkdir -p "$DEST"
  mv "$TMP/$ASSET" "$DEST/$BIN"
fi

ok "✓ Installed to $DEST/$BIN"

case ":$PATH:" in
  *":$DEST:"*) ;;
  *) echo; dim "$DEST is not on your PATH. Add it:"; echo "  export PATH=\"$DEST:\$PATH\"" ;;
esac

echo
ok "axur $VERSION installed"
echo
echo "  axur init                                    set up this project"
echo "  axur install vercel-labs/skills#skills/find-skills"
echo "  axur sync                                    install what the manifest declares"
echo
dim "Verify what it authorised to run:  axur audit"
