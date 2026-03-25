use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;
use std::process::Command;

fn find_free_display() -> Result<u32> {
    for n in 99..200 {
        let lock_file = format!("/tmp/.X{n}-lock");
        if !std::path::Path::new(&lock_file).exists() {
            return Ok(n);
        }
    }
    anyhow::bail!("no free X display found in range :99-:199")
}

fn xvfb_is_installed() -> bool {
    Command::new("which")
        .arg("Xvfb")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

fn real_xclip_path() -> Option<String> {
    for path in ["/usr/bin/xclip", "/usr/local/bin/xclip"] {
        if std::path::Path::new(path).exists() {
            return Some(path.to_string());
        }
    }
    None
}

fn generate_x11_owner_script(port: u16, display_num: u32, real_xclip: &str) -> String {
    format!(
        r#"#!/usr/bin/env bash
# cliptunnel X11 clipboard owner - polls tunnel and updates Xvfb clipboard
# Uses real xclip (not the shim) to push images to the Xvfb clipboard
# Auth token is passed via curl config file to avoid exposure in process list

CLIPTUNNEL_PORT="{port}"
CLIPTUNNEL_TOKEN_FILE="${{HOME}}/.config/cliptunnel/token"
DISPLAY=":{display_num}"
XAUTHORITY="${{HOME}}/.Xauthority"
export DISPLAY XAUTHORITY
REAL_XCLIP="{real_xclip}"

TOKEN=$(cat "$CLIPTUNNEL_TOKEN_FILE" 2>/dev/null || true)
if [ -z "$TOKEN" ]; then
    echo "cliptunnel x11-owner: no token found" >&2
    exit 1
fi

# Write auth to a curl config file (not CLI args) to avoid /proc exposure
AUTH_CONFIG=$(mktemp /tmp/cliptunnel-auth.XXXXXX)
chmod 600 "$AUTH_CONFIG"
printf 'header = "Authorization: Bearer %s"\n' "$TOKEN" > "$AUTH_CONFIG"
trap "rm -f '$AUTH_CONFIG'" EXIT

LAST_HASH=""

while true; do
    METADATA=$(curl -s -K "$AUTH_CONFIG" \
        "http://127.0.0.1:${{CLIPTUNNEL_PORT}}/clipboard/metadata" 2>/dev/null || true)

    if [ -n "$METADATA" ]; then
        HASH=$(echo "$METADATA" | grep -o '"sha256":"[^"]*"' | cut -d'"' -f4)
        if [ -n "$HASH" ] && [ "$HASH" != "$LAST_HASH" ]; then
            TMPFILE=$(mktemp /tmp/cliptunnel-x11-XXXXXXXXXX.png)
            HTTP_CODE=$(curl -s -o "$TMPFILE" -w "%{{http_code}}" \
                -K "$AUTH_CONFIG" \
                "http://127.0.0.1:${{CLIPTUNNEL_PORT}}/clipboard" 2>/dev/null || echo "000")
            if [ "$HTTP_CODE" = "200" ] && [ -s "$TMPFILE" ]; then
                "$REAL_XCLIP" -selection clipboard -t image/png -i < "$TMPFILE" 2>/dev/null || true
                LAST_HASH="$HASH"
            fi
            rm -f "$TMPFILE"
        fi
    fi

    sleep 2
done
"#
    )
}

fn owner_script_path() -> Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".local/bin/cliptunnel-x11-owner"))
}

pub async fn setup(port: u16) -> Result<()> {
    if !xvfb_is_installed() {
        println!("Xvfb is not installed. Please install it:");
        println!("  Debian/Ubuntu: sudo apt install xvfb xclip");
        println!("  RHEL/Fedora:   sudo dnf install xorg-x11-server-Xvfb xclip");
        anyhow::bail!("Xvfb not found");
    }

    let real_xclip = real_xclip_path()
        .context("real xclip not found at /usr/bin/xclip — install it: sudo apt install xclip")?;

    // Kill only OUR previous Xvfb/x11-owner (by PID file), not other users' processes
    let pid_file = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".local/share/cliptunnel/xvfb.pid");
    if pid_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            let pid = pid_str.trim();
            let _ = Command::new("kill").arg(pid).output();
            tracing::debug!("killed previous Xvfb (pid {pid})");
        }
        let _ = fs::remove_file(&pid_file);
    }
    let owner_pid_file = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".local/share/cliptunnel/x11-owner.pid");
    if owner_pid_file.exists() {
        if let Ok(pid_str) = fs::read_to_string(&owner_pid_file) {
            let pid = pid_str.trim();
            let _ = Command::new("kill").arg(pid).output();
            tracing::debug!("killed previous x11-owner (pid {pid})");
        }
        let _ = fs::remove_file(&owner_pid_file);
    }
    std::thread::sleep(std::time::Duration::from_millis(500));

    let display_num = find_free_display()?;
    tracing::info!("using display :{display_num}");

    // Generate xauth cookie for access control
    let xauth_file = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".Xauthority");
    let cookie: String = {
        use rand::RngCore;
        let mut bytes = [0u8; 16];
        rand::rng().fill_bytes(&mut bytes);
        hex::encode(bytes)
    };

    // Add auth entry for both unix and tcp access
    let _ = Command::new("xauth")
        .args([
            "-f",
            &xauth_file.to_string_lossy(),
            "add",
            &format!(":{display_num}"),
            ".",
            &cookie,
        ])
        .output();
    let _ = Command::new("xauth")
        .args([
            "-f",
            &xauth_file.to_string_lossy(),
            "add",
            &format!("127.0.0.1:{display_num}"),
            ".",
            &cookie,
        ])
        .output();

    tracing::info!("created xauth cookie for display :{display_num}");

    // Start Xvfb with TCP listening (needed for Codex sandbox) but WITH access control
    let xvfb = Command::new("Xvfb")
        .args([
            &format!(":{display_num}"),
            "-screen",
            "0",
            "1x1x24",
            "-listen",
            "tcp",
            "-auth",
            &xauth_file.to_string_lossy(),
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .context("failed to start Xvfb")?;

    tracing::info!("started Xvfb :{display_num} (pid {})", xvfb.id());

    // Save PID for clean shutdown later
    let state_dir = dirs::home_dir()
        .context("cannot determine home directory")?
        .join(".local/share/cliptunnel");
    fs::create_dir_all(&state_dir)?;
    fs::write(state_dir.join("xvfb.pid"), xvfb.id().to_string())?;

    // Give Xvfb a moment to start
    std::thread::sleep(std::time::Duration::from_millis(500));

    // Write X11 clipboard owner script
    let script = generate_x11_owner_script(port, display_num, &real_xclip);
    let script_path = owner_script_path()?;
    fs::write(&script_path, &script)
        .with_context(|| format!("failed to write {}", script_path.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        fs::set_permissions(&script_path, fs::Permissions::from_mode(0o755))?;
    }

    // Start the owner script in background and save PID
    let owner = Command::new(&script_path)
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .with_context(|| format!("failed to start {}", script_path.display()))?;
    fs::write(state_dir.join("x11-owner.pid"), owner.id().to_string())?;

    // DISPLAY=:N works for most CLIs, TCP format for sandboxed ones (Codex)
    let display_unix = format!(":{display_num}");
    let display_tcp = format!("127.0.0.1:{display_num}");

    // Add DISPLAY and XAUTHORITY exports to shell profiles
    let export_line = format!(
        "export DISPLAY={display_unix}\nexport XAUTHORITY={}",
        xauth_file.display()
    );
    let marker = "# cliptunnel X11 bridge";

    for rc_file in [".bashrc", ".zshrc"] {
        let rc_path = dirs::home_dir()
            .context("cannot determine home directory")?
            .join(rc_file);

        // Guard: skip symlinks to avoid overwriting unexpected targets
        if rc_path.exists() {
            let meta = fs::symlink_metadata(&rc_path)?;
            if meta.file_type().is_symlink() {
                tracing::warn!("skipping {} — it is a symlink", rc_path.display());
                continue;
            }
        }

        let content = if rc_path.exists() {
            fs::read_to_string(&rc_path).unwrap_or_default()
        } else {
            String::new()
        };

        // Remove old cliptunnel DISPLAY lines, then add fresh one
        let cleaned: Vec<&str> = content
            .lines()
            .filter(|l| !l.contains(marker) && !l.contains("cliptunnel X11"))
            .filter(|l| !(l.contains("export DISPLAY=") && l.contains("cliptunnel")))
            .collect();

        let mut new_content = cleaned.join("\n");
        if !new_content.ends_with('\n') {
            new_content.push('\n');
        }
        new_content.push_str(&format!("{marker}\n{export_line}\n"));

        fs::write(&rc_path, &new_content)?;
        tracing::info!("updated DISPLAY export in {}", rc_path.display());
    }

    println!("X11 bridge started on DISPLAY={display_unix}");
    println!("  TCP also available: DISPLAY={display_tcp}");
    println!("Restart your shell or run: export DISPLAY={display_unix}");
    Ok(())
}
