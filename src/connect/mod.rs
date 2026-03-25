pub mod deploy;
pub mod ssh_config;

use anyhow::{Context, Result};
use std::path::Path;
use tokio::process::Command;

use crate::config;

use config::DEFAULT_PORT as PORT;

/// Orchestrate connecting to a remote host:
/// 1. Check that the local daemon is healthy
/// 2. Ensure a token exists
/// 3. Modify SSH config to add RemoteForward
/// 4. Detect remote architecture (if not overridden)
/// 5. Deploy binary + token and run install-remote
pub async fn run(host: &str, x11: bool, binary: Option<&Path>, arch: &str) -> Result<()> {
    // Step 1: Check local daemon health
    tracing::info!("checking local daemon health...");
    check_daemon_health()
        .await
        .context("local daemon is not running; start it with `cliptunnel daemon --foreground`")?;
    tracing::info!("local daemon is healthy");

    // Step 2: Ensure token exists
    let _token = config::load_or_create_token().context("failed to load or create auth token")?;
    tracing::info!("auth token ready");

    // Step 3: Modify SSH config
    tracing::info!("configuring SSH RemoteForward for host '{}'", host);
    ssh_config::add_remote_forward(host, PORT)
        .context("failed to add RemoteForward to SSH config")?;

    // Step 4: Detect remote architecture if the user passed default
    let effective_arch = if binary.is_some() {
        // If user provides a binary, trust their arch setting
        arch.to_string()
    } else {
        tracing::info!("detecting remote architecture...");
        let detected = deploy::detect_remote_arch(host)
            .await
            .context("failed to detect remote architecture")?;
        // Normalize uname -m output
        match detected.as_str() {
            "x86_64" => "x86_64".to_string(),
            "aarch64" | "arm64" => "aarch64".to_string(),
            other => {
                tracing::warn!("unusual remote arch '{}', using as-is", other);
                other.to_string()
            }
        }
    };
    tracing::info!("target architecture: {}", effective_arch);

    // Step 5: Deploy binary + token, run install-remote
    tracing::info!("deploying to {}...", host);
    deploy::deploy_to_remote(host, binary, &effective_arch, x11)
        .await
        .context("deployment failed")?;

    tracing::info!("connected to {} successfully", host);
    tracing::info!(
        "SSH RemoteForward {} configured -- use `ssh {}` to connect",
        PORT,
        host
    );

    Ok(())
}

/// Check that the local daemon is responding on the health endpoint.
/// Shells out to curl to avoid requiring reqwest on the local (Mac) side.
async fn check_daemon_health() -> Result<()> {
    let url = format!("http://127.0.0.1:{}/health", PORT);

    let output = Command::new("curl")
        .args(["-sf", "--max-time", "3", &url])
        .output()
        .await
        .context("failed to run curl -- is curl installed?")?;

    if !output.status.success() {
        anyhow::bail!(
            "daemon health check failed (is it running on port {}?)",
            PORT
        );
    }

    // Optionally verify the response contains "ok"
    let body = String::from_utf8_lossy(&output.stdout);
    if !body.contains("ok") {
        anyhow::bail!("daemon health check returned unexpected response: {}", body);
    }

    Ok(())
}
