# ClipTunnel

Transparent clipboard image proxy: Mac → SSH tunnel → remote Linux. Single Rust binary.

## Tech Stack

- **Language**: Rust 1.88+ (edition 2021)
- **Async runtime**: tokio 1.49
- **HTTP server**: axum 0.8 (local daemon)
- **Clipboard**: arboard 3.6 (macOS, feature-gated)
- **CLI**: clap 4.6 (derive macros)
- **Image**: image 0.25 (PNG/JPEG encoding)
- **Dev tools**: mise (Rust toolchain), cross (Linux cross-compile)

## Project Structure

```
src/
  main.rs              # CLI entry point, subcommand dispatch
  lib.rs               # Library crate (exposes modules for integration tests)
  cli.rs               # clap derive structs
  config.rs            # Token management, DEFAULT_PORT, write_auth_config
  daemon/
    mod.rs             # Daemon orchestration
    server.rs          # axum HTTP server (health, clipboard, metadata endpoints)
    clipboard.rs       # arboard clipboard reading + PNG encoding + SHA256 cache
    launchd.rs         # macOS LaunchAgent plist generation
  connect/
    mod.rs             # Connect orchestration (health check, deploy)
    ssh_config.rs      # ~/.ssh/config parser/modifier (RemoteForward)
    deploy.rs          # SCP binary + token, SSH install-remote
  remote/
    mod.rs
    install.rs         # Deploy shims to ~/.local/bin
    shims.rs           # Embedded shim script content (include_str!)
    x11_bridge.rs      # Xvfb + xauth + clipboard owner daemon
    gc.rs              # Temp file cleanup (/tmp/cliptunnel-*.png)
  tunnel.rs            # Persistent SSH tunnel with auto-reconnect + launchd
  doctor.rs            # Diagnostic checks (local + remote, colored output)
  disconnect.rs        # Teardown SSH config + remote cleanup
  paste.rs             # Manual clipboard fetch + tmux integration
shims/
  xclip.sh             # Bash xclip wrapper (deployed to remote)
  xsel.sh              # Bash xsel wrapper
  wl-paste.sh          # Bash wl-paste wrapper
tests/
  daemon_test.rs       # Integration tests (axum router, auth middleware)
  e2e/test_full_flow.sh  # End-to-end test script (needs real Mac + Linux)
```

## Build & Test Commands

```bash
# Dev build
cargo build

# Release build (Mac)
cargo build --release

# Cross-compile Linux binary (requires Docker)
cross build --release --target x86_64-unknown-linux-gnu --no-default-features

# Lint
cargo clippy

# Unit tests (CI-safe, no external deps)
cargo test --lib

# Integration tests (CI-safe, no clipboard access needed)
cargo test --test daemon_test

# All tests including clipboard access (needs macOS GUI)
cargo test -- --include-ignored

# E2E (manual, needs Mac + remote Linux host)
CLIPTUNNEL_TEST_HOST=user@host ./tests/e2e/test_full_flow.sh
```

## Code Style

- Error handling: `anyhow::Result` everywhere (no custom error types)
- Logging: `tracing` macros (`tracing::info!`, `tracing::warn!`, etc.)
- Async: `tokio::process::Command` for shell-outs, never blocking `std::process::Command` in async contexts (except in `remote/` modules which run synchronously on Linux)
- CLI: clap derive macros, subcommand pattern
- No `unwrap()` in production paths — use `context()` or `bail!()`
- Feature gates: `#[cfg(feature = "daemon")]` for macOS-only clipboard code

Example style:
```rust
pub async fn run(host: &str) -> Result<()> {
    let token = config::load_token()
        .context("failed to load token")?;
    tracing::info!("connecting to {host}");
    // ...
    Ok(())
}
```

## Security Boundaries

### Always do
- Pass bearer tokens via `curl -K <config-file>` (temp file with 0600 perms), never via `-H` CLI args (visible in `/proc`)
- Keep `NamedTempFile` handles alive until done — don't drop and recreate
- Validate SSH host names with `validate_host()` before writing to config/plists
- XML-escape values before embedding in launchd plists
- Check `symlink_metadata()` before writing to shell rc files
- Enforce 0600 permissions on token files (fix if insecure)
- Bind daemon to `127.0.0.1` only
- Use `xauth` cookies for Xvfb (no `-ac` flag)
- Track PIDs via files, kill only tracked processes (no `pkill Xvfb`)

### Never do
- Pass secrets as CLI arguments to any process
- Use `StrictHostKeyChecking=accept-new` (let user's SSH config handle it)
- Use `-ac` (disable access control) on Xvfb
- Use `pkill` to kill processes by name (kills other users' processes)
- Return internal error strings in HTTP responses
- Expose version in unauthenticated endpoints
- Follow symlinks when writing to shell rc files
- Use predictable temp file paths (always use `tempfile::Builder`)

### Ask first
- Modifying `~/.ssh/config` (we back up to `.cliptunnel.bak`)
- Installing launchd services
- Writing to `.bashrc` / `.zshrc`

## Architecture Notes

- **Port 18442** is the default (defined as `DEFAULT_PORT` in `config.rs`)
- **Two clipboard layers**: xclip/xsel/wl-paste shims (for CLIs that shell out to clipboard tools) and X11/Xvfb bridge (for CLIs that read X11 CLIPBOARD selection directly via native libs)
- **Feature flags**: `daemon` (default, macOS) includes arboard + image. Linux cross-compile uses `--no-default-features` to exclude them.
- **SSH config**: `RemoteForward` is persistent in `~/.ssh/config`, not per-session. For mosh users, `cliptunnel tunnel` maintains a separate SSH connection.
- **Shims fall through**: if the tunnel is down, shims exec the real binary transparently

## Git Conventions

- Conventional commits: `feat:`, `fix:`, `security:`, `test:`, `docs:`, `chore:`
- Branch pattern: `feature/*`
- Signed commits required
