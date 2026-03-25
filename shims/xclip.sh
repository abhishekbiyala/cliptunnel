#!/usr/bin/env bash
# cliptunnel xclip shim
# Intercepts clipboard image reads and routes them through the cliptunnel tunnel.
# For all other invocations, passes through to the real xclip binary.

set -euo pipefail

SHIM_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=_common.sh
source "${SHIM_DIR}/_common.sh"

BINARY_NAME="xclip"

# Detect: xclip -selection clipboard -t image/png -o (in any order)
is_clipboard_image_read() {
    local has_selection_clipboard=false
    local has_type_image_png=false
    local has_output=false
    local i=1

    while [ $i -le $# ]; do
        local arg="${!i}"
        case "$arg" in
            -selection)
                i=$((i + 1))
                if [ $i -le $# ] && [ "${!i}" = "clipboard" ]; then
                    has_selection_clipboard=true
                fi
                ;;
            -t)
                i=$((i + 1))
                if [ $i -le $# ] && [ "${!i}" = "image/png" ]; then
                    has_type_image_png=true
                fi
                ;;
            -o|-out)
                has_output=true
                ;;
        esac
        i=$((i + 1))
    done

    $has_selection_clipboard && $has_type_image_png && $has_output
}

# Detect: xclip -selection clipboard -t TARGETS -o
is_targets_query() {
    local has_selection_clipboard=false
    local has_type_targets=false
    local has_output=false
    local i=1

    while [ $i -le $# ]; do
        local arg="${!i}"
        case "$arg" in
            -selection)
                i=$((i + 1))
                if [ $i -le $# ] && [ "${!i}" = "clipboard" ]; then
                    has_selection_clipboard=true
                fi
                ;;
            -t)
                i=$((i + 1))
                if [ $i -le $# ] && [ "${!i}" = "TARGETS" ]; then
                    has_type_targets=true
                fi
                ;;
            -o|-out)
                has_output=true
                ;;
        esac
        i=$((i + 1))
    done

    $has_selection_clipboard && $has_type_targets && $has_output
}

# Main logic
if is_clipboard_image_read "$@"; then
    if fetch_clipboard_image; then
        exit 0
    fi
    fallback_to_real "$BINARY_NAME" "$@"

elif is_targets_query "$@"; then
    if tunnel_available; then
        echo "image/png"
        exit 0
    fi
    fallback_to_real "$BINARY_NAME" "$@"

else
    fallback_to_real "$BINARY_NAME" "$@"
fi
