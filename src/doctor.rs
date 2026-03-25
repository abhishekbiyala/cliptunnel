use anyhow::Result;
use colored::Colorize;
use tokio::process::Command;

use crate::config;

fn pass(msg: &str) {
    println!("  {} {}", "✓".green(), msg);
}

fn fail(msg: &str) {
    println!("  {} {}", "✗".red(), msg);
}

async fn curl_ok(url: &str, token: Option<&str>) -> bool {
    let tmp_file = if let Some(t) = token {
        match config::write_auth_config(t) {
            Ok(f) => Some(f),
            Err(_) => return false,
        }
    } else {
        None
    };

    let mut cmd = Command::new("curl");
    cmd.arg("-s")
        .arg("-o")
        .arg("/dev/null")
        .arg("-w")
        .arg("%{http_code}");
    if let Some(ref f) = tmp_file {
        cmd.arg("-K").arg(f.path());
    }
    cmd.arg(url);

    let result = match cmd.output().await {
        Ok(output) => {
            let code = String::from_utf8_lossy(&output.stdout);
            code.trim() == "200"
        }
        Err(_) => false,
    };

    drop(tmp_file); // auto-deletes
    result
}

pub async fn run(host: Option<&str>) -> Result<()> {
    println!("{}", "ClipTunnel Doctor".bold());
    println!();

    let mut passed = 0u32;
    let mut failed = 0u32;

    // Check 1: Local daemon running
    if curl_ok("http://127.0.0.1:18442/health", None).await {
        pass("Local daemon is running");
        passed += 1;
    } else {
        fail("Local daemon is NOT running (http://127.0.0.1:18442/health)");
        failed += 1;
    }

    // Check 2: Token exists
    let token_path = config::token_path();
    let token = if token_path.exists() {
        pass(&format!("Token exists at {}", token_path.display()));
        passed += 1;
        config::load_token().ok()
    } else {
        fail(&format!("Token not found at {}", token_path.display()));
        failed += 1;
        None
    };

    // Check 3: Clipboard has image
    if curl_ok(
        "http://127.0.0.1:18442/clipboard/metadata",
        token.as_deref(),
    )
    .await
    {
        pass("Clipboard has image data");
        passed += 1;
    } else {
        fail("Clipboard does NOT have image data (or daemon unreachable)");
        failed += 1;
    }

    // Remote checks
    if let Some(host) = host {
        println!();
        println!("{}", format!("Remote checks for {host}").bold());

        // Check 4: SSH config has RemoteForward
        let ssh_config_path = dirs::home_dir()
            .unwrap_or_default()
            .join(".ssh")
            .join("config");
        if ssh_config_path.exists() {
            let content = std::fs::read_to_string(&ssh_config_path).unwrap_or_default();
            if has_remote_forward(&content, host) {
                pass("SSH config has RemoteForward 18442");
                passed += 1;
            } else {
                fail("SSH config missing RemoteForward 18442 for this host");
                failed += 1;
            }
        } else {
            fail("SSH config file not found");
            failed += 1;
        }

        // Check 5: Remote binary installed
        let output = Command::new("ssh")
            .arg(host)
            .arg("which")
            .arg("cliptunnel")
            .output()
            .await;
        if output.map(|o| o.status.success()).unwrap_or(false) {
            pass("Remote cliptunnel binary installed");
            passed += 1;
        } else {
            fail("Remote cliptunnel binary NOT found");
            failed += 1;
        }

        // Check 6: Remote shims in place
        let output = Command::new("ssh")
            .arg(host)
            .arg("ls")
            .arg("~/.local/bin/xclip")
            .output()
            .await;
        if output.map(|o| o.status.success()).unwrap_or(false) {
            pass("Remote xclip shim in place");
            passed += 1;
        } else {
            fail("Remote xclip shim NOT found at ~/.local/bin/xclip");
            failed += 1;
        }

        // Check 7: Tunnel connectivity
        // Write auth to a temp file on the remote to avoid token in process list
        let tunnel_ok = {
            let cmd = concat!(
                "CFG=$(mktemp /tmp/cliptunnel-chk.XXXXXX); ",
                "chmod 600 \"$CFG\"; ",
                "printf 'header = \"Authorization: Bearer %s\"\\n' \"$(cat ~/.config/cliptunnel/token)\" > \"$CFG\"; ",
                "CODE=$(curl -s -o /dev/null -w %{http_code} -K \"$CFG\" http://127.0.0.1:18442/health); ",
                "rm -f \"$CFG\"; ",
                "echo $CODE",
            );
            let output = Command::new("ssh").arg(host).arg(cmd).output().await;
            output
                .map(|o| String::from_utf8_lossy(&o.stdout).trim() == "200")
                .unwrap_or(false)
        };
        if tunnel_ok {
            pass("SSH tunnel connectivity OK");
            passed += 1;
        } else {
            fail("SSH tunnel connectivity FAILED (curl localhost:18442/health on remote)");
            failed += 1;
        }

        // Check 8: Xvfb running
        let output = Command::new("ssh")
            .arg(host)
            .arg("pgrep")
            .arg("Xvfb")
            .output()
            .await;
        if output.map(|o| o.status.success()).unwrap_or(false) {
            pass("Xvfb is running on remote");
            passed += 1;
        } else {
            fail("Xvfb is NOT running on remote (X11 bridge disabled or not started)");
            failed += 1;
        }
    }

    println!();
    let total = passed + failed;
    if failed == 0 {
        println!("{}", format!("All {total} checks passed!").green().bold());
    } else {
        println!(
            "{}",
            format!("{passed}/{total} checks passed, {failed} failed")
                .yellow()
                .bold()
        );
    }

    Ok(())
}

/// Check if the SSH config has a RemoteForward 18442 entry within the Host block for the given host.
fn has_remote_forward(config: &str, host: &str) -> bool {
    let mut in_host_block = false;
    for line in config.lines() {
        let trimmed = line.trim();
        if trimmed.to_lowercase().starts_with("host ") {
            let hosts: Vec<&str> = trimmed[5..].split_whitespace().collect();
            in_host_block = hosts.contains(&host);
        } else if in_host_block
            && trimmed.to_lowercase().contains("remoteforward")
            && trimmed.contains("18442")
        {
            return true;
        }
    }
    false
}
