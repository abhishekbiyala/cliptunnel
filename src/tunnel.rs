use anyhow::{Context, Result};
use colored::Colorize;
use std::fs;
use std::path::PathBuf;
use std::time::Duration;
use tokio::process::Command;

const LABEL_PREFIX: &str = "dev.cliptunnel.tunnel";

fn plist_path(host: &str) -> PathBuf {
    let safe_host = host.replace(['@', '.', '/'], "-");
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join("Library/LaunchAgents")
        .join(format!("{LABEL_PREFIX}.{safe_host}.plist"))
}

fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
        .replace('\'', "&apos;")
}

fn generate_plist(host: &str, port: u16) -> Result<String> {
    let binary = std::env::current_exe().context("cannot determine binary path")?;
    let binary_str = xml_escape(&binary.display().to_string());
    let safe_host = host.replace(['@', '.', '/'], "-");
    let host_escaped = xml_escape(host);
    let label = format!("{LABEL_PREFIX}.{safe_host}");
    let log_dir = crate::config::data_dir();

    Ok(format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>Label</key>
    <string>{label}</string>
    <key>ProgramArguments</key>
    <array>
        <string>{binary_str}</string>
        <string>tunnel</string>
        <string>{host_escaped}</string>
        <string>--port</string>
        <string>{port}</string>
    </array>
    <key>RunAtLoad</key>
    <true/>
    <key>KeepAlive</key>
    <true/>
    <key>LimitLoadToSessionType</key>
    <string>Aqua</string>
    <key>ThrottleInterval</key>
    <integer>10</integer>
    <key>StandardOutPath</key>
    <string>{log_dir}/tunnel-{safe_host}.log</string>
    <key>StandardErrorPath</key>
    <string>{log_dir}/tunnel-{safe_host}.err.log</string>
</dict>
</plist>
"#,
        log_dir = log_dir.display()
    ))
}

pub async fn run(host: &str, install: bool, uninstall: bool, port: u16) -> Result<()> {
    crate::connect::ssh_config::validate_host(host)?;
    if install {
        return install_service(host, port);
    }
    if uninstall {
        return uninstall_service(host);
    }

    // Run the tunnel (blocking, with auto-reconnect)
    run_tunnel_loop(host, port).await
}

fn install_service(host: &str, port: u16) -> Result<()> {
    let plist = plist_path(host);
    let content = generate_plist(host, port)?;

    if let Some(parent) = plist.parent() {
        fs::create_dir_all(parent)?;
    }
    let log_dir = crate::config::data_dir();
    fs::create_dir_all(&log_dir)?;

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

    println!(
        "{}",
        format!("Tunnel to {host} installed as launchd service").green()
    );
    println!("  plist: {}", plist.display());
    println!("  logs:  {}/tunnel-*.log", log_dir.display());
    println!(
        "{}",
        "Tunnel will auto-start on login and reconnect on failure."
            .green()
            .bold()
    );
    Ok(())
}

fn uninstall_service(host: &str) -> Result<()> {
    let plist = plist_path(host);

    if !plist.exists() {
        println!("no tunnel plist found at {}", plist.display());
        return Ok(());
    }

    let _ = std::process::Command::new("launchctl")
        .args(["unload", &plist.to_string_lossy()])
        .output();

    fs::remove_file(&plist).with_context(|| format!("failed to remove {}", plist.display()))?;

    println!(
        "{}",
        format!("Tunnel service for {host} uninstalled").green()
    );
    Ok(())
}

async fn run_tunnel_loop(host: &str, port: u16) -> Result<()> {
    println!(
        "{}",
        format!("Maintaining SSH tunnel to {host} (port {port})...").green()
    );
    println!("Press Ctrl+C to stop.\n");

    loop {
        tracing::info!("starting SSH tunnel to {host}");

        let status = Command::new("ssh")
            .args([
                "-N", // No remote command
                "-o",
                "ExitOnForwardFailure=yes",
                "-o",
                "ServerAliveInterval=30",
                "-o",
                "ServerAliveCountMax=3",
                "-R",
                &format!("{port}:127.0.0.1:{port}"),
                "--",
                host,
            ])
            .status()
            .await
            .context("failed to run ssh")?;

        if status.success() {
            tracing::info!("SSH tunnel exited normally");
        } else {
            tracing::warn!("SSH tunnel exited with status {status}, reconnecting in 5s...");
        }

        // Wait before reconnecting
        tokio::time::sleep(Duration::from_secs(5)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn xml_escape_no_special_chars() {
        assert_eq!(xml_escape("hello world"), "hello world");
    }

    #[test]
    fn xml_escape_ampersand() {
        assert_eq!(xml_escape("a&b"), "a&amp;b");
    }

    #[test]
    fn xml_escape_all_special_chars() {
        assert_eq!(
            xml_escape(r#"<tag attr="val" & 'x'>"#),
            "&lt;tag attr=&quot;val&quot; &amp; &apos;x&apos;&gt;"
        );
    }

    #[test]
    fn xml_escape_empty_string() {
        assert_eq!(xml_escape(""), "");
    }

    #[test]
    fn xml_escape_multiple_ampersands() {
        assert_eq!(xml_escape("a&&b"), "a&amp;&amp;b");
    }

    #[test]
    fn xml_escape_already_escaped_not_double_escaped_except_ampersand() {
        assert_eq!(xml_escape("&amp;"), "&amp;amp;");
    }

    #[test]
    fn plist_path_sanitizes_special_chars() {
        let path = plist_path("user@host.example.com");
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(
            filename,
            "dev.cliptunnel.tunnel.user-host-example-com.plist"
        );
    }

    #[test]
    fn plist_path_simple_host() {
        let path = plist_path("devbox");
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "dev.cliptunnel.tunnel.devbox.plist");
    }

    #[test]
    fn plist_path_lives_in_launch_agents() {
        let path = plist_path("myhost");
        let parent = path.parent().unwrap();
        assert!(
            parent.ends_with("Library/LaunchAgents"),
            "plist should be in ~/Library/LaunchAgents, got {}",
            parent.display()
        );
    }

    #[test]
    fn plist_path_with_slashes() {
        let path = plist_path("user/host");
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(
            !filename.contains('/'),
            "filename should not contain slashes"
        );
    }

    #[test]
    fn generate_plist_contains_host_and_port() {
        let plist = generate_plist("devbox", 18442).unwrap();
        assert!(plist.contains("<string>devbox</string>"));
        assert!(plist.contains("<string>18442</string>"));
    }

    #[test]
    fn generate_plist_is_valid_xml_structure() {
        let plist = generate_plist("devbox", 18442).unwrap();
        assert!(plist.starts_with("<?xml version=\"1.0\""));
        assert!(plist.contains("<plist version=\"1.0\">"));
        assert!(plist.contains("</plist>"));
    }

    #[test]
    fn generate_plist_escapes_host_with_special_xml_chars() {
        let plist = generate_plist("host-ok", 9999).unwrap();
        assert!(plist.contains("<string>host-ok</string>"));
    }

    #[test]
    fn generate_plist_label_uses_safe_host() {
        let plist = generate_plist("user@host.com", 18442).unwrap();
        assert!(plist.contains("dev.cliptunnel.tunnel.user-host-com"));
    }

    #[test]
    fn generate_plist_has_keep_alive_and_run_at_load() {
        let plist = generate_plist("devbox", 18442).unwrap();
        assert!(plist.contains("<key>KeepAlive</key>"));
        assert!(plist.contains("<key>RunAtLoad</key>"));
    }
}
