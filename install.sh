#!/bin/sh
# mcp-hub installer — https://github.com/jiale-cheng-ning/mcp-hub
# Usage: curl -sSL https://raw.githubusercontent.com/jiale-cheng-ning/mcp-hub/main/install.sh | sh

set -e

REPO="jiale-cheng-ning/mcp-hub"
BINARY="mcp-hub"

# Detect OS and architecture
OS="$(uname -s)"
ARCH="$(uname -m)"

case "$OS" in
    Linux*)   PLATFORM="linux"  ;;
    Darwin*)  PLATFORM="darwin" ;;
    MINGW*|MSYS*|CYGWIN*) PLATFORM="windows" ;;
    *)
        echo "Unsupported OS: $OS"
        echo "Download manually from: https://github.com/$REPO/releases"
        exit 1
        ;;
esac

case "$ARCH" in
    x86_64|amd64)  ARCH_NAME="amd64" ;;
    aarch64|arm64) ARCH_NAME="arm64" ;;
    *)
        echo "Unsupported architecture: $ARCH"
        echo "Download manually from: https://github.com/$REPO/releases"
        exit 1
        ;;
esac

if [ "$PLATFORM" = "linux" ] && [ "$ARCH_NAME" = "arm64" ]; then
    echo "Linux ARM64 is not yet supported."
    echo "Build from source: cargo install --git https://github.com/$REPO"
    exit 1
fi

ARTIFACT="${BINARY}-${PLATFORM}-${ARCH_NAME}"
if [ "$PLATFORM" = "windows" ]; then
    ARTIFACT="${ARTIFACT}.exe"
fi

# Get latest release URL
echo "Downloading mcp-hub for ${PLATFORM}-${ARCH_NAME}..."
URL="https://github.com/${REPO}/releases/latest/download/${ARTIFACT}"

# Download
if command -v curl >/dev/null 2>&1; then
    curl -sSL "$URL" -o "$BINARY"
elif command -v wget >/dev/null 2>&1; then
    wget -q "$URL" -O "$BINARY"
else
    echo "Error: curl or wget is required."
    echo "Download manually from: https://github.com/$REPO/releases"
    exit 1
fi

chmod +x "$BINARY"

# Move to PATH
INSTALL_DIR="${HOME}/.local/bin"
mkdir -p "$INSTALL_DIR"
mv "$BINARY" "$INSTALL_DIR/$BINARY"

echo ""
echo "Installed mcp-hub to ${INSTALL_DIR}/${BINARY}"
echo ""

# Check if in PATH
case ":$PATH:" in
    *":$INSTALL_DIR:"*) ;;
    *)
        echo "NOTE: Add ${INSTALL_DIR} to your PATH:"
        echo "  export PATH=\"\$HOME/.local/bin:\$PATH\""
        echo ""
        ;;
esac

echo "Run 'mcp-hub --help' to get started."
