#!/bin/sh
set -eu

REPO="kannandreams/tuff"
BIN="tuff"

die() {
    echo "Error: $*" >&2
    exit 1
}

require_command() {
    if ! command -v "$1" >/dev/null 2>&1; then
        die "required command '$1' is unavailable. Install it and retry."
    fi
}

require_command curl
require_command tar
require_command sed
require_command head
require_command awk

OS=$(uname -s)
ARCH=$(uname -m)

case "$OS-$ARCH" in
    Darwin-arm64 | Darwin-aarch64)
        TARGET="aarch64-apple-darwin"
        DEFAULT_INSTALL_DIR="/usr/local/bin"
        HASH_COMMAND="shasum"
        ;;

    Darwin-x86_64)
        TARGET="x86_64-apple-darwin"
        DEFAULT_INSTALL_DIR="/usr/local/bin"
        HASH_COMMAND="shasum"
        ;;

    Linux-x86_64)
        TARGET="x86_64-unknown-linux-gnu"
        DEFAULT_INSTALL_DIR="/usr/local/bin"
        HASH_COMMAND="sha256sum"
        ;;

    *)
        echo "Unsupported platform: $OS / $ARCH" >&2
        exit 1
        ;;
esac

require_command "$HASH_COMMAND"

INSTALL_DIR="${TUFF_INSTALL_DIR:-$DEFAULT_INSTALL_DIR}"

echo "Detected platform: $OS / $ARCH"
echo "Release target: $TARGET"

RELEASE_METADATA=$(curl -fsSL \
    -H "Accept: application/vnd.github+json" \
    "https://api.github.com/repos/${REPO}/releases/latest") ||
    die "could not download the latest Tuff release metadata. Check your network connection and retry."

LATEST_TAG=$(printf '%s\n' "$RELEASE_METADATA" |
    sed -n 's/.*"tag_name":[[:space:]]*"\([^"]*\)".*/\1/p' |
    head -n 1)

if [ -z "$LATEST_TAG" ]; then
    die "could not determine the latest Tuff release from GitHub."
fi

TARBALL="${BIN}-${TARGET}.tar.gz"
DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/${TARBALL}"
CHECKSUM_URL="https://github.com/${REPO}/releases/download/${LATEST_TAG}/checksums.txt"

TMP_DIR=$(mktemp -d)

cleanup() {
    rm -rf "$TMP_DIR"
}

trap cleanup EXIT INT TERM

echo "Installing ${BIN} ${LATEST_TAG}..."
echo "Downloading ${DOWNLOAD_URL}"

if ! curl -fsSL "$DOWNLOAD_URL" -o "$TMP_DIR/$TARBALL"; then
    die "could not download the release archive. Check that ${TARBALL} exists for ${LATEST_TAG} and retry."
fi

echo "Downloading ${CHECKSUM_URL}"
if ! curl -fsSL "$CHECKSUM_URL" -o "$TMP_DIR/checksums.txt"; then
    die "could not download checksums.txt for ${LATEST_TAG}. The release may be incomplete."
fi

EXPECTED_SHA256=$(awk -v archive="$TARBALL" '$2 == archive {print tolower($1); exit}' "$TMP_DIR/checksums.txt")
if [ -z "$EXPECTED_SHA256" ]; then
    die "checksums.txt does not contain an entry for ${TARBALL}. Installation aborted."
fi

if ! printf '%s\n' "$EXPECTED_SHA256" | awk 'length($0) == 64 && $0 !~ /[^0-9a-f]/ {valid=1} END {exit !valid}'; then
    die "checksums.txt contains an invalid SHA-256 value for ${TARBALL}. Installation aborted."
fi

if [ "$HASH_COMMAND" = "shasum" ]; then
    ACTUAL_SHA256=$(shasum -a 256 "$TMP_DIR/$TARBALL" | awk '{print tolower($1)}') ||
        die "could not calculate the SHA-256 checksum for ${TARBALL}. Installation aborted."
else
    ACTUAL_SHA256=$(sha256sum "$TMP_DIR/$TARBALL" | awk '{print tolower($1)}') ||
        die "could not calculate the SHA-256 checksum for ${TARBALL}. Installation aborted."
fi

if [ "$EXPECTED_SHA256" != "$ACTUAL_SHA256" ]; then
    echo "Checksum verification failed." >&2
    echo "Expected: $EXPECTED_SHA256" >&2
    echo "Actual:   $ACTUAL_SHA256" >&2
    echo >&2
    echo "Installation aborted." >&2
    exit 1
fi

echo "Checksum verified: ${TARBALL}"

if ! tar -xzf "$TMP_DIR/$TARBALL" -C "$TMP_DIR"; then
    die "could not extract the verified release archive. Installation aborted."
fi

if [ ! -f "$TMP_DIR/$BIN" ]; then
    die "verified archive did not contain the '$BIN' executable. Installation aborted."
fi

chmod +x "$TMP_DIR/$BIN"

if [ ! -d "$INSTALL_DIR" ]; then
    if mkdir -p "$INSTALL_DIR" 2>/dev/null; then
        :
    elif command -v sudo >/dev/null 2>&1; then
        sudo mkdir -p "$INSTALL_DIR"
    else
        echo "Error: cannot create $INSTALL_DIR and sudo is unavailable." >&2
        exit 1
    fi
fi

if [ -w "$INSTALL_DIR" ]; then
    mv "$TMP_DIR/$BIN" "$INSTALL_DIR/$BIN"
elif command -v sudo >/dev/null 2>&1; then
    sudo mv "$TMP_DIR/$BIN" "$INSTALL_DIR/$BIN"
else
    echo "Error: $INSTALL_DIR is not writable and sudo is unavailable." >&2
    echo "Set TUFF_INSTALL_DIR to a writable directory, for example:" >&2
    echo "  TUFF_INSTALL_DIR=\"\$HOME/.local/bin\" sh install.sh" >&2
    exit 1
fi

echo
echo "${BIN} ${LATEST_TAG} installed to ${INSTALL_DIR}/${BIN}"
echo "Run '${BIN} --version' to verify the installation."
