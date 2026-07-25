#!/bin/sh
set -eu

REPO="kannandreams/tuff"
BIN="tuff"
INSTALL_DIR="/usr/local/bin"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$ARCH" in
    x86_64)  TARGET="x86_64-apple-darwin" ;;
    arm64|aarch64) TARGET="aarch64-apple-darwin" ;;
    *)       echo "Unsupported architecture: $ARCH" && exit 1 ;;
esac

case "$OS" in
    darwin)  ;;
    linux)   TARGET="${TARGET//darwin/linux}" ;;
    *)       echo "Unsupported OS: $OS" && exit 1 ;;
esac

echo "Detected: $OS / $ARCH → $TARGET"

# Fetch latest release tag
LATEST_TAG=$(curl -sL "https://api.github.com/repos/${REPO}/releases/latest" \
    | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

if [ -z "$LATEST_TAG" ]; then
    echo "Error: could not find latest release"
    exit 1
fi

echo "Installing tuff ${LATEST_TAG}..."

# Download and extract
TARBALL="tuff-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${TARBALL}"

curl -fsSL "$DOWNLOAD_URL" -o "/tmp/${TARBALL}"
tar xzf "/tmp/${TARBALL}" -C /tmp tuff
rm -f "/tmp/${TARBALL}"

# Install
if [ -w "$INSTALL_DIR" ]; then
    mv /tmp/tuff "$INSTALL_DIR/$BIN"
else
    sudo mv /tmp/tuff "$INSTALL_DIR/$BIN"
fi

echo "tuff ${LATEST_TAG} installed to ${INSTALL_DIR}/${BIN}"
echo "Run 'tuff' to get started."
