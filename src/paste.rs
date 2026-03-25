use anyhow::{Context, Result};
use rand::Rng;
use std::path::{Path, PathBuf};
use tokio::process::Command;

use crate::config;

fn generate_temp_path() -> PathBuf {
    let mut bytes = [0u8; 4];
    rand::rng().fill_bytes(&mut bytes);
    let suffix = hex::encode(bytes);
    PathBuf::from(format!("/tmp/cliptunnel-{suffix}.png"))
}

pub async fn run(path: Option<&Path>, tmux: bool, url: &str) -> Result<()> {
    let token = config::load_token()
        .context("failed to load token - run 'cliptunnel daemon' first to generate one")?;

    let out_path = path
        .map(|p| p.to_path_buf())
        .unwrap_or_else(generate_temp_path);

    let auth_config = config::write_auth_config(&token)?;

    let output = Command::new("curl")
        .arg("-s")
        .arg("-f")
        .arg("-o")
        .arg(out_path.to_str().unwrap_or("/tmp/cliptunnel-paste.png"))
        .arg("-K")
        .arg(auth_config.path())
        .arg(format!("{url}/clipboard"))
        .output()
        .await
        .context("failed to run curl")?;

    drop(auth_config); // auto-deletes the temp file

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        anyhow::bail!(
            "failed to fetch clipboard image (HTTP error): {}",
            stderr.trim()
        );
    }

    if !out_path.exists() {
        anyhow::bail!(
            "clipboard image file was not created at {}",
            out_path.display()
        );
    }

    tracing::info!("saved clipboard image to {}", out_path.display());

    if tmux {
        let pane = std::env::var("TMUX_PANE").unwrap_or_default();
        if pane.is_empty() {
            tracing::warn!("--tmux flag set but TMUX_PANE is not set");
        } else {
            let path_str = out_path.display().to_string();
            let status = Command::new("tmux")
                .arg("send-keys")
                .arg("-t")
                .arg(&pane)
                .arg(&path_str)
                .status()
                .await
                .context("failed to run tmux send-keys")?;
            if !status.success() {
                tracing::warn!("tmux send-keys exited with non-zero status");
            }
        }
    }

    println!("{}", out_path.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Read;

    #[test]
    fn generate_temp_path_has_cliptunnel_prefix() {
        let path = generate_temp_path();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert!(filename.starts_with("cliptunnel-"));
    }

    #[test]
    fn generate_temp_path_has_png_extension() {
        let path = generate_temp_path();
        assert_eq!(path.extension().and_then(|e| e.to_str()), Some("png"));
    }

    #[test]
    fn generate_temp_path_lives_in_tmp() {
        let path = generate_temp_path();
        assert_eq!(path.parent().unwrap().to_str().unwrap(), "/tmp");
    }

    #[test]
    fn generate_temp_path_is_unique() {
        let p1 = generate_temp_path();
        let p2 = generate_temp_path();
        assert_ne!(p1, p2);
    }

    #[test]
    fn generate_temp_path_hex_suffix_is_8_chars() {
        let path = generate_temp_path();
        let filename = path.file_name().unwrap().to_str().unwrap();
        let hex_part = filename
            .strip_prefix("cliptunnel-")
            .unwrap()
            .strip_suffix(".png")
            .unwrap();
        assert_eq!(hex_part.len(), 8);
        assert!(hex_part.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn write_auth_config_contains_bearer_header() {
        let tmp = config::write_auth_config("my-secret-token").unwrap();
        let mut content = String::new();
        std::fs::File::open(tmp.path())
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.contains("Authorization: Bearer my-secret-token"));
    }

    #[test]
    fn write_auth_config_is_curl_config_format() {
        let tmp = config::write_auth_config("tok123").unwrap();
        let mut content = String::new();
        std::fs::File::open(tmp.path())
            .unwrap()
            .read_to_string(&mut content)
            .unwrap();
        assert!(content.starts_with("header = "));
    }

    #[test]
    fn write_auth_config_file_deleted_on_drop() {
        let path = {
            let tmp = config::write_auth_config("ephemeral").unwrap();
            let p = tmp.path().to_path_buf();
            assert!(p.exists());
            p
        };
        assert!(!path.exists());
    }
}
