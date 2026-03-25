#!/usr/bin/env bash
# ClipTunnel End-to-End Test
#
# Prerequisites:
#   - macOS with cliptunnel built (cargo build --release)
#   - Linux cross-compiled binary (cross build --release --target x86_64-unknown-linux-gnu --no-default-features)
#   - SSH access to a Linux remote host (set CLIPTUNNEL_TEST_HOST)
#   - Xvfb and xclip installed on remote (sudo apt install xvfb xclip)
#
# Usage:
#   CLIPTUNNEL_TEST_HOST=user@mydevbox ./tests/e2e/test_full_flow.sh
#
# This test is NOT automated — it requires manual setup and a real remote host.

set -euo pipefail

RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m'

PASS=0
FAIL=0

pass() { echo -e "  ${GREEN}✓${NC} $1"; PASS=$((PASS + 1)); }
fail() { echo -e "  ${RED}✗${NC} $1"; FAIL=$((FAIL + 1)); }

HOST="${CLIPTUNNEL_TEST_HOST:?Set CLIPTUNNEL_TEST_HOST=user@host}"
BIN="./target/release/cliptunnel"
LINUX_BIN="./target/x86_64-unknown-linux-gnu/release/cliptunnel"

echo "ClipTunnel E2E Test"
echo "  Host: $HOST"
echo ""

# ── Pre-flight checks ────────────────────────────────────────────

echo "Pre-flight checks:"

[ -x "$BIN" ] && pass "Mac binary exists" || fail "Mac binary not found at $BIN"
[ -x "$LINUX_BIN" ] && pass "Linux binary exists" || fail "Linux binary not found at $LINUX_BIN"
ssh "$HOST" "true" 2>/dev/null && pass "SSH to $HOST works" || fail "Cannot SSH to $HOST"

echo ""

# ── Test 1: Daemon ────────────────────────────────────────────────

echo "1. Daemon:"

# Kill any existing daemon
lsof -ti :18442 | xargs kill 2>/dev/null || true
sleep 1

$BIN daemon --foreground &
DAEMON_PID=$!
sleep 2

if curl -s http://127.0.0.1:18442/health | grep -q '"ok"'; then
    pass "Daemon started and healthy"
else
    fail "Daemon health check failed"
fi

# Put test image on clipboard
python3 -c "
import struct, zlib
w, h = 4, 4
raw = b''
for y in range(h):
    raw += b'\x00'
    for x in range(w):
        raw += bytes([255, 0, 0, 255])  # RGBA red
ihdr = struct.pack('>IIBBBBB', w, h, 8, 6, 0, 0, 0)
def chunk(ct, d):
    c = ct + d
    return struct.pack('>I', len(d)) + c + struct.pack('>I', zlib.crc32(c) & 0xffffffff)
with open('/tmp/cliptunnel-e2e-test.png', 'wb') as f:
    f.write(b'\x89PNG\r\n\x1a\n' + chunk(b'IHDR', ihdr) + chunk(b'IDAT', zlib.compress(raw)) + chunk(b'IEND', b''))
"
osascript -e 'set the clipboard to (read (POSIX file "/tmp/cliptunnel-e2e-test.png") as «class PNGf»)' 2>/dev/null

TOKEN=$(cat "$HOME/Library/Application Support/cliptunnel/token" 2>/dev/null || cat "$HOME/.config/cliptunnel/token" 2>/dev/null)

METADATA=$(curl -s -H "Authorization: Bearer $TOKEN" http://127.0.0.1:18442/clipboard/metadata)
if echo "$METADATA" | grep -q '"width":4'; then
    pass "Clipboard serves test image (4x4 red PNG)"
else
    fail "Clipboard metadata unexpected: $METADATA"
fi

echo ""

# ── Test 2: Connect & Deploy ─────────────────────────────────────

echo "2. Connect & Deploy:"

$BIN connect "$HOST" --binary "$LINUX_BIN" --x11 2>/dev/null
if [ $? -eq 0 ]; then
    pass "cliptunnel connect succeeded"
else
    fail "cliptunnel connect failed"
fi

ssh "$HOST" "~/.local/bin/cliptunnel --version" 2>/dev/null | grep -q "cliptunnel" && \
    pass "Remote binary installed" || fail "Remote binary not found"

ssh "$HOST" "ls ~/.local/bin/xclip ~/.local/bin/xsel ~/.local/bin/wl-paste" 2>/dev/null >/dev/null && \
    pass "Shims installed" || fail "Shims not found"

echo ""

# ── Test 3: Tunnel & Shims ───────────────────────────────────────

echo "3. Tunnel & Shims (xclip path — Claude Code, Gemini CLI, OpenCode CLI):"

TARGETS=$(ssh "$HOST" "PATH=~/.local/bin:\$PATH xclip -selection clipboard -t TARGETS -o 2>/dev/null" || true)
if echo "$TARGETS" | grep -q "image/png"; then
    pass "xclip shim TARGETS query returns image/png"
else
    fail "xclip shim TARGETS query failed: $TARGETS"
fi

IMG_TYPE=$(ssh "$HOST" "PATH=~/.local/bin:\$PATH xclip -selection clipboard -t image/png -o 2>/dev/null | file -" || true)
if echo "$IMG_TYPE" | grep -q "PNG image data"; then
    pass "xclip shim returns valid PNG image"
else
    fail "xclip shim image fetch failed: $IMG_TYPE"
fi

echo ""

# ── Test 4: X11 Bridge ──────────────────────────────────────────

echo "4. X11 Bridge (Codex CLI, Copilot CLI):"

ssh "$HOST" "pgrep Xvfb" >/dev/null 2>&1 && \
    pass "Xvfb running" || fail "Xvfb not running"

sleep 4  # Wait for x11-owner to poll

X11_IMG=$(ssh "$HOST" "DISPLAY=:99 XAUTHORITY=\$HOME/.Xauthority /usr/bin/xclip -selection clipboard -t image/png -o 2>/dev/null | file -" || true)
if echo "$X11_IMG" | grep -q "PNG image data"; then
    pass "X11 clipboard contains valid PNG image"
else
    fail "X11 clipboard empty or invalid: $X11_IMG"
fi

echo ""

# ── Test 5: Doctor ───────────────────────────────────────────────

echo "5. Doctor:"

DOCTOR_OUTPUT=$($BIN doctor --host "$HOST" 2>/dev/null)
DOCTOR_PASSED=$(echo "$DOCTOR_OUTPUT" | grep -c "✓" || true)
DOCTOR_FAILED=$(echo "$DOCTOR_OUTPUT" | grep -c "✗" || true)

if [ "$DOCTOR_FAILED" -eq 0 ]; then
    pass "All doctor checks passed ($DOCTOR_PASSED checks)"
else
    fail "Doctor: $DOCTOR_PASSED passed, $DOCTOR_FAILED failed"
    echo "$DOCTOR_OUTPUT" | grep "✗"
fi

echo ""

# ── Test 6: GC ───────────────────────────────────────────────────

echo "6. GC:"

ssh "$HOST" "touch -t 202501010000 /tmp/cliptunnel-e2e-old.png" 2>/dev/null
GC_OUTPUT=$(ssh "$HOST" "~/.local/bin/cliptunnel gc --max-age 1" 2>/dev/null)
if echo "$GC_OUTPUT" | grep -q "Cleaned up"; then
    pass "GC cleaned old files"
else
    fail "GC did not clean files: $GC_OUTPUT"
fi

echo ""

# ── Test 7: Disconnect ───────────────────────────────────────────

echo "7. Disconnect:"

$BIN disconnect "$HOST" 2>/dev/null
if ! grep -A5 "^Host $HOST$" ~/.ssh/config 2>/dev/null | grep -q "RemoteForward 18442"; then
    pass "RemoteForward removed from SSH config"
else
    fail "RemoteForward still in SSH config"
fi

echo ""

# ── Cleanup ──────────────────────────────────────────────────────

kill $DAEMON_PID 2>/dev/null || true
rm -f /tmp/cliptunnel-e2e-test.png

# ── Summary ──────────────────────────────────────────────────────

TOTAL=$((PASS + FAIL))
echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
if [ "$FAIL" -eq 0 ]; then
    echo -e "${GREEN}All $TOTAL tests passed!${NC}"
else
    echo -e "${YELLOW}$PASS/$TOTAL passed, $FAIL failed${NC}"
fi
