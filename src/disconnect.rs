use anyhow::{Context, Result};
use colored::Colorize;
use tokio::process::Command;

use crate::connect::ssh_config;

pub async fn run(host: &str, clean: bool) -> Result<()> {
    // Remove RemoteForward from ~/.ssh/config
    ssh_config::remove_remote_forward(host)
        .context("failed to remove RemoteForward from SSH config")?;
    println!(
        "{}",
        format!("Removed RemoteForward from SSH config for {host}").green()
    );

    // If clean flag, remove remote artifacts
    if clean {
        println!("Cleaning up remote host {host}...");

        let cleanup_script = concat!(
            "rm -f ~/.local/bin/xclip ~/.local/bin/xsel ~/.local/bin/wl-paste ~/.local/bin/cliptunnel; ",
            "rm -f ~/.local/bin/cliptunnel-x11-owner; ",
            "rm -f ~/.config/cliptunnel/token; ",
            "rm -f /tmp/cliptunnel-*.png; ",
            // Kill only our tracked processes via PID files, not all Xvfb instances
            "for f in ~/.local/share/cliptunnel/xvfb.pid ~/.local/share/cliptunnel/x11-owner.pid; do ",
            "  [ -f \"$f\" ] && kill $(cat \"$f\") 2>/dev/null; rm -f \"$f\"; ",
            "done; ",
            "rm -rf ~/.local/share/cliptunnel",
        );

        let output = Command::new("ssh")
            .arg(host)
            .arg("--")
            .arg("bash")
            .arg("-c")
            .arg(cleanup_script)
            .output()
            .await
            .context("failed to SSH to remote host for cleanup")?;

        if output.status.success() {
            println!("{}", "Remote cleanup complete".green());
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr);
            tracing::warn!("remote cleanup had issues: {}", stderr.trim());
            println!(
                "{}",
                "Remote cleanup completed with warnings (check logs)".yellow()
            );
        }
    }

    println!("{}", format!("Disconnected from {host}").green().bold());

    Ok(())
}
