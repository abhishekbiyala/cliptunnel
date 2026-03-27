# ClipTunnel

[![CI](https://github.com/abhishekbiyala/cliptunnel/actions/workflows/ci.yml/badge.svg)](https://github.com/abhishekbiyala/cliptunnel/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-25%25-yellow.svg)](TESTING.md)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)

Screenshot on your Mac. Ctrl+V in your remote coding agent over **SSH**. It just works.

Terminal coding agents like Claude Code, Codex, and Copilot CLI support pasting images with Ctrl+V — but that breaks the moment you SSH into a remote devbox. ClipTunnel brings it back, transparently, for every agent, across any terminal.

## Install

```bash
curl -fsSL https://raw.githubusercontent.com/abhishekbiyala/cliptunnel/main/install.sh | sh
```

## Setup

```bash
cliptunnel setup user@your-remote-host
```

That's it. Two commands, one time. Here's what `setup` does:
- Start a local clipboard daemon on your Mac (auto-starts on login)
- Deploy a lightweight binary to your remote host
- Add a `RemoteForward` entry to `~/.ssh/config` (backed up to `.cliptunnel.bak`)
- Install clipboard shims so coding agents find images transparently
- Verify everything works

## Use

1. Copy a screenshot on your Mac (Cmd+Shift+4, etc.)
2. SSH into your remote host
3. Press **Ctrl+V** in your coding agent

No extra commands. No terminals to keep open. It survives reboots.

## Supported Coding Agents

| CLI | Paste key | Layer |
|-----|-----------|-------|
| Claude Code | Ctrl+V | Shim |
| Gemini CLI | Ctrl+V | Shim |
| OpenCode CLI | Ctrl+V | Shim |
| Codex CLI | Ctrl+V | X11 bridge (needs `--x11`) |
| Copilot CLI | Alt+V | X11 bridge (needs `--x11`) |

> For Codex/Copilot CLI, run `cliptunnel setup host --x11` instead. This requires `xvfb` and `xclip` on the remote (`sudo apt install xvfb xclip`).

## SSH Requirements

**Minimum:** You can `ssh user@host` and get a shell. That's it.

ClipTunnel works with both key-based and password-based SSH. During setup, it will detect which you have and advise accordingly.

**Recommended:** Key-based auth (`ssh-copy-id user@host`) for zero-friction reconnects. With password auth, everything works — you'll just see password prompts during setup.

**Mosh users:** Mosh doesn't support port forwarding, so ClipTunnel maintains a separate SSH tunnel. This requires key-based auth for auto-reconnect. After `setup`, run:

```bash
cliptunnel tunnel user@host --install
```

See [Advanced Usage](#persistent-tunnel-mosh-users) for details.

## How It Works

```
Mac (local)                              Remote Linux (headless)
┌──────────────────────┐                 ┌────────────────────────────────┐
│ cliptunnel daemon    │   SSH Tunnel    │ xclip/xsel/wl-paste shims      │
│ (HTTP on :18442)     │◄───────────────►│ (curl → localhost:18442)       │
│                      │  RemoteForward  │                                │
│ reads Mac clipboard  │                 │ X11 bridge (Xvfb + owner)      │
│ serves PNG over HTTP │                 │ for Codex/Copilot CLI          │
│ bearer token auth    │                 │                                │
│ launchd auto-start   │                 │ /tmp/cliptunnel-*.png cache    │
└──────────────────────┘                 └────────────────────────────────┘
```

**Two layers of clipboard support:**

| Layer | What it does | Which CLIs |
|-------|-------------|------------|
| **Shims** | Bash scripts at `~/.local/bin/{xclip,xsel,wl-paste}` intercept clipboard reads and fetch from the tunnel. Fall through to real binaries for everything else. | Claude Code, Gemini CLI, OpenCode CLI |
| **X11 bridge** | Xvfb virtual display + polling script that pushes Mac clipboard images into a real X11 clipboard selection. | Codex CLI, Copilot CLI, any native X11 consumer |

## What Survives Reboots

| Component | Mac reboot | Linux reboot |
|-----------|-----------|-------------|
| Daemon | Yes (launchd) | N/A |
| SSH config | Yes (file) | N/A |
| Tunnel (mosh) | Yes (launchd) | N/A |
| Shims | N/A | Yes (files) |
| Token | Yes (file) | Yes (file) |
| X11 bridge | N/A | **No** — re-run `cliptunnel setup host --x11` |

---

## Advanced Usage

### All Commands

```
cliptunnel setup <host>              # One-command setup (recommended)
cliptunnel setup <host> --x11        # Setup with X11 bridge for Codex/Copilot

cliptunnel daemon --install          # Install daemon as launchd service
cliptunnel daemon --foreground       # Run daemon in foreground (debugging)
cliptunnel daemon --uninstall        # Remove launchd service

cliptunnel connect <host>            # Deploy to remote (without daemon install)
cliptunnel connect <host> --x11      # Deploy with X11 bridge

cliptunnel tunnel <host> --install   # Persistent SSH tunnel (mosh users)
cliptunnel tunnel <host> --uninstall # Remove tunnel service

cliptunnel disconnect <host>         # Remove SSH forwarding
cliptunnel disconnect <host> --clean # Also remove remote shims/binary

cliptunnel doctor                    # Check local daemon status
cliptunnel doctor --host <host>      # Full pipeline diagnostics

cliptunnel gc                        # Clean up old temp images on remote
cliptunnel paste                     # Manual fallback: save clipboard to file
cliptunnel paste --tmux              # Save and send path to current tmux pane
```

### Persistent Tunnel (Mosh Users)

Mosh doesn't support SSH port forwarding. ClipTunnel can maintain a separate SSH tunnel alongside your mosh session:

```bash
# Requires key-based SSH auth for auto-reconnect
cliptunnel tunnel user@host --install
```

This installs a launchd service that keeps the tunnel alive, reconnecting automatically if it drops.

### Multiple Remote Hosts

Run `setup` for each host:

```bash
cliptunnel setup devbox1
cliptunnel setup devbox2
```

Each gets its own SSH config entry and remote deployment.

### Troubleshooting

```bash
# Run full diagnostics
cliptunnel doctor --host mydevbox

# Daemon not running?
cliptunnel daemon --install

# Shims not in PATH on remote?
# Add to remote .bashrc/.zshrc:
export PATH="$HOME/.local/bin:$PATH"

# X11 bridge not working?
# On remote: export DISPLAY=:99
# Check: DISPLAY=:99 /usr/bin/xclip -selection clipboard -t TARGETS -o

# Clipboard empty?
# Copy an image on Mac first, then check:
CFG=$(mktemp); chmod 600 "$CFG"
printf 'header = "Authorization: Bearer %s"\n' \
  "$(cat ~/Library/Application\ Support/cliptunnel/token)" > "$CFG"
curl -s -K "$CFG" http://127.0.0.1:18442/clipboard/metadata
rm -f "$CFG"
```

### Uninstall

```bash
# Remove from a specific host
cliptunnel disconnect mydevbox --clean

# Remove local daemon
cliptunnel daemon --uninstall

# Remove binary
rm ~/.local/bin/cliptunnel
```

---

## Building from Source

### Prerequisites

**Mac (client):**
- macOS 13+
- Rust toolchain via [mise](https://mise.jdx.dev/): `mise install`
- Docker (for cross-compiling Linux binary via `cross`)

**Linux (remote):**
- `curl` (for shims)
- `xclip` + `xvfb` (only if using `--x11` for Codex/Copilot)

### Build

```bash
git clone https://github.com/abhishekbiyala/cliptunnel.git
cd cliptunnel
mise install

# Mac binary
cargo build --release

# Cross-compile Linux binary (requires Docker)
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-gnu --no-default-features
```

### Test

```bash
# Unit tests (CI-safe)
cargo test --lib

# Integration tests (CI-safe)
cargo test --test daemon_test

# All tests including clipboard (needs macOS GUI)
cargo test -- --include-ignored

# E2E (needs Mac + remote Linux host)
CLIPTUNNEL_TEST_HOST=user@host ./tests/e2e/test_full_flow.sh
```

### Lint & Quality

```bash
cargo fmt --check
cargo clippy --all-targets -- -D warnings
cargo tarpaulin --lib --engine llvm --fail-under 25 --exclude-files src/setup.rs
cargo deny check
```

## Architecture

**Port:** 18442 (fixed, localhost only)

**Auth:** Bearer token (32 bytes, base64url-encoded) stored at:
- Mac: `~/Library/Application Support/cliptunnel/token`
- Linux: `~/.config/cliptunnel/token`

**SSH:** `RemoteForward 18442 127.0.0.1:18442` added to `~/.ssh/config`

**Feature flags:** `daemon` (default, macOS) includes arboard + image crates. Linux cross-compile uses `--no-default-features` to exclude them.

**Shim fallthrough:** If the tunnel is down, shims exec the real binary transparently — nothing breaks.

## License

MIT
