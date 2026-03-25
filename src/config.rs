use anyhow::{Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use rand::RngCore;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const TOKEN_BYTES: usize = 32;

/// Default daemon port. Single source of truth — referenced by CLI defaults,
/// connect, doctor, install-remote, and SSH config operations.
pub const DEFAULT_PORT: u16 = 18442;

pub fn config_dir() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("~/.config"))
        .join("cliptunnel")
}

pub fn data_dir() -> PathBuf {
    dirs::data_local_dir()
        .unwrap_or_else(|| PathBuf::from("~/.local/share"))
        .join("cliptunnel")
}

pub fn token_path() -> PathBuf {
    config_dir().join("token")
}

/// Generate a new random token string (32 bytes, base64url-encoded, no padding).
fn generate_token() -> String {
    let mut bytes = vec![0u8; TOKEN_BYTES];
    rand::rng().fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(&bytes)
}

/// Create or load a token at the given path.
fn load_or_create_token_at(path: &Path) -> Result<String> {
    if path.exists() {
        // Enforce 0600 permissions — reject or fix world/group-readable tokens
        let meta = fs::metadata(path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode & 0o077 != 0 {
            tracing::warn!(
                "token file {} has insecure permissions {:o}, fixing to 0600",
                path.display(),
                mode
            );
            fs::set_permissions(path, fs::Permissions::from_mode(0o600))?;
        }

        let token = fs::read_to_string(path)
            .with_context(|| format!("failed to read token from {}", path.display()))?;
        let token = token.trim().to_string();
        if !token.is_empty() {
            return Ok(token);
        }
    }

    let dir = path.parent().unwrap();
    fs::create_dir_all(dir)
        .with_context(|| format!("failed to create config dir {}", dir.display()))?;

    let token = generate_token();

    fs::write(path, &token)
        .with_context(|| format!("failed to write token to {}", path.display()))?;

    // chmod 600
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(path, perms)?;

    tracing::info!("generated new bearer token at {}", path.display());
    Ok(token)
}

/// Load a token from the given path, failing if it doesn't exist or is empty.
fn load_token_at(path: &Path) -> Result<String> {
    let token = fs::read_to_string(path)
        .with_context(|| format!("token not found at {}", path.display()))?;
    let token = token.trim().to_string();
    if token.is_empty() {
        anyhow::bail!("token file is empty: {}", path.display());
    }
    Ok(token)
}

pub fn load_or_create_token() -> Result<String> {
    load_or_create_token_at(&token_path())
}

pub fn load_token() -> Result<String> {
    load_token_at(&token_path())
}

/// Create a temporary curl config file with the bearer auth header.
/// Returns a NamedTempFile — caller must keep it alive until curl finishes.
/// Token is never exposed in process arguments.
pub fn write_auth_config(token: &str) -> Result<tempfile::NamedTempFile> {
    use std::io::Write;
    let mut tmp = tempfile::Builder::new()
        .prefix("cliptunnel-curl-")
        .suffix(".cfg")
        .tempfile()
        .context("failed to create auth config tempfile")?;
    writeln!(tmp, "header = \"Authorization: Bearer {token}\"")?;
    tmp.as_file().sync_all()?;
    Ok(tmp)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::PermissionsExt;
    use tempfile::TempDir;

    #[test]
    fn creates_new_token_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");

        assert!(!path.exists());
        let token = load_or_create_token_at(&path).unwrap();
        assert!(path.exists());
        assert!(!token.is_empty());
    }

    #[test]
    fn returns_existing_token() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");

        let first = load_or_create_token_at(&path).unwrap();
        let second = load_or_create_token_at(&path).unwrap();
        assert_eq!(first, second, "loading again should return the same token");
    }

    #[test]
    fn load_token_fails_when_missing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nonexistent-token");

        let result = load_token_at(&path);
        assert!(
            result.is_err(),
            "load_token should fail when file is missing"
        );
    }

    #[test]
    fn token_is_43_chars() {
        // 32 bytes base64url-encoded without padding = ceil(32*4/3) = 43 characters
        let token = generate_token();
        assert_eq!(
            token.len(),
            43,
            "token should be 43 chars (32 bytes base64url no-pad), got {}",
            token.len()
        );
    }

    #[test]
    fn token_is_url_safe_base64() {
        let token = generate_token();
        // URL-safe base64 only contains [A-Za-z0-9_-]
        assert!(
            token
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '_' || c == '-'),
            "token should only contain URL-safe base64 chars, got: {}",
            token
        );
    }

    #[test]
    fn generate_token_uniqueness() {
        // Two generated tokens should (almost certainly) be different
        let t1 = generate_token();
        let t2 = generate_token();
        assert_ne!(t1, t2, "two random tokens should not be identical");
    }

    #[test]
    fn load_token_fails_on_empty_file() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "").unwrap();

        let result = load_token_at(&path);
        assert!(result.is_err(), "loading empty token file should fail");
    }

    #[test]
    fn load_token_trims_whitespace() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "  my-token-value  \n").unwrap();

        let token = load_token_at(&path).unwrap();
        assert_eq!(token, "my-token-value");
    }

    #[test]
    fn load_or_create_trims_whitespace_from_existing() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "  existing-token  \n").unwrap();
        // Set permissions to 0600 to avoid the warn+fix path
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let token = load_or_create_token_at(&path).unwrap();
        assert_eq!(token, "existing-token");
    }

    #[test]
    fn load_or_create_regenerates_when_file_is_empty() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "   \n").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o600)).unwrap();

        let token = load_or_create_token_at(&path).unwrap();
        // Should have generated a new 43-char token
        assert_eq!(token.len(), 43);
    }

    #[test]
    fn load_or_create_fixes_insecure_permissions() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");
        fs::write(&path, "some-token").unwrap();
        fs::set_permissions(&path, fs::Permissions::from_mode(0o644)).unwrap();

        let _token = load_or_create_token_at(&path).unwrap();
        let mode = fs::metadata(&path).unwrap().permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "permissions should be fixed to 0600");
    }

    #[test]
    fn load_or_create_creates_parent_dirs() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("nested").join("deep").join("token");

        let token = load_or_create_token_at(&path).unwrap();
        assert!(!token.is_empty());
        assert!(path.exists());
    }

    #[test]
    fn config_dir_ends_with_cliptunnel() {
        let dir = config_dir();
        assert!(
            dir.ends_with("cliptunnel"),
            "config_dir should end with 'cliptunnel', got {}",
            dir.display()
        );
    }

    #[test]
    fn data_dir_ends_with_cliptunnel() {
        let dir = data_dir();
        assert!(
            dir.ends_with("cliptunnel"),
            "data_dir should end with 'cliptunnel', got {}",
            dir.display()
        );
    }

    #[test]
    fn token_path_ends_with_token() {
        let path = token_path();
        assert_eq!(path.file_name().unwrap().to_str().unwrap(), "token");
    }

    #[test]
    fn token_file_has_0600_permissions() {
        let dir = TempDir::new().unwrap();
        let path = dir.path().join("token");

        load_or_create_token_at(&path).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(
            mode, 0o600,
            "token file should have 0600 permissions, got {:o}",
            mode
        );
    }
}
