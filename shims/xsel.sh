#!/usr/bin/env bash
# cliptunnel xsel shim
# Intercepts clipboard image reads and routes them through the cliptunnel tunnel.
# For all other invocations, passes through to the real xsel binary.

set -euo pipefail

SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${SHIM_DIR}/_common.sh"

BINARY_NAME="xsel"

is_clipboard_output() {
    local has_clipboard=false
    local has_output=false

    for arg in "$@"; do
        case "$arg" in
            --clipboard|-b)
                has_clipboard=true
                ;;
            --output|-o)
                has_output=true
                ;;
            -bo|-ob)
                has_clipboard=true
                has_output=true
                ;;
        esac
    done

    $has_clipboard && $has_output
}

if is_clipboard_output "$@"; then
    if fetch_clipboard_image; then
        exit 0
    fi
    fallback_to_real "$BINARY_NAME" "$@"
else
    fallback_to_real "$BINARY_NAME" "$@"
fi
