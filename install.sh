#!/bin/sh
# ClipTunnel installer — download pre-built binary from GitHub Releases
#
# Usage:
#   curl -fsSL https://raw.githubusercontent.com/abhishekbiyala/cliptunnel/main/install.sh | sh
#
# Options:
#   CLIPTUNNEL_VERSION=v0.2.0  — install a specific version (default: latest)
#
# Installs to ~/.local/bin/cliptunnel

set -eu
umask 077

REPO="abhishekbiyala/cliptunnel"
INSTALL_DIR="$HOME/.local/bin"
BINARY_NAME="cliptunnel"
VERSION="${CLIPTUNNEL_VERSION:-latest}"

# ── Helpers ──────────────────────────────────────────────────────────────

info()  { printf '  \033[1m%s\033[0m\n' "$*"; }
ok()    { printf '  \033[32m✓\033[0m %s\n' "$*"; }
warn()  { printf '  \033[33m⚠\033[0m %s\n' "$*"; }
err()   { printf '  \033[31m✗\033[0m %s\n' "$*" >&2; exit 1; }

# ── Platform detection ───────────────────────────────────────────────────

detect_platform() {
    OS=$(uname -s)
    ARCH=$(uname -m)

    case "$OS" in
        Darwin) OS_LABEL="darwin" ;;
        Linux)
            err "ClipTunnel is installed on your Mac (the client side).
    The Linux binary is deployed automatically when you run:
      cliptunnel setup user@your-remote-host"
            ;;
        *)
            err "Unsupported OS: $OS. ClipTunnel requires macOS."
            ;;
    esac

    case "$ARCH" in
        arm64|aarch64) ARCH_LABEL="arm64" ;;
        x86_64)        ARCH_LABEL="x86_64" ;;
        *)
            err "Unsupported architecture: $ARCH"
            ;;
    esac
}

# ── Download ─────────────────────────────────────────────────────────────

get_release_url() {
    ASSET_NAME="${BINARY_NAME}-${OS_LABEL}-${ARCH_LABEL}"

    if [ "$VERSION" = "latest" ]; then
        API_URL="https://api.github.com/repos/${REPO}/releases/latest"
    else
        API_URL="https://api.github.com/repos/${REPO}/releases/tags/${VERSION}"
    fi

    TMPJSON=$(mktemp)
    HTTP_CODE=""

    if command -v curl >/dev/null 2>&1; then
        HTTP_CODE=$(curl -sS -w '%{http_code}' -o "$TMPJSON" "$API_URL") || true
    elif command -v wget >/dev/null 2>&1; then
        wget -qO "$TMPJSON" "$API_URL" 2>/dev/null && HTTP_CODE="200" || HTTP_CODE="000"
    else
        rm -f "$TMPJSON"
        err "Neither curl nor wget found. Please install one and try again."
    fi

    if [ "$HTTP_CODE" = "403" ]; then
        rm -f "$TMPJSON"
        err "GitHub API rate limited. Try again later or set GITHUB_TOKEN."
    fi

    # Anchor the grep to match exact asset name (not supersets)
    DOWNLOAD_URL=$(grep "browser_download_url.*${ASSET_NAME}\"" "$TMPJSON" | head -1 | cut -d '"' -f 4)
    rm -f "$TMPJSON"

    if [ -z "$DOWNLOAD_URL" ]; then
        err "Could not find release asset '${ASSET_NAME}' at:
    https://github.com/${REPO}/releases/${VERSION}

    If this is a new project, you may need to create a release first.
    See: https://github.com/${REPO}#building-from-source"
    fi
}

download_binary() {
    mkdir -p "$INSTALL_DIR"

    # Download to temp file first, then move atomically
    TMPFILE=$(mktemp "${INSTALL_DIR}/.cliptunnel-install.XXXXXX")

    info "Downloading ${BINARY_NAME}..."
    if command -v curl >/dev/null 2>&1; then
        curl -fsSL -o "$TMPFILE" "$DOWNLOAD_URL" || { rm -f "$TMPFILE"; err "Download failed."; }
    else
        wget -qO "$TMPFILE" "$DOWNLOAD_URL" || { rm -f "$TMPFILE"; err "Download failed."; }
    fi

    mv "$TMPFILE" "${INSTALL_DIR}/${BINARY_NAME}"
    chmod +x "${INSTALL_DIR}/${BINARY_NAME}"
    ok "Installed to ${INSTALL_DIR}/${BINARY_NAME}"
}

# ── PATH setup ───────────────────────────────────────────────────────────

ensure_path() {
    case ":$PATH:" in
        *":${INSTALL_DIR}:"*) return ;;
    esac

    # Determine which shell rc to update
    SHELL_NAME=$(basename "$SHELL" 2>/dev/null || echo "")
    case "$SHELL_NAME" in
        zsh)  RC_FILE="$HOME/.zshrc" ;;
        bash) RC_FILE="$HOME/.bashrc" ;;
        *)    RC_FILE="$HOME/.profile" ;;
    esac

    LINE="export PATH=\"\$HOME/.local/bin:\$PATH\""

    # Check if already in rc file
    if [ -f "$RC_FILE" ] && grep -qF '.local/bin' "$RC_FILE" 2>/dev/null; then
        return
    fi

    # Do not follow symlinks when writing to shell rc files
    if [ -L "$RC_FILE" ]; then
        warn "Skipping PATH setup: ${RC_FILE} is a symlink."
        warn "Add this to your shell config manually: $LINE"
        return
    fi

    printf '\n# Added by ClipTunnel installer\n%s\n' "$LINE" >> "$RC_FILE"
    ok "Added ~/.local/bin to PATH in ${RC_FILE}"
    NEEDS_RELOAD=1
}

# ── Main ─────────────────────────────────────────────────────────────────

main() {
    printf '\n'
    info "ClipTunnel Installer"
    printf '\n'

    detect_platform
    get_release_url
    download_binary
    ensure_path

    printf '\n'
    ok "ClipTunnel installed successfully!"
    printf '\n'
    info "Get started:"
    printf '    cliptunnel setup user@your-remote-host\n'
    printf '\n'
    info "Copy an image on your Mac, paste in your remote coding agent."
    printf '\n'
    info "Docs: https://github.com/${REPO}"
    printf '\n'

    if [ "${NEEDS_RELOAD:-}" = "1" ]; then
        warn "Run 'source ${RC_FILE}' or open a new terminal to update your PATH."
        printf '\n'
    fi
}

main "$@"
