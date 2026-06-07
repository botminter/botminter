use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// Symlinks all `.md` files from `src_dir` into `dst_dir` using relative paths.
/// Silently returns Ok if `src_dir` does not exist.
pub(super) fn symlink_md_files(src_dir: &Path, dst_dir: &Path) -> Result<()> {
    if !src_dir.is_dir() {
        return Ok(());
    }

    let canonical_src = fs::canonicalize(src_dir)
        .with_context(|| format!("Failed to canonicalize {}", src_dir.display()))?;
    let canonical_dst = fs::canonicalize(dst_dir)
        .with_context(|| format!("Failed to canonicalize {}", dst_dir.display()))?;
    let rel = relative_path(&canonical_dst, &canonical_src);

    for entry in fs::read_dir(&canonical_src)? {
        let entry = entry?;
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) == Some("md") {
            let filename = path.file_name().unwrap();
            let dst = dst_dir.join(filename);
            if dst.symlink_metadata().is_ok() {
                fs::remove_file(&dst).ok();
            }
            let rel_target = rel.join(filename);
            unix_fs::symlink(&rel_target, &dst).with_context(|| {
                format!("Failed to symlink {} → {}", dst.display(), rel_target.display())
            })?;
        }
    }

    Ok(())
}

/// Symlinks all subdirectories from `src_dir` into `dst_dir` using relative paths.
/// Used for skills and commands — each is a directory.
/// Silently returns Ok if `src_dir` does not exist.
pub(super) fn symlink_subdirs(src_dir: &Path, dst_dir: &Path) -> Result<()> {
    if !src_dir.is_dir() {
        return Ok(());
    }

    let canonical_src = fs::canonicalize(src_dir)
        .with_context(|| format!("Failed to canonicalize {}", src_dir.display()))?;
    let canonical_dst = fs::canonicalize(dst_dir)
        .with_context(|| format!("Failed to canonicalize {}", dst_dir.display()))?;
    let rel = relative_path(&canonical_dst, &canonical_src);

    for entry in fs::read_dir(&canonical_src)? {
        let entry = entry?;
        if !entry.path().is_dir() {
            continue;
        }
        let dir_name = entry.file_name();
        let dst = dst_dir.join(&dir_name);
        if dst.symlink_metadata().is_ok() {
            if dst.is_dir() && !dst.symlink_metadata().map(|m| m.file_type().is_symlink()).unwrap_or(false) {
                fs::remove_dir_all(&dst).ok();
            } else {
                fs::remove_file(&dst).ok();
            }
        }
        let rel_target = rel.join(&dir_name);
        unix_fs::symlink(&rel_target, &dst).with_context(|| {
            format!("Failed to symlink {} → {}", dst.display(), rel_target.display())
        })?;
    }

    Ok(())
}

/// Computes a relative path from `from_dir` to `to_path`.
/// Both paths must be absolute (canonicalized).
/// Example: relative_path("/a/b/c", "/a/b/d/e") → "../d/e"
fn relative_path(from_dir: &Path, to_path: &Path) -> PathBuf {
    let from_components: Vec<_> = from_dir.components().collect();
    let to_components: Vec<_> = to_path.components().collect();

    // Find common prefix length
    let common = from_components
        .iter()
        .zip(to_components.iter())
        .take_while(|(a, b)| a == b)
        .count();

    let mut result = PathBuf::new();
    // Go up from `from_dir` to the common ancestor
    for _ in common..from_components.len() {
        result.push("..");
    }
    // Go down from the common ancestor to `to_path`
    for component in &to_components[common..] {
        result.push(component);
    }
    result
}

/// Creates a symlink: `link_path` → `target`.
/// Target can be relative or absolute. Removes existing link/file at `link_path` first.
/// Skips if `target` doesn't exist (resolved relative to link's parent for relative targets).
#[cfg(test)]
fn create_symlink(target: &Path, link_path: &Path) -> Result<()> {
    // For relative targets, resolve against the link's parent to check existence
    let check_path = if target.is_relative() {
        link_path
            .parent()
            .map(|p| p.join(target))
            .unwrap_or_else(|| target.to_path_buf())
    } else {
        target.to_path_buf()
    };
    if !check_path.exists() {
        return Ok(());
    }
    if link_path.symlink_metadata().is_ok() {
        fs::remove_file(link_path).ok();
    }
    unix_fs::symlink(target, link_path).with_context(|| {
        format!(
            "Failed to symlink {} → {}",
            link_path.display(),
            target.display()
        )
    })
}

/// Copies `src` to `dst` only if `src` exists and is newer than `dst`.
#[cfg(test)]
fn copy_if_newer(src: &Path, dst: &Path) -> Result<()> {
    copy_if_newer_verbose(src, dst)?;
    Ok(())
}

/// Copies `src` to `dst` only if `src` exists and is newer than `dst`.
/// Returns `true` if a copy was made, `false` if skipped.
pub(super) fn copy_if_newer_verbose(src: &Path, dst: &Path) -> Result<bool> {
    if !src.exists() {
        return Ok(false);
    }
    let should_copy = if dst.exists() {
        let src_mod = fs::metadata(src)?.modified()?;
        let dst_mod = fs::metadata(dst)?.modified()?;
        src_mod > dst_mod
    } else {
        true
    };
    if should_copy {
        if let Some(parent) = dst.parent() {
            fs::create_dir_all(parent)?;
        }
        fs::copy(src, dst).with_context(|| {
            format!("Failed to copy {} → {}", src.display(), dst.display())
        })?;
    }
    Ok(should_copy)
}

/// Verifies a symlink points to the expected target. Re-creates as relative if wrong or broken.
#[cfg(test)]
fn verify_symlink(link: &Path, expected_target: &Path) -> Result<()> {
    if !expected_target.exists() {
        return Ok(());
    }
    let canonical_target = fs::canonicalize(expected_target)
        .with_context(|| format!("Failed to canonicalize {}", expected_target.display()))?;

    // Compute the relative path from the link's parent to the target
    let link_parent = link.parent().unwrap_or(Path::new("."));
    let canonical_parent = fs::canonicalize(link_parent).unwrap_or_else(|_| link_parent.to_path_buf());
    let rel = relative_path(&canonical_parent, &canonical_target);

    let needs_fix = match fs::read_link(link) {
        Ok(current) => {
            // Fix if absolute (we want relative) or if it resolves to a different file
            if current.is_absolute() {
                true
            } else {
                let resolved = link_parent.join(&current);
                match fs::canonicalize(&resolved) {
                    Ok(c) => c != canonical_target,
                    Err(_) => true, // broken symlink
                }
            }
        }
        Err(_) => true,
    };

    if needs_fix {
        if link.symlink_metadata().is_ok() {
            fs::remove_file(link).ok();
        }
        unix_fs::symlink(&rel, link)
            .with_context(|| format!("Failed to re-create symlink {}", link.display()))?;
    }
    Ok(())
}

/// Adds a git submodule, allowing the file protocol for local paths.
///
/// Git 2.38.1+ blocks `file://` transport in submodule adds by default
/// (CVE-2022-39253). We allow it since local clones are intentional here
/// (during `bm teams sync --repos` for local workspace setup). For remote
/// repos, URLs are HTTPS and this config has no effect.
pub(super) fn git_submodule_add(dir: &Path, url: &str, path: &str) -> Result<()> {
    // Check if submodule already exists via git
    let status = Command::new("git")
        .args(["submodule", "status", path])
        .current_dir(dir)
        .output();
    if let Ok(ref out) = status {
        if out.status.success() {
            // Submodule already registered — skip
            return Ok(());
        }
    }

    let output = Command::new("git")
        .args([
            "-c",
            "protocol.file.allow=always",
            "submodule",
            "add",
            url,
            path,
        ])
        .current_dir(dir)
        .output()
        .with_context(|| format!("Failed to run git submodule add {} {}", url, path))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!(
            "git submodule add {} {} failed: {}",
            url,
            path,
            stderr.trim()
        );
    }
    Ok(())
}

/// Returns the current git branch name for a workspace, or "unknown" on failure.
pub fn workspace_git_branch(ws_root: &Path) -> String {
    git_cmd_output(ws_root, &["rev-parse", "--abbrev-ref", "HEAD"])
        .map(|s| s.trim().to_string())
        .unwrap_or_else(|_| "unknown".to_string())
}

/// Describes the status of a single git submodule.
#[derive(Debug, Clone, PartialEq)]
pub struct SubmoduleStatus {
    pub name: String,
    pub status: SubmoduleState,
}

/// Whether a submodule is up-to-date or has new commits available.
#[derive(Debug, Clone, PartialEq)]
pub enum SubmoduleState {
    UpToDate,
    Behind,
    Modified,
    Uninitialized,
}

impl SubmoduleState {
    pub fn label(&self) -> &'static str {
        match self {
            SubmoduleState::UpToDate => "up-to-date",
            SubmoduleState::Behind => "behind",
            SubmoduleState::Modified => "modified",
            SubmoduleState::Uninitialized => "uninitialized",
        }
    }
}

/// Returns submodule status for all submodules in a workspace.
///
/// Uses `git submodule status` which prefixes each line with:
/// - ' ' (space) = up-to-date
/// - '+' = checked out to different commit than recorded
/// - '-' = not initialized
/// - 'U' = merge conflict
pub fn workspace_submodule_status(ws_root: &Path) -> Vec<SubmoduleStatus> {
    let output = match git_cmd_output(ws_root, &["submodule", "status"]) {
        Ok(o) => o,
        Err(_) => return Vec::new(),
    };

    output
        .lines()
        .filter(|line| !line.is_empty())
        .filter_map(|line| {
            // Format: " <hash> <path> (<desc>)" or "+<hash> <path> (<desc>)"
            let first_char = line.chars().next()?;
            let state = match first_char {
                ' ' => SubmoduleState::UpToDate,
                '+' => SubmoduleState::Modified,
                '-' => SubmoduleState::Uninitialized,
                _ => SubmoduleState::Behind,
            };
            // Extract the path (second whitespace-delimited field after the hash)
            let rest = line[1..].trim();
            let path = rest.split_whitespace().nth(1)?;
            Some(SubmoduleStatus {
                name: path.to_string(),
                status: state,
            })
        })
        .collect()
}

/// Returns the remote URL for a workspace repo, or None if not available.
pub fn workspace_remote_url(ws_root: &Path) -> Option<String> {
    git_cmd_output(ws_root, &["remote", "get-url", "origin"])
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// Runs a git command in the given directory. Returns `Ok(())` on success.
pub(super) fn git_cmd(dir: &Path, args: &[&str]) -> Result<()> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(())
}

/// Runs a git command and returns stdout as a String.
pub(super) fn git_cmd_output(dir: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .with_context(|| format!("Failed to run git {}", args.join(" ")))?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("git {} failed: {}", args.join(" "), stderr.trim());
    }
    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}


#[cfg(test)]
mod tests {
    use super::*;

    // ── Symlink edge cases ──────────────────────────────────────────

    #[test]
    fn create_symlink_replaces_regular_file() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("real.md");
        fs::write(&target, "# Real").unwrap();

        let link = tmp.path().join("link.md");
        fs::write(&link, "# Regular file occupying the path").unwrap();

        // create_symlink should remove the regular file and create a symlink
        create_symlink(&target, &link).unwrap();

        assert!(
            link.symlink_metadata().unwrap().file_type().is_symlink(),
            "Should be a symlink, not a regular file"
        );
        assert_eq!(fs::read_to_string(&link).unwrap(), "# Real");
    }

    #[test]
    fn verify_symlink_fixes_broken_link() {
        let tmp = tempfile::tempdir().unwrap();

        // Create the correct target
        let correct_target = tmp.path().join("correct.md");
        fs::write(&correct_target, "# Correct").unwrap();

        // Create a broken symlink (pointing to a non-existent path)
        let link = tmp.path().join("link.md");
        let ghost = tmp.path().join("ghost.md");
        unix_fs::symlink(&ghost, &link).unwrap();

        // Link exists as symlink but is broken (ghost doesn't exist)
        assert!(link.symlink_metadata().is_ok(), "Symlink metadata readable");
        assert!(!link.exists(), "Broken symlink — target doesn't exist");

        // verify_symlink should detect the mismatch and re-create
        verify_symlink(&link, &correct_target).unwrap();

        assert!(link.exists(), "Link should now resolve");
        assert_eq!(fs::read_to_string(&link).unwrap(), "# Correct");
    }

    #[test]
    fn verify_symlink_fixes_wrong_target() {
        let tmp = tempfile::tempdir().unwrap();

        let correct = tmp.path().join("correct.md");
        fs::write(&correct, "# Correct").unwrap();

        let wrong = tmp.path().join("wrong.md");
        fs::write(&wrong, "# Wrong").unwrap();

        let link = tmp.path().join("link.md");
        unix_fs::symlink(&wrong, &link).unwrap();
        assert_eq!(fs::read_to_string(&link).unwrap(), "# Wrong");

        verify_symlink(&link, &correct).unwrap();

        assert_eq!(
            fs::read_to_string(&link).unwrap(),
            "# Correct",
            "verify_symlink should re-point to the correct target"
        );
    }

    // ── copy_if_newer ───────────────────────────────────────────────

    #[test]
    fn copy_if_newer_skips_when_dest_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "old source").unwrap();
        fs::write(&dst, "newer dest").unwrap();

        // Make source older than dest
        let old_time = filetime::FileTime::from_unix_time(1_000_000, 0);
        filetime::set_file_mtime(&src, old_time).unwrap();
        let new_time = filetime::FileTime::from_unix_time(2_000_000, 0);
        filetime::set_file_mtime(&dst, new_time).unwrap();

        copy_if_newer(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "newer dest",
            "Destination should be unchanged when it is newer"
        );
    }

    #[test]
    fn copy_if_newer_copies_when_source_newer() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "newer source").unwrap();
        fs::write(&dst, "old dest").unwrap();

        // Make source newer than dest
        let old_time = filetime::FileTime::from_unix_time(1_000_000, 0);
        filetime::set_file_mtime(&dst, old_time).unwrap();
        let new_time = filetime::FileTime::from_unix_time(2_000_000, 0);
        filetime::set_file_mtime(&src, new_time).unwrap();

        copy_if_newer(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "newer source",
            "Destination should be overwritten when source is newer"
        );
    }

    #[test]
    fn copy_if_newer_copies_when_dest_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("src.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&src, "content").unwrap();

        assert!(!dst.exists());
        copy_if_newer(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "content",
            "Should copy when destination does not exist"
        );
    }

    #[test]
    fn copy_if_newer_skips_when_source_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let src = tmp.path().join("nonexistent.txt");
        let dst = tmp.path().join("dst.txt");
        fs::write(&dst, "preserved").unwrap();

        copy_if_newer(&src, &dst).unwrap();

        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            "preserved",
            "Should be a no-op when source doesn't exist"
        );
    }

    // ── workspace_git_branch ──────────────────────────────────────

    #[test]
    fn workspace_git_branch_returns_branch_name() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        git_cmd(ws, &["init", "-b", "my-feature"]).unwrap();
        git_cmd(ws, &["config", "user.email", "test@test.com"]).unwrap();
        git_cmd(ws, &["config", "user.name", "Test"]).unwrap();
        fs::write(ws.join("README.md"), "hello").unwrap();
        git_cmd(ws, &["add", "."]).unwrap();
        git_cmd(ws, &["commit", "-m", "init"]).unwrap();

        let branch = workspace_git_branch(ws);
        assert_eq!(branch, "my-feature");
    }

    #[test]
    fn workspace_git_branch_returns_unknown_for_non_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let branch = workspace_git_branch(tmp.path());
        assert_eq!(branch, "unknown");
    }

    // ── workspace_submodule_status ────────────────────────────────

    #[test]
    fn workspace_submodule_status_with_submodule() {
        let tmp = tempfile::tempdir().unwrap();

        // Create a "remote" repo to use as a submodule
        let remote = tmp.path().join("remote");
        fs::create_dir_all(&remote).unwrap();
        git_cmd(&remote, &["init", "-b", "main"]).unwrap();
        git_cmd(&remote, &["config", "user.email", "test@test.com"]).unwrap();
        git_cmd(&remote, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&remote, &["config", "commit.gpgsign", "false"]).unwrap();
        fs::write(remote.join("README.md"), "hello").unwrap();
        git_cmd(&remote, &["add", "."]).unwrap();
        git_cmd(&remote, &["commit", "-m", "init"]).unwrap();

        // Create workspace repo with a submodule
        let ws = tmp.path().join("ws");
        fs::create_dir_all(&ws).unwrap();
        git_cmd(&ws, &["init", "-b", "main"]).unwrap();
        git_cmd(&ws, &["config", "user.email", "test@test.com"]).unwrap();
        git_cmd(&ws, &["config", "user.name", "Test"]).unwrap();
        git_cmd(&ws, &["config", "commit.gpgsign", "false"]).unwrap();
        git_cmd(
            &ws,
            &[
                "-c", "protocol.file.allow=always",
                "submodule", "add", remote.to_str().unwrap(), "team",
            ],
        )
        .unwrap();
        git_cmd(&ws, &["commit", "-m", "add submodule"]).unwrap();

        let subs = workspace_submodule_status(&ws);
        assert_eq!(subs.len(), 1);
        assert_eq!(subs[0].name, "team");
        assert_eq!(subs[0].status, SubmoduleState::UpToDate);
    }

    #[test]
    fn workspace_submodule_status_empty_for_non_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let subs = workspace_submodule_status(tmp.path());
        assert!(subs.is_empty());
    }

    // ── workspace_remote_url ──────────────────────────────────────

    #[test]
    fn workspace_remote_url_returns_none_without_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        git_cmd(ws, &["init", "-b", "main"]).unwrap();
        git_cmd(ws, &["config", "user.email", "test@test.com"]).unwrap();
        git_cmd(ws, &["config", "user.name", "Test"]).unwrap();

        let url = workspace_remote_url(ws);
        assert!(url.is_none());
    }

    #[test]
    fn workspace_remote_url_returns_url_with_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path();
        git_cmd(ws, &["init", "-b", "main"]).unwrap();
        git_cmd(ws, &["config", "user.email", "test@test.com"]).unwrap();
        git_cmd(ws, &["config", "user.name", "Test"]).unwrap();
        git_cmd(
            ws,
            &["remote", "add", "origin", "https://github.com/org/repo.git"],
        )
        .unwrap();

        let url = workspace_remote_url(ws);
        assert_eq!(url, Some("https://github.com/org/repo.git".to_string()));
    }

    // ── SubmoduleState ────────────────────────────────────────────

    #[test]
    fn submodule_state_labels() {
        assert_eq!(SubmoduleState::UpToDate.label(), "up-to-date");
        assert_eq!(SubmoduleState::Behind.label(), "behind");
        assert_eq!(SubmoduleState::Modified.label(), "modified");
        assert_eq!(SubmoduleState::Uninitialized.label(), "uninitialized");
    }

}
