# Testing

ClipTunnel has three tiers of tests, each with different requirements.

## Tier 1: Unit Tests

Pure logic tests. No external dependencies, no GUI, no network. Runs anywhere.

```bash
cargo test --lib
```

**What's covered (95 tests):**
- SSH config parser + host validation (39 tests): add/remove RemoteForward, adversarial hostnames, allowlist enforcement
- Token management (16 tests): create, load, permissions, format, edge cases
- Tunnel plist + XML escape (15 tests): generation, escaping, path sanitization
- GC (9 tests): old file deletion, preservation, pattern matching, edge cases
- Paste helpers (8 tests): temp path generation, auth config file creation
- HTTP server auth (8 tests): bearer token, rejection, route protection

**Requirements:** Rust toolchain only.

## Tier 2: Integration Tests

Test the HTTP server, auth middleware, and API contracts. Some tests require macOS GUI session for clipboard access.

```bash
# Run tests that don't need clipboard access (CI-safe)
cargo test --test daemon_test

# Run ALL integration tests including clipboard access (macOS GUI required)
cargo test --test daemon_test -- --ignored
```

**What's covered (7 pass + 2 ignored):**
- Health endpoint returns JSON `{"status":"ok"}` (no auth, no version exposed)
- Auth middleware rejects: no token, bad token, malformed header, empty bearer, non-Bearer scheme
- Clipboard endpoints return 204 when no image (requires macOS GUI — ignored by default)
- Unknown routes return 404, POST to GET-only routes returns 405

**Requirements:**
- Rust toolchain
- macOS (for `--ignored` clipboard tests — needs GUI session, won't work in headless CI)

## Tier 3: End-to-End Tests

Full pipeline test: local daemon → SSH tunnel → remote shims → X11 bridge. Requires a real Mac + real Linux remote host.

```bash
CLIPTUNNEL_TEST_HOST=user@mydevbox ./tests/e2e/test_full_flow.sh
```

**What's covered:**
1. Daemon starts and serves clipboard images
2. `cliptunnel connect` deploys binary, token, shims to remote
3. xclip shim intercepts TARGETS and image reads (Claude Code/Gemini CLI path)
4. X11 bridge populates Xvfb clipboard (Codex/Copilot CLI path)
5. `cliptunnel doctor` passes all checks
6. GC cleans old files on remote
7. `cliptunnel disconnect` removes SSH config

**Requirements:**
- macOS with GUI session (clipboard access)
- Docker (for `cross` Linux cross-compilation)
- Linux remote host with SSH access
- `xvfb` and `xclip` installed on remote (`sudo apt install xvfb xclip`)
- Pre-built binaries: `cargo build --release` and `cross build --release --target x86_64-unknown-linux-gnu --no-default-features`

**Setup steps:**
1. Build both binaries (Mac + Linux)
2. Set `CLIPTUNNEL_TEST_HOST=user@mydevbox`
3. Ensure you can `ssh $CLIPTUNNEL_TEST_HOST` without password prompt
4. Install `xvfb` and `xclip` on the remote host
5. Run the script

## CI Recommendations

For GitHub Actions or similar CI:

```yaml
# Unit tests — run on every PR
- name: Unit tests
  run: cargo test --lib

# Integration tests (no clipboard) — run on every PR
- name: Integration tests
  run: cargo test --test daemon_test

# E2E tests — manual trigger only
# Requires self-hosted runner with macOS + SSH to a test Linux host
```
