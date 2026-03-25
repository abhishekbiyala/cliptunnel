use anyhow::{Context, Result};
use std::fs;
use std::os::unix::fs::PermissionsExt;

const SHIM_NAMES: &[(&str, &str)] = &[
    ("_common.sh", super::shims::COMMON_SHIM),
    ("xclip", super::shims::XCLIP_SHIM),
    ("xsel", super::shims::XSEL_SHIM),
    ("wl-paste", super::shims::WL_PASTE_SHIM),
];

pub async fn run(x11: bool) -> Result<()> {
    let bin_dir = dirs::home_dir()
        .context("could not determine home directory")?
        .join(".local/bin");

    // 1. Create ~/.local/bin/ if missing
    fs::create_dir_all(&bin_dir)
        .with_context(|| format!("failed to create {}", bin_dir.display()))?;

    // 2-4. Write shims and chmod +x
    for (name, content) in SHIM_NAMES {
        let shim_path = bin_dir.join(name);
        fs::write(&shim_path, content)
            .with_context(|| format!("failed to write shim {}", shim_path.display()))?;

        let mut perms = fs::metadata(&shim_path)
            .with_context(|| format!("failed to read metadata for {}", shim_path.display()))?
            .permissions();
        perms.set_mode(0o755);
        fs::set_permissions(&shim_path, perms)
            .with_context(|| format!("failed to chmod +x {}", shim_path.display()))?;

        tracing::info!("installed shim: {}", shim_path.display());
    }

    // 5. Ensure ~/.local/bin is in PATH
    ensure_path_contains(&bin_dir)?;

    // 6. If x11, set up the X11 bridge
    if x11 {
        super::x11_bridge::setup(crate::config::DEFAULT_PORT)
            .await
            .context("failed to set up x11 bridge")?;
    }

    // 7. Success message
    println!("cliptunnel: shims installed to {}", bin_dir.display());
    println!("cliptunnel: xclip, xsel, wl-paste shims are ready");
    if x11 {
        println!("cliptunnel: x11 bridge configured");
    }

    Ok(())
}

/// Ensure the given directory is in PATH. If not, append an export line
/// to .bashrc and .zshrc in the user's home directory.
fn ensure_path_contains(bin_dir: &std::path::Path) -> Result<()> {
    let bin_str = bin_dir.to_string_lossy();

    // Check if already in PATH
    if let Ok(path) = std::env::var("PATH") {
        for entry in path.split(':') {
            if entry == bin_str.as_ref() {
                tracing::info!("{} is already in PATH", bin_str);
                return Ok(());
            }
        }
    }

    let home = dirs::home_dir().context("could not determine home directory")?;
    let export_line = format!(
        "\n# Added by cliptunnel\nexport PATH=\"{}:$PATH\"\n",
        bin_str
    );

    for rc_name in &[".bashrc", ".zshrc"] {
        let rc_path = home.join(rc_name);

        // Guard: skip symlinks to avoid overwriting unexpected targets
        if rc_path.exists() {
            let meta = fs::symlink_metadata(&rc_path)?;
            if meta.file_type().is_symlink() {
                tracing::warn!(
                    "skipping {} — it is a symlink, refusing to modify",
                    rc_path.display()
                );
                continue;
            }
        }

        // Read existing content to avoid duplicating the line
        if rc_path.exists() {
            let existing = fs::read_to_string(&rc_path).unwrap_or_default();
            if existing.contains(&format!("export PATH=\"{}:$PATH\"", bin_str)) {
                tracing::info!("{} already has PATH entry", rc_path.display());
                continue;
            }
        }

        // Append the export line (create the file if it doesn't exist)
        let mut content = if rc_path.exists() {
            fs::read_to_string(&rc_path).unwrap_or_default()
        } else {
            String::new()
        };
        content.push_str(&export_line);
        fs::write(&rc_path, content)
            .with_context(|| format!("failed to update {}", rc_path.display()))?;

        tracing::info!("added PATH entry to {}", rc_path.display());
        println!(
            "cliptunnel: added {} to PATH in {}",
            bin_str,
            rc_path.display()
        );
    }

    Ok(())
}
