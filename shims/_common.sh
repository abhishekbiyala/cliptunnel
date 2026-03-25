#!/usr/bin/env bash
# cliptunnel shared functions for clipboard shims
# Sourced by xclip, xsel, and wl-paste shims.

CLIPTUNNEL_PORT="${CLIPTUNNEL_PORT:-18442}"
CLIPTUNNEL_URL="${CLIPTUNNEL_URL:-http://127.0.0.1:${CLIPTUNNEL_PORT}}"
CLIPTUNNEL_TOKEN_FILE="${HOME}/.config/cliptunnel/token"

# Resolve the caller shim's real path at source time (BASH_SOURCE[1] is the script that sourced us)
_CLIPTUNNEL_SHIM_REAL="$(realpath "${BASH_SOURCE[1]}" 2>/dev/null || readlink -f "${BASH_SOURCE[1]}" 2>/dev/null || echo "${BASH_SOURCE[1]}")"
_CLIPTUNNEL_SHIM_DIR="$(dirname "$_CLIPTUNNEL_SHIM_REAL")"

# Locate the real binary, skipping this shim.
# Usage: find_real_binary <binary_name>
find_real_binary() {
    local binary_name="$1"
    local self="$_CLIPTUNNEL_SHIM_REAL"
    local self_dir="$_CLIPTUNNEL_SHIM_DIR"

    for candidate in "/usr/bin/${binary_name}" "/usr/local/bin/${binary_name}"; do
        if [ -x "$candidate" ]; then
            local candidate_real
            candidate_real="$(realpath "$candidate" 2>/dev/null || readlink -f "$candidate" 2>/dev/null || echo "$candidate")"
            if [ "$candidate_real" != "$self" ]; then
                echo "$candidate"
                return 0
            fi
        fi
    done

    # Search PATH, excluding our own directory
    while IFS= read -r -d: dir || [ -n "$dir" ]; do
        if [ "$dir" = "$self_dir" ]; then
            continue
        fi
        if [ -x "${dir}/${binary_name}" ]; then
            echo "${dir}/${binary_name}"
            return 0
        fi
    done <<< "$PATH"

    return 1
}

# Create a curl config file with auth header (avoids token in process list)
make_auth_config() {
    local token_file="$1"
    local config
    config="$(mktemp /tmp/cliptunnel-curl.XXXXXX)"
    chmod 600 "$config"
    printf 'header = "Authorization: Bearer %s"\n' "$(cat "$token_file" | tr -d '[:space:]')" > "$config"
    echo "$config"
}

# Fetch clipboard image from the tunnel
fetch_clipboard_image() {
    [ -f "$CLIPTUNNEL_TOKEN_FILE" ] || return 1

    local config
    config="$(make_auth_config "$CLIPTUNNEL_TOKEN_FILE")"
    trap "rm -f '$config'" RETURN

    local tmpfile
    tmpfile="$(mktemp /tmp/cliptunnel-clip.XXXXXX)"

    local http_code
    http_code="$(curl -s -o "$tmpfile" -w '%{http_code}' \
        -K "$config" \
        "${CLIPTUNNEL_URL}/clipboard" 2>/dev/null)" || { rm -f "$tmpfile" "$config"; return 1; }

    rm -f "$config"

    if [ "$http_code" = "200" ] && [ -s "$tmpfile" ]; then
        cat "$tmpfile"
        rm -f "$tmpfile"
        return 0
    fi

    rm -f "$tmpfile"
    return 1
}

# Check if the tunnel is reachable
tunnel_available() {
    [ -f "$CLIPTUNNEL_TOKEN_FILE" ] || return 1

    local config
    config="$(make_auth_config "$CLIPTUNNEL_TOKEN_FILE")"

    local http_code
    http_code="$(curl -s -o /dev/null -w '%{http_code}' \
        -K "$config" \
        "${CLIPTUNNEL_URL}/clipboard" 2>/dev/null)" || { rm -f "$config"; return 1; }

    rm -f "$config"
    [ "$http_code" = "200" ]
}

# Fallback to real binary. Usage: fallback_to_real <binary_name> "$@"
fallback_to_real() {
    local binary_name="$1"
    shift
    local real_bin
    real_bin="$(find_real_binary "$binary_name")" || { echo "cliptunnel: real ${binary_name} not found" >&2; exit 1; }
    exec "$real_bin" "$@"
}
