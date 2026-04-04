use anyhow::{bail, Context, Result};
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::config;

const REMOTE_BIN_DIR: &str = ".local/bin";
const REMOTE_CONFIG_DIR: &str = ".config/cliptunnel";
const REMOTE_BINARY_NAME: &str = "cliptunnel";
const GITHUB_REPO: &str = "abhishekbiyala/cliptunnel";

/// Deploy the cliptunnel binary and token to the remote host, then run install-remote.
pub async fn deploy_to_remote(
    host: &str,
    binary: Option<&Path>,
    arch: &str,
    x11: bool,
) -> Result<()> {
    let binary_path = resolve_binary(binary, arch).await?;
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
/// Otherwise, look for a pre-built binary in standard locations,
/// and fall back to downloading from GitHub releases.
async fn resolve_binary(binary: Option<&Path>, arch: &str) -> Result<PathBuf> {
    if let Some(path) = binary {
        if !path.exists() {
            bail!("specified binary does not exist: {}", path.display());
        }
        return Ok(path.to_path_buf());
    }

    let target = match arch {
        "x86_64" | "x86-64" => "x86_64-unknown-linux-gnu",
        "aarch64" | "arm64" => "aarch64-unknown-linux-gnu",
        other => bail!("unsupported architecture: {}", other),
    };

    // Check target/release for cross-compiled binary (dev workflow)
    let candidates = [
        format!("target/{}/release/cliptunnel", target),
        "target/release/cliptunnel".to_string(),
    ];

    for candidate in &candidates {
        let p = Path::new(candidate);
        if p.exists() {
            tracing::info!("found binary at {}", p.display());
            return Ok(p.to_path_buf());
        }
    }

    // Check cache for a previously downloaded binary
    let arch_label = match arch {
        "x86_64" | "x86-64" => "x86_64",
        "aarch64" | "arm64" => "aarch64",
        _ => arch,
    };
    let asset_name = format!("cliptunnel-linux-{}", arch_label);
    let cache_dir = config::data_dir().join("bin");
    let cached_path = cache_dir.join(&asset_name);

    if cached_path.exists() {
        tracing::info!("found cached binary at {}", cached_path.display());
        return Ok(cached_path);
    }

    // Download from GitHub releases
    tracing::info!(
        "no local binary found for arch '{}', downloading from GitHub releases...",
        arch
    );
    download_linux_binary(&asset_name, &cache_dir).await
}

/// Download a Linux binary from GitHub releases for the current version.
async fn download_linux_binary(asset_name: &str, cache_dir: &Path) -> Result<PathBuf> {
    let version = env!("CARGO_PKG_VERSION");
    let url = format!(
        "https://github.com/{}/releases/download/v{}/{}",
        GITHUB_REPO, version, asset_name
    );

    std::fs::create_dir_all(cache_dir)
        .with_context(|| format!("failed to create cache dir {}", cache_dir.display()))?;

    let dest = cache_dir.join(asset_name);
    let tmp = cache_dir.join(format!(".{}.tmp", asset_name));

    tracing::info!("downloading {} ...", url);
    let output = Command::new("curl")
        .args(["-fSL", "-o"])
        .arg(&tmp)
        .arg(&url)
        .output()
        .await
        .context("failed to run curl")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let _ = std::fs::remove_file(&tmp);
        bail!(
            "failed to download Linux binary from {}:\n    {}\n\n    \
             Make sure release v{} exists with asset '{}'.\n    \
             Or pass --binary <path> to use a local binary.",
            url,
            stderr.trim(),
            version,
            asset_name
        );
    }

    // Make executable and move into place atomically
    let perms = std::fs::Permissions::from_mode(0o755);
    std::fs::set_permissions(&tmp, perms).context("failed to chmod downloaded binary")?;
    std::fs::rename(&tmp, &dest).context("failed to move downloaded binary into cache")?;

    tracing::info!("cached Linux binary at {}", dest.display());
    Ok(dest)
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
