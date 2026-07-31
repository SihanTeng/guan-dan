#!/usr/bin/env bash
# Guandan client one-line install
# Usage: curl -fsSL https://raw.githubusercontent.com/SihanTeng/guan-dan/main/install.sh | bash

set -euo pipefail

REPO="SihanTeng/guan-dan"
BIN_NAME="guandan"

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
CYAN='\033[0;36m'
NC='\033[0m'

info()  { echo -e "${GREEN}[INFO]${NC} $1"; }
warn()  { echo -e "${YELLOW}[WARN]${NC} $1"; }
error() { echo -e "${RED}[ERROR]${NC} $1"; exit 1; }

detect_platform() {
    OS=$(uname -s | tr '[:upper:]' '[:lower:]')
    ARCH=$(uname -m)

    case "$OS" in
        linux)  OS="linux" ;;
        darwin) OS="darwin" ;;
        *) error "Unsupported OS: $OS" ;;
    esac

    case "$ARCH" in
        x86_64|amd64)   ARCH="amd64" ;;
        aarch64|arm64)  ARCH="arm64" ;;
        *) error "Unsupported architecture: $ARCH" ;;
    esac

    info "Detected platform: ${OS}-${ARCH}"
}

get_latest_version() {
    info "Fetching latest release..."
    LATEST_VERSION=$(curl -fsSL "https://api.github.com/repos/${REPO}/releases/latest" \
        | grep '"tag_name":' | sed -E 's/.*"([^"]+)".*/\1/')

    if [ -z "${LATEST_VERSION:-}" ]; then
        error "Could not determine latest version (no releases yet?)"
    fi
    info "Latest version: $LATEST_VERSION"
}

download_binary() {
    BINARY_NAME="guandan-${OS}-${ARCH}"
    DOWNLOAD_URL="https://github.com/${REPO}/releases/download/${LATEST_VERSION}/${BINARY_NAME}"
    CHECKSUM_URL="${DOWNLOAD_URL}.sha256"

    info "Downloading client..."
    TMP_DIR=$(mktemp -d)
    # shellcheck disable=SC2064
    trap "rm -rf '$TMP_DIR'" EXIT
    cd "$TMP_DIR"

    if ! curl -fsSL -o "$BINARY_NAME" "$DOWNLOAD_URL"; then
        error "Download failed: $DOWNLOAD_URL"
    fi

    if curl -fsSL -o "${BINARY_NAME}.sha256" "$CHECKSUM_URL" 2>/dev/null; then
        info "Verifying checksum..."
        if command -v sha256sum >/dev/null 2>&1; then
            sha256sum -c "${BINARY_NAME}.sha256" || error "Checksum verification failed"
        elif command -v shasum >/dev/null 2>&1; then
            shasum -a 256 -c "${BINARY_NAME}.sha256" || error "Checksum verification failed"
        else
            warn "No sha256 tool found; skipping checksum"
        fi
    else
        warn "Checksum file not available; skipping verification"
    fi
}

install_binary() {
    info "Installing client..."

    if [ -d "$HOME/.local/bin" ] || mkdir -p "$HOME/.local/bin" 2>/dev/null; then
        INSTALL_DIR="$HOME/.local/bin"
    else
        INSTALL_DIR="/usr/local/bin"
    fi

    if [ "$INSTALL_DIR" = "/usr/local/bin" ] && [ ! -w "$INSTALL_DIR" ]; then
        warn "Need sudo to install into $INSTALL_DIR"
        sudo mv "$BINARY_NAME" "$INSTALL_DIR/${BIN_NAME}"
        sudo chmod +x "$INSTALL_DIR/${BIN_NAME}"
    else
        mv "$BINARY_NAME" "$INSTALL_DIR/${BIN_NAME}"
        chmod +x "$INSTALL_DIR/${BIN_NAME}"
    fi

    info "Installed to: $INSTALL_DIR/${BIN_NAME}"

    case ":$PATH:" in
        *":$INSTALL_DIR:"*) ;;
        *)
            warn "$INSTALL_DIR is not on PATH"
            warn "Add this to your shell config (~/.bashrc / ~/.zshrc):"
            echo ""
            echo "    export PATH=\"\$PATH:$INSTALL_DIR\""
            echo ""
            ;;
    esac
}

main() {
    echo ""
    echo -e "${CYAN}🥚 掼蛋 Guandan — client install${NC}"
    echo ""

    detect_platform
    get_latest_version
    download_binary
    install_binary

    echo ""
    info "✅ Install complete!"
    echo ""
    echo -e "  Play:   ${CYAN}${BIN_NAME}${NC}"
    echo -e "  Help:   ${CYAN}${BIN_NAME} --help${NC}"
    echo ""
    echo "  Default server: ws://127.0.0.1:9100"
    echo "  Override:       guandan --server ws://host:9100"
    echo ""
    echo "  Run a server locally:"
    echo "    guandan-server   # from release assets or cargo run -p guandan-server"
    echo ""
}

main
