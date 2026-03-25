# ClipTunnel

[![CI](https://github.com/abhishekbiyala/cliptunnel/actions/workflows/ci.yml/badge.svg)](https://github.com/abhishekbiyala/cliptunnel/actions/workflows/ci.yml)
[![Coverage](https://img.shields.io/badge/coverage-25%25-yellow.svg)](TESTING.md)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](https://opensource.org/licenses/MIT)
[![Rust](https://img.shields.io/badge/rust-1.88%2B-orange.svg)](https://www.rust-lang.org/)
[![cargo-deny](https://img.shields.io/badge/cargo--deny-checked-green.svg)](https://embarkstudios.github.io/cargo-deny/)
[![cargo-audit](https://img.shields.io/badge/cargo--audit-clean-green.svg)](https://rustsec.org/)

Screenshot on your Mac. Ctrl+V in your remote coding agent over **SSH**. It just works.

Terminal coding agents like Claude Code and Codex support pasting images with Ctrl+V, but that breaks the moment you SSH into a remote devbox. ClipTunnel brings it back — transparently, for every agent, across any terminal.

Works with **every** coding CLI — Claude Code, Codex CLI, Copilot CLI, Gemini CLI, OpenCode CLI, and any future agent that reads the system clipboard.

## How It Works

```
Mac (local)                              Remote Linux (headless)
┌──────────────────────┐                 ┌────────────────────────────────┐
│ cliptunnel daemon    │   SSH Tunnel    │ xclip/xsel/wl-paste shims     │
│ (HTTP on :18442)     │◄───────────────►│ (curl → localhost:18442)       │
│                      │  RemoteForward  │                                │
│ reads Mac clipboard  │                 │ X11 bridge (Xvfb + owner)     │
│ serves PNG over HTTP │                 │ for Codex/Copilot CLI          │
│ bearer token auth    │                 │                                │
│ launchd auto-start   │                 │ /tmp/cliptunnel-*.png cache    │
└──────────────────────┘                 └────────────────────────────────┘
```

**Two layers of support:**

| Layer | What it does | Which CLIs |
|-------|-------------|------------|
| **xclip/xsel/wl-paste shims** | Intercept clipboard tool calls, route through tunnel | Claude Code, Gemini CLI, OpenCode CLI |
| **X11 bridge (Xvfb)** | Provide a real X11 clipboard via virtual display | Codex CLI, and any CLI using native X11 clipboard |

## Prerequisites

### Mac (client)

- macOS 13+
- Rust toolchain via [mise](https://mise.jdx.dev/): `mise install`
- Docker (for cross-compiling Linux binary via `cross`)
- SSH access to your remote host

### Linux (remote server)

- `curl` (for shims to fetch clipboard data)
- `xclip` (for X11 bridge): `sudo apt install xclip`
- `Xvfb` (for Codex/Copilot CLI support): `sudo apt install xvfb`
- No Rust toolchain needed — binary is cross-compiled from Mac

## Quick Start

### 1. Build

```bash
git clone https://github.com/abhishekbiyala/cliptunnel.git
cd cliptunnel

# Install Rust via mise
mise install

# Build Mac binary
cargo build --release

# Cross-compile Linux binary (requires Docker)
cargo install cross --git https://github.com/cross-rs/cross
cross build --release --target x86_64-unknown-linux-gnu --no-default-features
```

### 2. Install (one-time setup)

```bash
# Install daemon — auto-starts on Mac login, serves clipboard over HTTP
./target/release/cliptunnel daemon --install

# Deploy to your remote host — copies binary, token, installs shims
./target/release/cliptunnel connect mydevbox --binary target/x86_64-unknown-linux-gnu/release/cliptunnel

# For Codex CLI support (reads X11 clipboard directly), add --x11:
# Requires: sudo apt install xvfb xclip on the remote
./target/release/cliptunnel connect mydevbox --binary target/x86_64-unknown-linux-gnu/release/cliptunnel --x11

# If you use mosh (not SSH), install a persistent tunnel:
./target/release/cliptunnel tunnel mydevbox --install
```

### 3. Use

1. Copy a screenshot on your Mac (Cmd+Shift+4, etc.)
2. Switch to your remote coding agent (Claude Code, Copilot CLI, etc.)
3. Press **Ctrl+V**

That's it. No commands to run, no terminals to keep open.

## How CLIs Read Clipboard (and why two layers)

| CLI | Paste key | Method | ClipTunnel layer | Status |
|-----|-----------|--------|-----------------|--------|
| Claude Code | Ctrl+V | `xclip -selection clipboard -t image/png -o` | **Shim** | **Working** |
| Codex CLI | Ctrl+V | X11 CLIPBOARD selection (native) | **X11 bridge** (needs `--x11`) | **Working** |
| Copilot CLI | **Alt+V** | `@teddyzhu/clipboard` native addon (X11) | **X11 bridge** (needs `--x11`) | **Working** |
| Gemini CLI | Ctrl+V | `xclip` or `wl-paste` | **Shim** | **Working** |
| OpenCode CLI | Ctrl+V | `xclip` or `wl-paste` | **Shim** | **Working** |

> **Note:** Copilot CLI uses **Alt+V** (not Ctrl+V) for image paste. Ctrl+V in Copilot CLI only pastes text.

## Commands

```
cliptunnel daemon --install       # Install daemon as launchd service (Mac)
cliptunnel daemon --foreground    # Run daemon in foreground (for debugging)
cliptunnel daemon --uninstall     # Remove launchd service

cliptunnel connect <host>         # Deploy to remote: SSH config + binary + shims
cliptunnel connect <host> --x11   # Also set up X11 bridge for Codex/Copilot

cliptunnel tunnel <host> --install   # Persistent SSH tunnel (for mosh users)
cliptunnel tunnel <host> --uninstall # Remove tunnel service

cliptunnel disconnect <host>         # Remove SSH forwarding
cliptunnel disconnect <host> --clean # Also remove remote shims/binary

cliptunnel doctor                    # Check local daemon status
cliptunnel doctor --host <host>      # Check full pipeline (local + remote)

cliptunnel gc                        # Clean up old temp images on remote
cliptunnel paste                     # Manual fallback: save clipboard to file
cliptunnel paste --tmux              # Save and send path to current tmux pane
```

## Architecture

**Port:** 18442 (fixed, localhost only)

**Auth:** Bearer token stored at:
- Mac: `~/Library/Application Support/cliptunnel/token`
- Linux: `~/.config/cliptunnel/token`

**SSH:** `RemoteForward 18442 127.0.0.1:18442` added to `~/.ssh/config`

**Shims:** Bash scripts at `~/.local/bin/{xclip,xsel,wl-paste}` that:
- Detect clipboard image read requests (specific flag patterns)
- Fetch from `localhost:18442/clipboard` via curl with bearer token
- Fall through to real binaries for all other operations

**X11 bridge:** Xvfb virtual display + polling script that:
- Checks `/clipboard/metadata` every 2 seconds for changes
- On change, fetches image and pushes to Xvfb clipboard via real xclip
- Both Unix socket (`:99`) and TCP (`127.0.0.1:99`) access

## Troubleshooting

```bash
# Check everything
cliptunnel doctor --host mydevbox

# Daemon not running?
cliptunnel daemon --install

# Tunnel not working? (mosh users)
cliptunnel tunnel mydevbox --install

# Shims not in PATH?
# Add to your remote .bashrc/.zshrc:
export PATH="$HOME/.local/bin:$PATH"

# X11 bridge not working?
# On remote: export DISPLAY=:99
# Check: DISPLAY=:99 /usr/bin/xclip -selection clipboard -t TARGETS -o
# Should show: image/png

# Clipboard empty?
# Copy an image on Mac first, then check:
curl -s -H "Authorization: Bearer $(cat ~/Library/Application\ Support/cliptunnel/token)" \
  http://127.0.0.1:18442/clipboard/metadata
```

## What Survives Reboots

| Component | Survives Mac reboot | Survives Linux reboot |
|-----------|--------------------|-----------------------|
| Daemon | Yes (launchd) | N/A (Mac only) |
| Tunnel | Yes (launchd) | N/A (Mac only) |
| Shims | N/A | Yes (files on disk) |
| X11 bridge | N/A | **No** — run `cliptunnel connect <host> --x11` again |
| Token | Yes (file) | Yes (file) |
| SSH config | Yes (file) | N/A |

## License

MIT
