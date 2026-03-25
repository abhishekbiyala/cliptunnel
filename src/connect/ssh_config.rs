use anyhow::{Context, Result};
use std::fs;
use std::path::PathBuf;

fn ssh_config_path() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".ssh")
        .join("config")
}

fn backup_path() -> PathBuf {
    dirs::home_dir()
        .expect("cannot determine home directory")
        .join(".ssh")
        .join("config.cliptunnel.bak")
}

/// Validate that an SSH host name is safe for inclusion in SSH config, plists,
/// and as an argument to ssh/scp commands.
/// Uses a positive allowlist: `[a-zA-Z0-9._@:-]` only.
/// Rejects `-` prefix to prevent SSH option injection.
pub fn validate_host(host: &str) -> Result<()> {
    if host.is_empty() {
        anyhow::bail!("host name cannot be empty");
    }
    if host.len() > 253 {
        anyhow::bail!("host name too long (max 253 chars)");
    }
    if host.starts_with('-') {
        anyhow::bail!("host name cannot start with '-' (would be interpreted as SSH option)");
    }
    for ch in host.chars() {
        if !matches!(ch, 'a'..='z' | 'A'..='Z' | '0'..='9' | '.' | '_' | '@' | ':' | '-') {
            anyhow::bail!("host name contains invalid character: {:?}", ch);
        }
    }
    Ok(())
}

/// Pure string-manipulation logic for adding/updating a RemoteForward line in SSH config content.
fn add_forward_to_content(content: &str, host: &str, port: u16) -> String {
    let forward_line = format!("  RemoteForward {} 127.0.0.1:{}", port, port);
    let lines: Vec<&str> = content.lines().collect();
    let mut result: Vec<String> = Vec::new();

    // Try to find the exact Host block and update it
    let mut found_host = false;
    let mut in_target_block = false;
    let mut forward_replaced = false;
    let mut i = 0;

    while i < lines.len() {
        let line = lines[i];
        let trimmed = line.trim();

        if trimmed.starts_with("Host ") || trimmed.starts_with("Host\t") {
            // Check if this is our exact host (not a wildcard/multi-host)
            let host_value = trimmed.strip_prefix("Host").unwrap().trim();
            if host_value == host {
                in_target_block = true;
                found_host = true;
                result.push(line.to_string());
                i += 1;
                continue;
            } else {
                // Leaving target block, entering a different Host block
                if in_target_block && !forward_replaced {
                    // Insert RemoteForward before moving on
                    result.push(forward_line.clone());
                    forward_replaced = true;
                }
                in_target_block = false;
                result.push(line.to_string());
                i += 1;
                continue;
            }
        }

        // Match block starts also end the current Host block
        if trimmed.starts_with("Match ") || trimmed.starts_with("Match\t") {
            if in_target_block && !forward_replaced {
                result.push(forward_line.clone());
                forward_replaced = true;
            }
            in_target_block = false;
            result.push(line.to_string());
            i += 1;
            continue;
        }

        if in_target_block {
            // Check if this line is a RemoteForward for our port
            let t = trimmed.to_lowercase();
            if t.starts_with("remoteforward") {
                let rest = trimmed["RemoteForward".len()..].trim();
                // Check if this forward is for our port
                if rest.starts_with(&port.to_string()) {
                    // Replace this line
                    result.push(forward_line.clone());
                    forward_replaced = true;
                    i += 1;
                    continue;
                }
            }
        }

        result.push(line.to_string());
        i += 1;
    }

    // If we were in the target block at EOF and didn't add the forward yet
    if in_target_block && !forward_replaced {
        result.push(forward_line.clone());
        forward_replaced = true;
    }

    // If the host block didn't exist at all, append it
    if !found_host {
        if !result.is_empty() && !result.last().is_none_or(|l| l.is_empty()) {
            result.push(String::new());
        }
        result.push(format!("Host {}", host));
        result.push(forward_line);
        let _ = forward_replaced; // suppress unused warning
    }

    // Ensure trailing newline
    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

/// Read SSH config, back it up, then add/update a RemoteForward line for the given host.
pub fn add_remote_forward(host: &str, port: u16) -> Result<()> {
    validate_host(host)?;
    let config_path = ssh_config_path();
    let backup = backup_path();

    // Read existing config or start empty
    let content = if config_path.exists() {
        fs::read_to_string(&config_path)
            .with_context(|| format!("failed to read {}", config_path.display()))?
    } else {
        // Ensure .ssh directory exists
        if let Some(parent) = config_path.parent() {
            fs::create_dir_all(parent)?;
        }
        String::new()
    };

    // Back up
    fs::write(&backup, &content)
        .with_context(|| format!("failed to write backup to {}", backup.display()))?;
    tracing::debug!("backed up ssh config to {}", backup.display());

    let output = add_forward_to_content(&content, host, port);

    fs::write(&config_path, &output)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    tracing::info!(
        "added RemoteForward {} to Host {} in {}",
        port,
        host,
        config_path.display()
    );
    Ok(())
}

/// Pure string-manipulation logic for removing the cliptunnel RemoteForward line from SSH config content.
fn remove_forward_from_content(content: &str, host: &str) -> String {
    let lines: Vec<&str> = content.lines().collect();
    let mut result: Vec<String> = Vec::new();
    let mut in_target_block = false;

    for line in &lines {
        let trimmed = line.trim();

        if trimmed.starts_with("Host ") || trimmed.starts_with("Host\t") {
            let host_value = trimmed.strip_prefix("Host").unwrap().trim();
            in_target_block = host_value == host;
            result.push(line.to_string());
            continue;
        }

        if trimmed.starts_with("Match ") || trimmed.starts_with("Match\t") {
            in_target_block = false;
            result.push(line.to_string());
            continue;
        }

        if in_target_block {
            let t = trimmed.to_lowercase();
            if t.starts_with("remoteforward") {
                // Check if it's a cliptunnel forward (port 18442)
                let rest = trimmed["RemoteForward".len()..].trim();
                if rest.starts_with("18442") {
                    continue; // skip this line
                }
            }
        }

        result.push(line.to_string());
    }

    let mut output = result.join("\n");
    if !output.ends_with('\n') {
        output.push('\n');
    }

    output
}

/// Remove the cliptunnel RemoteForward line from the given host block.
pub fn remove_remote_forward(host: &str) -> Result<()> {
    validate_host(host)?;
    let config_path = ssh_config_path();

    if !config_path.exists() {
        tracing::debug!("no ssh config found, nothing to remove");
        return Ok(());
    }

    let content = fs::read_to_string(&config_path)
        .with_context(|| format!("failed to read {}", config_path.display()))?;

    let output = remove_forward_from_content(&content, host);

    fs::write(&config_path, &output)
        .with_context(|| format!("failed to write {}", config_path.display()))?;

    tracing::info!("removed RemoteForward line from Host {}", host);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── add_forward_to_content ───────────────────────────────────────────

    #[test]
    fn add_forward_empty_config_creates_host_block() {
        let result = add_forward_to_content("", "myserver", 18442);
        assert_eq!(
            result,
            "Host myserver\n  RemoteForward 18442 127.0.0.1:18442\n"
        );
    }

    #[test]
    fn add_forward_existing_host_adds_line() {
        let config = "Host myserver\n  User alice\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        assert!(result.contains("Host myserver"));
        assert!(result.contains("  User alice"));
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn add_forward_updates_existing_remote_forward_same_port() {
        let config = "Host myserver\n  User alice\n  RemoteForward 18442 127.0.0.1:9999\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        // The old forward should be replaced, not duplicated
        assert_eq!(
            result.matches("RemoteForward").count(),
            1,
            "should have exactly one RemoteForward line"
        );
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
        assert!(!result.contains("127.0.0.1:9999"));
    }

    #[test]
    fn add_forward_preserves_other_hosts_comments_and_formatting() {
        let config = "# Global settings\nServerAliveInterval 60\n\nHost otherserver\n  User bob\n  Port 2222\n\nHost *\n  AddKeysToAgent yes\n";
        let result = add_forward_to_content(config, "myserver", 18442);

        // Original content should be preserved
        assert!(result.contains("# Global settings"));
        assert!(result.contains("ServerAliveInterval 60"));
        assert!(result.contains("Host otherserver"));
        assert!(result.contains("  User bob"));
        assert!(result.contains("  Port 2222"));
        assert!(result.contains("Host *"));
        assert!(result.contains("  AddKeysToAgent yes"));

        // New host block should be appended
        assert!(result.contains("Host myserver"));
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn add_forward_handles_match_blocks() {
        let config = "Host myserver\n  User alice\n\nMatch host example.com\n  ForwardAgent yes\n";
        let result = add_forward_to_content(config, "myserver", 18442);

        // RemoteForward should be added inside the myserver block, before the Match block
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
        assert!(result.contains("Match host example.com"));
        assert!(result.contains("  ForwardAgent yes"));

        // Verify ordering: RemoteForward appears before Match
        let fwd_pos = result.find("RemoteForward 18442").unwrap();
        let match_pos = result.find("Match host").unwrap();
        assert!(
            fwd_pos < match_pos,
            "RemoteForward should appear before Match block"
        );
    }

    #[test]
    fn add_forward_does_not_modify_wildcard_hosts() {
        let config = "Host *\n  AddKeysToAgent yes\n  IdentityFile ~/.ssh/id_ed25519\n";
        let result = add_forward_to_content(config, "myserver", 18442);

        // Wildcard host should be untouched
        assert!(result.contains("Host *\n  AddKeysToAgent yes"));

        // New host block should be appended separately
        assert!(result.contains("Host myserver"));
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn add_forward_host_block_between_other_blocks() {
        let config =
            "Host first\n  User one\n\nHost myserver\n  User alice\n\nHost last\n  User three\n";
        let result = add_forward_to_content(config, "myserver", 18442);

        assert!(result.contains("Host first"));
        assert!(result.contains("  User one"));
        assert!(result.contains("Host myserver"));
        assert!(result.contains("  User alice"));
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
        assert!(result.contains("Host last"));
        assert!(result.contains("  User three"));

        // Only one RemoteForward line
        assert_eq!(result.matches("RemoteForward").count(), 1);
    }

    #[test]
    fn add_forward_preserves_other_remote_forwards_in_same_block() {
        let config = "Host myserver\n  User alice\n  RemoteForward 9999 127.0.0.1:9999\n";
        let result = add_forward_to_content(config, "myserver", 18442);

        // Both forwards should be present
        assert!(result.contains("RemoteForward 9999 127.0.0.1:9999"));
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
        assert_eq!(result.matches("RemoteForward").count(), 2);
    }

    // ── remove_forward_from_content ──────────────────────────────────────

    #[test]
    fn remove_forward_removes_line_from_correct_host() {
        let config = "Host myserver\n  User alice\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");

        assert!(result.contains("Host myserver"));
        assert!(result.contains("  User alice"));
        assert!(
            !result.contains("RemoteForward"),
            "RemoteForward line should be removed"
        );
    }

    #[test]
    fn remove_forward_leaves_other_remote_forwards_untouched() {
        let config = "Host myserver\n  User alice\n  RemoteForward 9999 127.0.0.1:9999\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");

        // The non-cliptunnel forward should remain
        assert!(result.contains("RemoteForward 9999 127.0.0.1:9999"));
        // The cliptunnel forward (18442) should be gone
        assert!(!result.contains("RemoteForward 18442"));
    }

    #[test]
    fn remove_forward_noop_when_host_missing() {
        let config = "Host otherserver\n  User bob\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");

        // Nothing should change; the forward belongs to otherserver, not myserver
        assert!(result.contains("Host otherserver"));
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn remove_forward_noop_on_empty_content() {
        let result = remove_forward_from_content("", "myserver");
        assert_eq!(result, "\n");
    }

    #[test]
    fn remove_forward_only_affects_target_host() {
        let config = "Host server1\n  RemoteForward 18442 127.0.0.1:18442\n\nHost server2\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "server1");

        // server1's forward should be removed
        let server1_pos = result.find("Host server1").unwrap();
        let server2_pos = result.find("Host server2").unwrap();
        let between = &result[server1_pos..server2_pos];
        assert!(
            !between.contains("RemoteForward"),
            "server1 block should have no RemoteForward"
        );

        // server2's forward should remain
        let after_server2 = &result[server2_pos..];
        assert!(
            after_server2.contains("RemoteForward 18442"),
            "server2 block should still have RemoteForward"
        );
    }

    #[test]
    fn remove_forward_handles_match_block_boundary() {
        let config = "Host myserver\n  User alice\n  RemoteForward 18442 127.0.0.1:18442\n\nMatch host example.com\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");

        // myserver's forward should be removed
        assert!(result.contains("Host myserver"));
        let myserver_pos = result.find("Host myserver").unwrap();
        let match_pos = result.find("Match host").unwrap();
        let between = &result[myserver_pos..match_pos];
        assert!(!between.contains("RemoteForward"));

        // Match block's forward should remain (not in target host)
        let after_match = &result[match_pos..];
        assert!(after_match.contains("RemoteForward 18442"));
    }

    // ── validate_host (adversarial inputs) ──────────────────────────────

    #[test]
    fn validate_rejects_empty_host() {
        assert!(validate_host("").is_err());
    }

    #[test]
    fn validate_rejects_newline_injection() {
        assert!(validate_host("myserver\n  ProxyCommand evil").is_err());
    }

    #[test]
    fn validate_rejects_carriage_return() {
        assert!(validate_host("myserver\r\nHost evil").is_err());
    }

    #[test]
    fn validate_rejects_spaces() {
        assert!(validate_host("my server").is_err());
    }

    #[test]
    fn validate_rejects_tabs() {
        assert!(validate_host("my\tserver").is_err());
    }

    #[test]
    fn validate_rejects_null_byte() {
        assert!(validate_host("myserver\0").is_err());
    }

    #[test]
    fn validate_rejects_hash_comment() {
        assert!(validate_host("myserver # comment").is_err());
    }

    #[test]
    fn validate_rejects_quotes() {
        assert!(validate_host("my\"server").is_err());
        assert!(validate_host("my'server").is_err());
    }

    #[test]
    fn validate_rejects_xml_metacharacters_with_spaces() {
        // XML chars alone are fine in SSH host names, but with spaces they'd be caught
        assert!(validate_host("<script>alert(1)</script> evil").is_err());
    }

    #[test]
    fn validate_accepts_normal_hosts() {
        assert!(validate_host("myserver").is_ok());
        assert!(validate_host("user@myserver").is_ok());
        assert!(validate_host("192.168.1.1").is_ok());
        assert!(validate_host("my-devbox.example.com").is_ok());
    }

    #[test]
    fn validate_rejects_oversized_host() {
        let long = "a".repeat(254);
        assert!(validate_host(&long).is_err());
    }

    #[test]
    fn validate_accepts_max_length_host() {
        let host = "a".repeat(253);
        assert!(validate_host(&host).is_ok());
    }

    // ── add_forward_to_content edge cases ───────────────────────────────

    #[test]
    fn add_forward_to_content_with_tab_after_host() {
        let config = "Host\tmyserver\n  User alice\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
        assert_eq!(result.matches("RemoteForward").count(), 1);
    }

    #[test]
    fn add_forward_with_varying_indentation() {
        let config = "Host myserver\n    User alice\n\tHostName 10.0.0.1\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        assert!(result.contains("    User alice"));
        assert!(result.contains("\tHostName 10.0.0.1"));
        assert!(result.contains("  RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn add_forward_case_insensitive_remoteforward() {
        let config = "Host myserver\n  remoteforward 18442 127.0.0.1:9999\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        // Should replace the existing line regardless of case
        assert_eq!(result.matches("RemoteForward").count(), 1);
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
    }

    #[test]
    fn add_forward_empty_lines_between_blocks() {
        let config = "\n\nHost myserver\n  User alice\n\n\nHost other\n  User bob\n";
        let result = add_forward_to_content(config, "myserver", 18442);
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
        assert_eq!(result.matches("RemoteForward").count(), 1);
    }

    #[test]
    fn add_forward_host_at_end_of_file_no_trailing_newline() {
        let config = "Host myserver\n  User alice";
        let result = add_forward_to_content(config, "myserver", 18442);
        assert!(result.contains("RemoteForward 18442 127.0.0.1:18442"));
        assert!(result.ends_with('\n'), "output should end with newline");
    }

    #[test]
    fn add_forward_different_port() {
        let result = add_forward_to_content("", "server", 9999);
        assert!(result.contains("RemoteForward 9999 127.0.0.1:9999"));
    }

    #[test]
    fn add_forward_multiple_host_blocks_only_target_modified() {
        let config = "Host alpha\n  User a\n\nHost beta\n  User b\n\nHost gamma\n  User c\n";
        let result = add_forward_to_content(config, "beta", 18442);

        // Only beta should get RemoteForward
        assert_eq!(result.matches("RemoteForward").count(), 1);

        let beta_pos = result.find("Host beta").unwrap();
        let gamma_pos = result.find("Host gamma").unwrap();
        let fwd_pos = result.find("RemoteForward").unwrap();
        assert!(
            fwd_pos > beta_pos && fwd_pos < gamma_pos,
            "RemoteForward should be inside the beta block"
        );
    }

    // ── remove_forward_from_content edge cases ──────────────────────────

    #[test]
    fn remove_forward_case_insensitive_keyword() {
        let config = "Host myserver\n  remoteforward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(
            !result.to_lowercase().contains("remoteforward"),
            "case-insensitive RemoteForward should be removed"
        );
    }

    #[test]
    fn remove_forward_preserves_non_cliptunnel_port() {
        let config = "Host myserver\n  RemoteForward 9999 127.0.0.1:9999\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(
            result.contains("RemoteForward 9999"),
            "non-cliptunnel RemoteForward should be preserved"
        );
    }

    #[test]
    fn remove_forward_with_tab_delimited_host() {
        let config = "Host\tmyserver\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(!result.contains("RemoteForward 18442"));
    }

    #[test]
    fn remove_forward_trailing_newline_preserved() {
        let config = "Host myserver\n  User alice\n  RemoteForward 18442 127.0.0.1:18442\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(result.ends_with('\n'));
    }

    #[test]
    fn remove_forward_no_forward_present_is_noop() {
        let config = "Host myserver\n  User alice\n  HostName 10.0.0.1\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(result.contains("User alice"));
        assert!(result.contains("HostName 10.0.0.1"));
    }

    #[test]
    fn remove_forward_only_comments_and_blank_lines() {
        let config = "# just a comment\n\n# another comment\n";
        let result = remove_forward_from_content(config, "myserver");
        assert!(result.contains("# just a comment"));
        assert!(result.contains("# another comment"));
    }
}
