use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

const LABEL: &str = "dev.cliptunnel.daemon";

fn plist_path() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join("Library/LaunchAgents")
        .join(format!("{LABEL}.plist"))
}

fn generate_plist(port: u16) -> Result<String> {
    let binary = std::env::current_exe().context("cannot determine binary path")?;
    let binary_str = binary.display();
    let log_dir = crate::config::data_dir();

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{LABEL}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary_str}</string>
        <string>daemon</string>
        <string>--foreground</string>
        <string>--port</string>
        <string>{port}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>StandardOutPath</key>
    <string>{log_dir}/daemon.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/daemon.err.log</string>
</dict>
</plist>
"#,
        log_dir = log_dir.display()
    ))
}

pub fn install(port: u16) -> Result<()> {
    let plist = plist_path();
    let content = generate_plist(port)?;

    // Ensure directories exist
    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)?;
    }
    let log = crate::config::data_dir();
    fs::create_dir_all(&log)?;

    // Unload first if already installed
    if plist.exists() {
        let _ = std::process::Command::new("launchctl")
            .args(["unload", &plist.to_string_lossy()])
            .output();
    }

    fs::write(&plist, &content)
        .with_context(|| format!("failed to write plist to {}", plist.display()))?;

    let output = std::process::Command::new("launchctl")
        .args(["load", &plist.to_string_lossy()])
        .output()
        .context("failed to run launchctl load")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!("launchctl load failed: {stderr}");
    }

    tracing::info!("installed and loaded {}", plist.display());
    println!("daemon installed: {}", plist.display());
    println!("logs at: {}/daemon.log", log.display());
    Ok(())
}

pub fn uninstall() -> Result<()> {
    let plist = plist_path();

    if !plist.exists() {
        println!("no plist found at {}", plist.display());
        return Ok(());
    }

    let output = std::process::Command::new("launchctl")
        .args(["unload", &plist.to_string_lossy()])
        .output()
        .context("failed to run launchctl unload")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        tracing::warn!("launchctl unload warning: {stderr}");
    }

    fs::remove_file(&plist).with_context(|| format!("failed to remove {}", plist.display()))?;

    tracing::info!("uninstalled {}", plist.display());
    println!("daemon uninstalled");
    Ok(())
}
