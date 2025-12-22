#!/bin/bash
# AX Installer - One-line installation script
# Usage: curl -fsSL https://raw.githubusercontent.com/ahmed6ww/ax/main/install.sh | bash

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
CYAN='\033[0;36m'
NC='\033[0m' # No Color

echo -e "${CYAN}"
echo "  ╔═══════════════════════════════════════╗"
echo "  ║     AX - Agent Package Manager        ║"
echo "  ║   The npm of the Agentic AI era       ║"
echo "  ╚═══════════════════════════════════════╝"
echo -e "${NC}"

# Detect OS and architecture
OS=$(uname -s | tr '[:upper:]' '[:lower:]')
ARCH=$(uname -m)

case "$OS-$ARCH" in
  linux-x86_64)   
    BINARY="ax-linux-x64"
    ;;
  linux-aarch64)   
    BINARY="ax-linux-arm64"
    ;;
  darwin-x86_64)  
    BINARY="ax-macos-x64"
    ;;
  darwin-arm64)   
    BINARY="ax-macos-arm64"
    ;;
  *)
    echo -e "${RED}Error: Unsupported platform: $OS-$ARCH${NC}"
    echo "Please build from source: cargo install ax"
    exit 1
    ;;
esac

echo "→ Detected platform: $OS-$ARCH"

# Get latest version
echo "→ Fetching latest version..."
VERSION=$(curl -sS https://api.github.com/repos/ahmed6ww/ax/releases/latest 2>/dev/null | grep '"tag_name"' | sed -E 's/.*"([^"]+)".*/\1/' || echo "v1.2.1")

if [ -z "$VERSION" ]; then
  VERSION="v1.2.1"
fi

echo "→ Installing AX $VERSION..."

# Download binary
URL="https://github.com/ahmed6ww/ax/releases/download/$VERSION/$BINARY"
TEMP_FILE=$(mktemp)

if ! curl -fsSL "$URL" -o "$TEMP_FILE" 2>/dev/null; then
  echo -e "${RED}Error: Failed to download AX${NC}"
  echo "URL: $URL"
  echo ""
  echo "Try building from source instead:"
  echo "  cargo install ax"
  rm -f "$TEMP_FILE"
  exit 1
fi

# Install
chmod +x "$TEMP_FILE"

# Try to install to /usr/local/bin, fallback to ~/.local/bin
if [ -w /usr/local/bin ]; then
  mv "$TEMP_FILE" /usr/local/bin/ax
  echo -e "${GREEN}✓ Installed to /usr/local/bin/ax${NC}"
elif command -v sudo &> /dev/null; then
  sudo mv "$TEMP_FILE" /usr/local/bin/ax
  echo -e "${GREEN}✓ Installed to /usr/local/bin/ax${NC}"
else
  mkdir -p ~/.local/bin
  mv "$TEMP_FILE" ~/.local/bin/ax
  echo -e "${GREEN}✓ Installed to ~/.local/bin/ax${NC}"
  echo ""
  echo "Add to PATH if not already:"
  echo '  export PATH="$HOME/.local/bin:$PATH"'
fi

echo ""
echo -e "${GREEN}✅ AX installed successfully!${NC}"
echo ""
echo "Get started:"
echo "  ax init                          # Initialize AX"
echo "  ax list                          # Browse available agents"
echo "  ax install rust-architect        # Install an agent"
echo ""
echo -e "${CYAN}Learn more: https://github.com/ahmed6ww/ax${NC}"
