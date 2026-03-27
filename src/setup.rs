use anyhow::{Context, Result};
use colored::Colorize;
use std::path::Path;
use tokio::process::Command;

#[cfg(feature = "daemon")]
use crate::config;
use crate::config::DEFAULT_PORT as PORT;

/// One-command setup: check SSH, install daemon, connect to remote, verify.
pub async fn run(host: &str, x11: bool, binary: Option<&Path>, arch: &str) -> Result<()> {
    // Validate host before any SSH calls
    crate::connect::ssh_config::validate_host(host)?;

    println!();
    println!("{}", "ClipTunnel Setup".bold());
    println!();

    // Step 1: Check SSH access
    check_ssh(host).await?;

    // Step 2: Install daemon if needed
    install_daemon_if_needed().await?;

    // Step 3: Connect (SSH config + deploy)
    println!("  {} Deploying to {}...", "▸".bold(), host.bold());
    crate::connect::run(host, x11, binary, arch).await?;
    println!("  {} SSH config updated (RemoteForward)", "✓".green());
    println!("  {} Linux binary deployed", "✓".green());
    println!("  {} Clipboard shims installed", "✓".green());
    if x11 {
        println!("  {} X11 bridge (Xvfb) started", "✓".green());
    }

    // Step 4: Verify
    println!();
    println!("  {} Verifying...", "▸".bold());
    crate::doctor::run(Some(host)).await?;

    println!();
    println!(
        "{}",
        "  Done! Copy an image on your Mac, paste in your remote coding agent."
            .green()
            .bold()
    );
    println!();

    Ok(())
}

/// Check if SSH to host works. Warn if password-based (BatchMode fails).
async fn check_ssh(host: &str) -> Result<()> {
    println!("  {} Checking SSH access to {}...", "▸".bold(), host.bold());

    // Try BatchMode first — succeeds only with key-based auth
    let batch_result = Command::new("ssh")
        .args([
            "-o",
            "BatchMode=yes",
            "-o",
            "ConnectTimeout=10",
            host,
            "true",
        ])
        .output()
        .await
        .context("failed to run ssh")?;

    if batch_result.status.success() {
        println!("  {} SSH key-based auth working", "✓".green());
        return Ok(());
    }

    // BatchMode failed — could be password auth or host unreachable.
    // Try without BatchMode to distinguish.
    let regular_result = Command::new("ssh")
        .args(["-o", "ConnectTimeout=10", host, "true"])
        .output()
        .await
        .context("failed to run ssh")?;

    if regular_result.status.success() {
        // SSH works with password — warn but continue
        println!(
            "  {} {}",
            "⚠".yellow(),
            "Password-based SSH detected. You may be prompted a few times.".yellow()
        );
        println!(
            "    {} For a smoother experience, set up SSH keys:",
            "Tip:".bold()
        );
        println!("    ssh-copy-id {}", host);
        println!();
        return Ok(());
    }

    // SSH doesn't work at all
    let stderr = String::from_utf8_lossy(&regular_result.stderr);
    anyhow::bail!(
        "cannot SSH into '{}'. Make sure you can run: ssh {}\n  Error: {}",
        host,
        host,
        stderr.trim()
    );
}

/// Install the daemon via launchd if not already running.
async fn install_daemon_if_needed() -> Result<()> {
    println!("  {} Checking local clipboard daemon...", "▸".bold());

    // Check if daemon is already running
    let url = format!("http://127.0.0.1:{}/health", PORT);
    let health_ok = Command::new("curl")
        .args(["-sf", "--max-time", "3", &url])
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false);

    if health_ok {
        println!("  {} Daemon already running on port {}", "✓".green(), PORT);
        return Ok(());
    }

    // Not running — install via launchd
    println!("  {} Installing daemon...", "▸".bold());

    #[cfg(feature = "daemon")]
    {
        crate::daemon::launchd::install(PORT)?;

        // Wait for daemon to start (15 seconds — macOS may show a permissions dialog)
        for _ in 0..30 {
            tokio::time::sleep(std::time::Duration::from_millis(500)).await;
            let ok = Command::new("curl")
                .args(["-sf", "--max-time", "2", &url])
                .output()
                .await
                .map(|o| o.status.success())
                .unwrap_or(false);
            if ok {
                println!("  {} Daemon running on port {}", "✓".green(), PORT);
                return Ok(());
            }
        }

        let log_path = config::data_dir().join("daemon.err.log");
        let log_tail = std::fs::read_to_string(&log_path)
            .unwrap_or_default()
            .lines()
            .rev()
            .take(5)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect::<Vec<_>>()
            .join("\n");
        anyhow::bail!(
            "daemon installed but not responding on port {}.\n  Log output ({}):\n{}",
            PORT,
            log_path.display(),
            log_tail
        );
    }

    #[cfg(not(feature = "daemon"))]
    {
        anyhow::bail!("the setup command requires the 'daemon' feature (macOS only)");
    }
}
