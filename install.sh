#!/bin/sh
set -e

REPO="rahulhalder123-456/fcz"

echo "Installing fcz from GitHub Releases..."

# Detect OS and Architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

if [ "$OS" = "Linux" ] && [ "$ARCH" = "x86_64" ]; then
    ASSET="fcz-linux-x86_64"
elif [ "$OS" = "Darwin" ] && [ "$ARCH" = "x86_64" ]; then
    ASSET="fcz-macos-x86_64"
elif [ "$OS" = "Darwin" ] && [ "$ARCH" = "arm64" ]; then
    ASSET="fcz-macos-aarch64"
else
    echo "Unsupported OS or Architecture: $OS $ARCH"
    echo "Please install via cargo: cargo install --git https://github.com/$REPO.git"
    exit 1
fi

# Get the latest release URL
LATEST_RELEASE_URL=$(curl -s "https://api.github.com/repos/$REPO/releases/latest" | grep "browser_download_url.*$ASSET\"" | cut -d : -f 2,3 | tr -d \")

if [ -z "$LATEST_RELEASE_URL" ]; then
    echo "Error: Could not find release asset $ASSET"
    exit 1
fi

# Download to a temporary location
TMP_DIR=$(mktemp -d)
curl -L "$LATEST_RELEASE_URL" -o "$TMP_DIR/fcz"
chmod +x "$TMP_DIR/fcz"

# Install to ~/.cargo/bin or /usr/local/bin
INSTALL_DIR="$HOME/.cargo/bin"
if [ ! -d "$INSTALL_DIR" ]; then
    INSTALL_DIR="/usr/local/bin"
    if [ ! -w "$INSTALL_DIR" ]; then
        echo "Requires sudo to install to /usr/local/bin"
        sudo mv "$TMP_DIR/fcz" "$INSTALL_DIR/fcz"
        echo "fcz has been installed to $INSTALL_DIR/fcz"
        exit 0
    fi
fi

mv "$TMP_DIR/fcz" "$INSTALL_DIR/fcz"
echo "fcz has been successfully installed to $INSTALL_DIR/fcz!"
echo "Make sure $INSTALL_DIR is in your PATH."
