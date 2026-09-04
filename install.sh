#!/bin/sh
set -eu

REPO="rahulhalder123-456/fc"
INSTALL_DIR="${FCZ_INSTALL_DIR:-$HOME/.local/bin}"
OS=$(uname -s)
ARCH=$(uname -m)

case "$OS:$ARCH" in
  Linux:x86_64|Linux:amd64) ASSET="fcz-linux-x86_64" ;;
  Darwin:x86_64|Darwin:amd64) ASSET="fcz-macos-x86_64" ;;
  Darwin:arm64|Darwin:aarch64) ASSET="fcz-macos-aarch64" ;;
  *)
    echo "Unsupported platform: $OS $ARCH" >&2
    echo "Source fallback: cargo install --git https://github.com/$REPO.git" >&2
    exit 1
    ;;
esac

command -v curl >/dev/null 2>&1 || { echo "curl is required." >&2; exit 1; }
TMP_DIR=$(mktemp -d 2>/dev/null || mktemp -d -t fcz-install)
trap 'rm -rf "$TMP_DIR"' EXIT HUP INT TERM

echo "Installing fcz from $REPO..."
API="https://api.github.com/repos/$REPO/releases/latest"
if ! RELEASE_JSON=$(curl -fsSL -H 'Accept: application/vnd.github+json' -H 'User-Agent: fcz-installer' "$API"); then
  echo "GitHub API request failed. Check your connection and that a release exists." >&2
  exit 1
fi
TAG=$(printf '%s' "$RELEASE_JSON" | sed -n 's/.*"tag_name"[[:space:]]*:[[:space:]]*"\([^"]*\)".*/\1/p' | head -n 1)
[ -n "$TAG" ] || { echo "The latest GitHub release has no tag." >&2; exit 1; }

BASE_URL="https://github.com/$REPO/releases/download/$TAG"
if ! curl -fL --retry 2 --connect-timeout 15 "$BASE_URL/$ASSET" -o "$TMP_DIR/fcz"; then
  echo "Release '$TAG' does not contain $ASSET." >&2
  echo "Source fallback: cargo install --git https://github.com/$REPO.git" >&2
  exit 1
fi
[ -s "$TMP_DIR/fcz" ] || { echo "Downloaded binary is empty." >&2; exit 1; }

if curl -fsL "$BASE_URL/SHA256SUMS" -o "$TMP_DIR/SHA256SUMS"; then
  EXPECTED=$(awk -v asset="$ASSET" '$2 == asset || $2 == "*" asset { print $1; exit }' "$TMP_DIR/SHA256SUMS")
  [ -n "$EXPECTED" ] || { echo "SHA256SUMS has no entry for $ASSET." >&2; exit 1; }
  if command -v sha256sum >/dev/null 2>&1; then
    ACTUAL=$(sha256sum "$TMP_DIR/fcz" | awk '{print $1}')
  elif command -v shasum >/dev/null 2>&1; then
    ACTUAL=$(shasum -a 256 "$TMP_DIR/fcz" | awk '{print $1}')
  else
    echo "A SHA-256 utility (sha256sum or shasum) is required for this release." >&2
    exit 1
  fi
  [ "$ACTUAL" = "$EXPECTED" ] || { echo "SHA-256 verification failed for $ASSET." >&2; exit 1; }
  echo "SHA-256 verified."
else
  echo "Warning: this release has no SHA256SUMS asset; checksum verification was skipped." >&2
fi

mkdir -p "$INSTALL_DIR"
chmod 755 "$TMP_DIR/fcz"
mv "$TMP_DIR/fcz" "$INSTALL_DIR/fcz"
"$INSTALL_DIR/fcz" --version

case ":$PATH:" in
  *":$INSTALL_DIR:"*) ;;
  *)
    echo "Add fcz to PATH, then restart your shell:"
    echo "  export PATH=\"$INSTALL_DIR:\$PATH\""
    ;;
esac
echo "Installed fcz to $INSTALL_DIR/fcz"
