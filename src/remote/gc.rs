use anyhow::Result;
use colored::Colorize;
use std::fs;
use std::path::Path;
use std::time::{Duration, SystemTime};

/// Run GC against the default `/tmp` directory.
pub fn run(max_age: u64) -> Result<()> {
    run_in_dir(max_age, Path::new("/tmp"))
}

/// Run GC in the given directory, removing cliptunnel temp files older than `max_age` minutes.
pub fn run_in_dir(max_age: u64, tmp_dir: &Path) -> Result<()> {
    let max_age_duration = Duration::from_secs(max_age * 60);
    let now = SystemTime::now();
    let mut deleted = 0u32;

    if !tmp_dir.exists() {
        println!("{}", "No /tmp directory found".yellow());
        return Ok(());
    }

    let entries = fs::read_dir(tmp_dir)?;
    for entry in entries.flatten() {
        let path = entry.path();
        let name = match path.file_name().and_then(|n| n.to_str()) {
            Some(n) => n.to_string(),
            None => continue,
        };

        if !name.starts_with("cliptunnel-") || !name.ends_with(".png") {
            continue;
        }

        let metadata = match fs::metadata(&path) {
            Ok(m) => m,
            Err(_) => continue,
        };

        let modified = match metadata.modified() {
            Ok(t) => t,
            Err(_) => continue,
        };

        if let Ok(age) = now.duration_since(modified) {
            if age > max_age_duration && fs::remove_file(&path).is_ok() {
                tracing::debug!("deleted {}", path.display());
                deleted += 1;
            }
        }
    }

    if deleted > 0 {
        println!(
            "{}",
            format!("Cleaned up {deleted} old cliptunnel temp file(s)").green()
        );
    } else {
        println!("{}", "No old cliptunnel temp files to clean up".green());
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs::File;
    use std::io::Write;
    use tempfile::TempDir;

    #[test]
    fn no_files_returns_ok() {
        let dir = TempDir::new().unwrap();
        let result = run_in_dir(30, dir.path());
        assert!(result.is_ok());
    }

    #[test]
    fn old_files_are_deleted() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("cliptunnel-old-test.png");
        File::create(&file_path).unwrap().write_all(b"img").unwrap();

        // Use max_age=0 so the threshold is 0 seconds -- any file is considered old.
        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        assert!(!file_path.exists(), "old file should have been deleted");
    }

    #[test]
    fn recent_files_are_kept() {
        let dir = TempDir::new().unwrap();
        let file_path = dir.path().join("cliptunnel-recent-test.png");
        File::create(&file_path).unwrap().write_all(b"img").unwrap();

        // Use a very large max_age so the freshly created file is well within range.
        let result = run_in_dir(999_999, dir.path());
        assert!(result.is_ok());
        assert!(file_path.exists(), "recent file should be kept");
    }

    #[test]
    fn non_matching_filenames_are_ignored() {
        let dir = TempDir::new().unwrap();

        // Files that don't match the "cliptunnel-*.png" pattern
        let random_file = dir.path().join("random-file.png");
        let no_ext = dir.path().join("cliptunnel-noext");
        let wrong_prefix = dir.path().join("other-thing.png");

        for p in [&random_file, &no_ext, &wrong_prefix] {
            File::create(p).unwrap().write_all(b"data").unwrap();
        }

        // max_age=0 would delete any matching file, but these should not match.
        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        assert!(
            random_file.exists(),
            "non-matching file should not be deleted"
        );
        assert!(
            no_ext.exists(),
            "file without .png extension should not be deleted"
        );
        assert!(
            wrong_prefix.exists(),
            "file without cliptunnel- prefix should not be deleted"
        );
    }

    #[test]
    fn nonexistent_directory_returns_ok() {
        let dir = TempDir::new().unwrap();
        let nonexistent = dir.path().join("does-not-exist");
        let result = run_in_dir(30, &nonexistent);
        assert!(result.is_ok());
    }

    #[test]
    fn mixed_matching_and_non_matching_files() {
        let dir = TempDir::new().unwrap();
        let matching = dir.path().join("cliptunnel-abc123.png");
        let non_matching = dir.path().join("other-file.txt");

        File::create(&matching).unwrap().write_all(b"img").unwrap();
        File::create(&non_matching)
            .unwrap()
            .write_all(b"text")
            .unwrap();

        // max_age=0 so matching file is "old"
        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        assert!(!matching.exists(), "matching file should be deleted");
        assert!(non_matching.exists(), "non-matching file should survive");
    }

    #[test]
    fn cliptunnel_file_with_wrong_extension_not_deleted() {
        let dir = TempDir::new().unwrap();
        let jpg = dir.path().join("cliptunnel-test.jpg");
        let txt = dir.path().join("cliptunnel-test.txt");

        for p in [&jpg, &txt] {
            File::create(p).unwrap().write_all(b"data").unwrap();
        }

        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        assert!(jpg.exists(), ".jpg should not be deleted");
        assert!(txt.exists(), ".txt should not be deleted");
    }

    #[test]
    fn multiple_old_files_all_deleted() {
        let dir = TempDir::new().unwrap();
        let files: Vec<_> = (0..5)
            .map(|i| {
                let p = dir.path().join(format!("cliptunnel-{i}.png"));
                File::create(&p).unwrap().write_all(b"img").unwrap();
                p
            })
            .collect();

        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        for f in &files {
            assert!(!f.exists(), "{} should be deleted", f.display());
        }
    }

    #[test]
    fn subdirectories_are_ignored() {
        let dir = TempDir::new().unwrap();
        // Create a subdirectory that matches the pattern name
        let subdir = dir.path().join("cliptunnel-subdir.png");
        fs::create_dir(&subdir).unwrap();

        // Should not crash trying to delete a directory
        let result = run_in_dir(0, dir.path());
        assert!(result.is_ok());
        assert!(subdir.exists(), "subdirectory should not be affected by GC");
    }
}
