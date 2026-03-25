#!/usr/bin/env bash
# cliptunnel wl-paste shim
# Intercepts clipboard image reads and routes them through the cliptunnel tunnel.
# For all other invocations, passes through to the real wl-paste binary.

set -euo pipefail

SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${SHIM_DIR}/_common.sh"

BINARY_NAME="wl-paste"

is_image_png_paste() {
    local i=1
    while [ $i -le $# ]; do
        local arg="${!i}"
        case "$arg" in
            --type|-t)
                i=$((i + 1))
                if [ $i -le $# ] && [ "${!i}" = "image/png" ]; then
                    return 0
                fi
                ;;
            --type=image/png|-t=image/png)
                return 0
                ;;
        esac
        i=$((i + 1))
    done
    return 1
}

is_list_types() {
    for arg in "$@"; do
        case "$arg" in
            --list-types|-l)
                return 0
                ;;
        esac
    done
    return 1
}

if is_image_png_paste "$@"; then
    if fetch_clipboard_image; then
        exit 0
    fi
    fallback_to_real "$BINARY_NAME" "$@"

elif is_list_types "$@"; then
    if tunnel_available; then
        echo "image/png"
        exit 0
    fi
    fallback_to_real "$BINARY_NAME" "$@"

else
    fallback_to_real "$BINARY_NAME" "$@"
fi
