use anyhow::{bail, Context, Result};
use std::path::Path;
use tokio::process::Command;

use crate::config;

const REMOTE_BIN_DIR: &str = ".local/bin";
const REMOTE_CONFIG_DIR: &str = ".config/cliptunnel";
const REMOTE_BINARY_NAME: &str = "cliptunnel";

/// Deploy the cliptunnel binary and token to the remote host, then run install-remote.
pub async fn deploy_to_remote(
    host: &str,
    binary: Option<&Path>,
    arch: &str,
    x11: bool,
) -> Result<()> {
    let binary_path = resolve_binary(binary, arch)?;
    tracing::info!("using binary: {}", binary_path.display());

    // Ensure remote directories exist
    tracing::debug!("creating remote directories");
    run_ssh(
        host,
        &format!("mkdir -p ~/{} ~/{}", REMOTE_BIN_DIR, REMOTE_CONFIG_DIR),
    )
    .await
    .context("failed to create remote directories")?;

    // SCP the binary
    let remote_bin = format!("{}:~/{}/{}", host, REMOTE_BIN_DIR, REMOTE_BINARY_NAME);
    tracing::info!("copying binary to {}", remote_bin);
    run_scp(&binary_path, &remote_bin)
        .await
        .context("failed to SCP binary to remote")?;

    // Make it executable
    run_ssh(
        host,
        &format!("chmod +x ~/{}/{}", REMOTE_BIN_DIR, REMOTE_BINARY_NAME),
    )
    .await
    .context("failed to chmod remote binary")?;

    // SCP the token
    let token_path = config::token_path();
    if !token_path.exists() {
        bail!(
            "token not found at {}; run the daemon first",
            token_path.display()
        );
    }
    let remote_token = format!("{}:~/{}/token", host, REMOTE_CONFIG_DIR);
    tracing::info!("copying token to remote");
    run_scp(&token_path, &remote_token)
        .await
        .context("failed to SCP token to remote")?;

    // Chmod the token on remote
    run_ssh(host, &format!("chmod 600 ~/{}/token", REMOTE_CONFIG_DIR))
        .await
        .context("failed to chmod remote token")?;

    // Run install-remote via SSH
    let mut install_cmd = format!("~/{}/{} install-remote", REMOTE_BIN_DIR, REMOTE_BINARY_NAME);
    if x11 {
        install_cmd.push_str(" --x11");
    }
    tracing::info!("running install-remote on {}", host);
    run_ssh(host, &install_cmd)
        .await
        .context("failed to run install-remote on remote")?;

    tracing::info!("deployment to {} complete", host);
    Ok(())
}

/// Resolve which binary to deploy. If the user provided one, use it.
/// Otherwise, look for a pre-built binary in standard locations.
fn resolve_binary(binary: Option<&Path>, arch: &str) -> Result<std::path::PathBuf> {
    if let Some(path) = binary {
        if !path.exists() {
            bail!("specified binary does not exist: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    // Try to find a binary in common locations
    let target = match arch {
        "x86_64" | "x86-64" => "x86_64-unknown-linux-gnu",
        "aarch64" | "arm64" => "aarch64-unknown-linux-gnu",
        other => bail!("unsupported architecture: {}", other),
    };

    // Check target/release for cross-compiled binary
    let candidates = [
        format!("target/{}/release/cliptunnel", target),
        "target/release/cliptunnel".to_string(),
    ];

    for candidate in &candidates {
        let p = std::path::Path::new(candidate);
        if p.exists() {
            tracing::info!("found binary at {}", p.display());
            return Ok(p.to_path_buf());
        }
    }

    bail!(
        "no binary found for arch '{}'. Either cross-compile or pass --binary <path>",
        arch
    );
}

async fn run_ssh(host: &str, command: &str) -> Result<()> {
    tracing::debug!("ssh {} -- {}", host, command);
    let output = Command::new("ssh")
        .arg(host)
        .arg("--")
        .arg(command)
        .output()
        .await
        .context("failed to execute ssh")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("ssh command failed: {}", stderr.trim());
    }

    Ok(())
}

async fn run_scp(local: &Path, remote: &str) -> Result<()> {
    tracing::debug!("scp {} {}", local.display(), remote);
    let output = Command::new("scp")
        .arg(local)
        .arg(remote)
        .output()
        .await
        .context("failed to execute scp")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("scp failed: {}", stderr.trim());
    }

    Ok(())
}

/// Detect the remote architecture via `uname -m`.
pub async fn detect_remote_arch(host: &str) -> Result<String> {
    tracing::debug!("detecting remote architecture via ssh {} uname -m", host);
    let output = Command::new("ssh")
        .arg(host)
        .arg("uname")
        .arg("-m")
        .output()
        .await
        .context("failed to detect remote architecture")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("failed to detect remote arch: {}", stderr.trim());
    }

    let arch = String::from_utf8_lossy(&output.stdout).trim().to_string();
    tracing::info!("remote architecture: {}", arch);
    Ok(arch)
}
