use std::fs;
use std::os::unix::fs as unix_fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use anyhow::{bail, Context, Result};

/// BM files that should be hidden from git in the workspace.
const BM_GITIGNORE_ENTRIES: &[&str] = &[
    ".botminter/",
    "PROMPT.md",
    "CLAUDE.md",
    "ralph.yml",
    ".claude/",
    ".ralph/",
    "poll-log.txt",
    ".gitignore",
];

/// Creates a workspace for a member, optionally with a target project.
///
/// With project:
///   `{workspace_base}/{member_dir}/{project}/` — fork clone at member branch
///   plus `.botminter/` clone and surfaced files.
///
/// Without project (no-project mode):
///   `{workspace_base}/{member_dir}/` — git init + `.botminter/` + surfaced files.
pub fn create_workspace(
    team_repo_path: &Path,
    workspace_base: &Path,
    member_dir_name: &str,
    project: Option<(&str, &str)>, // (project_name, fork_url)
    github_repo: Option<&str>,
) -> Result<()> {
    let member_ws = workspace_base.join(member_dir_name);
    fs::create_dir_all(&member_ws)
        .with_context(|| format!("Failed to create workspace dir {}", member_ws.display()))?;

    let ws_root = match project {
        Some((project_name, fork_url)) => {
            let project_ws = member_ws.join(project_name);

            // Clone the fork into the project workspace
            git_cmd(
                &member_ws,
                &["clone", fork_url, &project_ws.to_string_lossy()],
            )
            .with_context(|| format!("Failed to clone fork {}", fork_url))?;

            // Checkout member branch (create if it doesn't exist remotely)
            if git_cmd(&project_ws, &["checkout", member_dir_name]).is_err() {
                git_cmd(&project_ws, &["checkout", "-b", member_dir_name])?;
            }

            project_ws
        }
        None => {
            // No-project mode: init a git repo so .git/info/exclude works
            git_cmd(&member_ws, &["init", "-b", "main"])?;
            member_ws.clone()
        }
    };

    // Clone team repo into .botminter/
    let team_repo_abs = fs::canonicalize(team_repo_path)
        .with_context(|| format!("Failed to resolve team repo {}", team_repo_path.display()))?;
    git_cmd(
        &ws_root,
        &["clone", &team_repo_abs.to_string_lossy(), ".botminter"],
    )
    .context("Failed to clone team repo into .botminter/")?;

    // Override .botminter/ remote to point to GitHub (instead of local path)
    if let Some(repo) = github_repo {
        let github_url = format!("https://github.com/{}.git", repo);
        let bm_dir = ws_root.join(".botminter");
        git_cmd(&bm_dir, &["remote", "set-url", "origin", &github_url])?;
    }

    // Surface files (symlinks + copy)
    surface_files(&ws_root, member_dir_name)?;

    // Assemble .claude/ directory
    assemble_claude_dir(&ws_root, member_dir_name, project.map(|(name, _)| name))?;

    // Write .gitignore
    write_gitignore(&ws_root)?;

    // Write .git/info/exclude and hide tracked BM files
    write_git_exclude(&ws_root)?;
    hide_tracked_bm_files(&ws_root)?;

    Ok(())
}

/// Syncs an existing workspace by pulling changes and re-assembling surfaced files.
pub fn sync_workspace(
    ws_root: &Path,
    member_dir_name: &str,
    project_name: Option<&str>,
    has_project: bool,
    github_repo: Option<&str>,
) -> Result<()> {
    let bm_dir = ws_root.join(".botminter");

    // Fix .botminter/ remote URL if it's a local path but GitHub URL is known
    if let Some(repo) = github_repo {
        if bm_dir.is_dir() {
            if let Ok(current_url) =
                git_cmd_output(&bm_dir, &["remote", "get-url", "origin"])
            {
                if !current_url.trim().contains("github.com") {
                    let github_url = format!("https://github.com/{}.git", repo);
                    git_cmd(&bm_dir, &["remote", "set-url", "origin", &github_url])?;
                }
            }
        }
    }

    // Pull .botminter/ (non-fatal — may not have a remote)
    if bm_dir.is_dir() {
        git_cmd(&bm_dir, &["pull"]).ok();
    }

    // Pull target project if it has a remote
    if has_project {
        let has_remote = git_cmd_output(ws_root, &["remote"])
            .map(|o| !o.trim().is_empty())
            .unwrap_or(false);
        if has_remote {
            git_cmd(ws_root, &["pull"]).ok();
        }
    }

    // Re-copy ralph.yml if source is newer
    copy_if_newer(
        &bm_dir.join("team").join(member_dir_name).join("ralph.yml"),
        &ws_root.join("ralph.yml"),
    )?;

    // Re-copy settings.local.json if source is newer
    copy_if_newer(
        &bm_dir
            .join("team")
            .join(member_dir_name)
            .join("agent")
            .join("settings.local.json"),
        &ws_root.join(".claude").join("settings.local.json"),
    )?;

    // Re-assemble .claude/agents/ symlinks (idempotent)
    assemble_claude_dir(ws_root, member_dir_name, project_name)?;

    // Verify PROMPT.md and CLAUDE.md symlinks
    verify_symlink(
        &ws_root.join("PROMPT.md"),
        &bm_dir.join("team").join(member_dir_name).join("PROMPT.md"),
    )?;
    verify_symlink(
        &ws_root.join("CLAUDE.md"),
        &bm_dir.join("team").join(member_dir_name).join("CLAUDE.md"),
    )?;

    // Ensure .git/info/exclude is up to date and hide tracked BM files
    write_git_exclude(ws_root)?;
    hide_tracked_bm_files(ws_root)?;

    Ok(())
}

/// Assembles the `.claude/` directory from three scopes:
///
/// 1. Team-level: `.botminter/agent/agents/*.md`
/// 2. Project-level: `.botminter/projects/{project}/agent/agents/*.md`
/// 3. Member-level: `.botminter/team/{member_dir}/agent/agents/*.md`
///
/// Also copies `settings.local.json` from the member's agent dir if present.
pub fn assemble_claude_dir(
    ws_root: &Path,
    member_dir_name: &str,
    project_name: Option<&str>,
) -> Result<()> {
    let claude_agents = ws_root.join(".claude").join("agents");

    // Remove and recreate for idempotency
    if claude_agents.exists() {
        fs::remove_dir_all(&claude_agents).ok();
    }
    fs::create_dir_all(&claude_agents).context("Failed to create .claude/agents/")?;

    let bm_dir = ws_root.join(".botminter");

    // 1. Team-level agents
    symlink_md_files(&bm_dir.join("agent").join("agents"), &claude_agents)?;

    // 2. Project-level agents
    if let Some(proj) = project_name {
        symlink_md_files(
            &bm_dir
                .join("projects")
                .join(proj)
                .join("agent")
                .join("agents"),
            &claude_agents,
        )?;
    }

    // 3. Member-level agents
    symlink_md_files(
        &bm_dir
            .join("team")
            .join(member_dir_name)
            .join("agent")
            .join("agents"),
        &claude_agents,
    )?;

    // 4. Copy settings.local.json if present
    let src = bm_dir
        .join("team")
        .join(member_dir_name)
        .join("agent")
        .join("settings.local.json");
    if src.exists() {
        let dst = ws_root.join(".claude").join("settings.local.json");
        fs::copy(&src, &dst).context("Failed to copy settings.local.json")?;
    }

    Ok(())
}

/// Creates PROMPT.md and CLAUDE.md as relative symlinks and copies ralph.yml.
pub fn surface_files(ws_root: &Path, member_dir_name: &str) -> Result<()> {
    let member_bm = ws_root
        .join(".botminter")
        .join("team")
        .join(member_dir_name);

    let canonical = fs::canonicalize(&member_bm)
        .with_context(|| format!("Failed to canonicalize {}", member_bm.display()))?;

    // Symlink PROMPT.md and CLAUDE.md (relative)
    let canonical_ws = fs::canonicalize(ws_root)
        .with_context(|| format!("Failed to canonicalize {}", ws_root.display()))?;
    let rel = relative_path(&canonical_ws, &canonical);

    create_symlink(&rel.join("PROMPT.md"), &ws_root.join("PROMPT.md"))?;
    create_symlink(&rel.join("CLAUDE.md"), &ws_root.join("CLAUDE.md"))?;

    // Copy ralph.yml (not symlink — may be modified per-run)
    let ralph_src = canonical.join("ralph.yml");
    if ralph_src.exists() {
        fs::copy(&ralph_src, ws_root.join("ralph.yml")).context("Failed to copy ralph.yml")?;
    }

    Ok(())
}

/// Writes `.gitignore` in the workspace to hide BM files.
pub fn write_gitignore(ws_root: &Path) -> Result<()> {
    fs::write(ws_root.join(".gitignore"), gitignore_content())
        .context("Failed to write .gitignore")
}

/// Returns the gitignore content for a workspace.
pub fn gitignore_content() -> String {
    let mut lines: Vec<&str> = vec!["# botminter — managed workspace files"];
    lines.extend_from_slice(BM_GITIGNORE_ENTRIES);
    lines.push(""); // trailing newline
    lines.join("\n")
}

/// Writes `.git/info/exclude` with BM patterns.
pub fn write_git_exclude(ws_root: &Path) -> Result<()> {
    let git_dir = ws_root.join(".git");
    if !git_dir.is_dir() {
        return Ok(()); // No .git dir — skip
    }
    let exclude_dir = git_dir.join("info");
    fs::create_dir_all(&exclude_dir).context("Failed to create .git/info/")?;
    fs::write(exclude_dir.join("exclude"), gitignore_content())
        .context("Failed to write .git/info/exclude")
}

/// Hides botminter-managed files from git status when they are already tracked
/// by the project repo. Uses `git update-index --skip-worktree` for modified files
/// and `--assume-unchanged` for deleted files.
pub fn hide_tracked_bm_files(ws_root: &Path) -> Result<()> {
    let git_dir = ws_root.join(".git");
    if !git_dir.is_dir() {
        return Ok(());
    }

    // Collect all files under BM_GITIGNORE_ENTRIES that git currently tracks
    let output = Command::new("git")
        .args(["ls-files", "--full-name"])
        .current_dir(ws_root)
        .output()
        .context("Failed to run git ls-files")?;
    if !output.status.success() {
        return Ok(()); // Not a git repo or other issue — skip silently
    }

    let tracked = String::from_utf8_lossy(&output.stdout);
    let bm_tracked: Vec<&str> = tracked
        .lines()
        .filter(|f| {
            BM_GITIGNORE_ENTRIES.iter().any(|pattern| {
                let pat = pattern.trim_end_matches('/');
                f.starts_with(pat) || *f == pat
            })
        })
        .collect();

    if bm_tracked.is_empty() {
        return Ok(());
    }

    // Apply --skip-worktree to all matching tracked files (handles both
    // modified and deleted files — git won't report changes for them)
    let mut cmd = Command::new("git");
    cmd.arg("update-index").arg("--skip-worktree");
    for file in &bm_tracked {
        cmd.arg(file);
    }
    cmd.current_dir(ws_root);
    let _ = cmd.output(); // best-effort — some files may fail if deleted

    // For files that --skip-worktree doesn't cover (some deleted files),
    // fall back to --assume-unchanged
    let mut cmd = Command::new("git");
    cmd.arg("update-index").arg("--assume-unchanged");
    for file in &bm_tracked {
        cmd.arg(file);
    }
    cmd.current_dir(ws_root);
    let _ = cmd.output(); // best-effort

    Ok(())
}

// ── Private helpers ──────────────────────────────────────────────────

/// Symlinks all `.md` files from `src_dir` into `dst_dir` using relative paths.
/// Silently returns Ok if `src_dir` does not exist.
fn symlink_md_files(src_dir: &Path, dst_dir: &Path) -> Result<()> {
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
fn copy_if_newer(src: &Path, dst: &Path) -> Result<()> {
    if !src.exists() {
        return Ok(());
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
    Ok(())
}

/// Verifies a symlink points to the expected target. Re-creates as relative if wrong or broken.
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

/// Runs a git command in the given directory. Returns `Ok(())` on success.
fn git_cmd(dir: &Path, args: &[&str]) -> Result<()> {
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
fn git_cmd_output(dir: &Path, args: &[&str]) -> Result<String> {
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

    #[test]
    fn gitignore_content_has_all_bm_entries() {
        let content = gitignore_content();
        for entry in BM_GITIGNORE_ENTRIES {
            assert!(
                content.contains(entry),
                ".gitignore should contain '{}'",
                entry
            );
        }
    }

    #[test]
    fn claude_dir_assembly_creates_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();

        // Set up .botminter/ with agent md files at team and member levels
        let team_agents = ws.join(".botminter/agent/agents");
        fs::create_dir_all(&team_agents).unwrap();
        fs::write(team_agents.join("team-agent.md"), "# Team").unwrap();

        let member_agents = ws.join(".botminter/team/arch-01/agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("member-agent.md"), "# Member").unwrap();

        assemble_claude_dir(&ws, "arch-01", None).unwrap();

        let agents = ws.join(".claude/agents");
        assert!(agents.exists());

        let team_link = agents.join("team-agent.md");
        assert!(
            team_link.symlink_metadata().unwrap().file_type().is_symlink(),
            "team-agent.md should be a symlink"
        );

        let member_link = agents.join("member-agent.md");
        assert!(
            member_link
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink(),
            "member-agent.md should be a symlink"
        );
    }

    #[test]
    fn surface_files_creates_symlinks() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();

        let member = ws.join(".botminter/team/arch-01");
        fs::create_dir_all(&member).unwrap();
        fs::write(member.join("PROMPT.md"), "# Prompt").unwrap();
        fs::write(member.join("CLAUDE.md"), "# Claude").unwrap();
        fs::write(member.join("ralph.yml"), "version: 1").unwrap();

        surface_files(&ws, "arch-01").unwrap();

        // PROMPT.md and CLAUDE.md should be symlinks
        assert!(ws
            .join("PROMPT.md")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(ws.join("PROMPT.md")).unwrap(), "# Prompt");

        assert!(ws
            .join("CLAUDE.md")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(fs::read_to_string(ws.join("CLAUDE.md")).unwrap(), "# Claude");

        // ralph.yml should be a copy (not symlink)
        assert!(!ws
            .join("ralph.yml")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "version: 1"
        );
    }

    #[test]
    fn no_project_creates_simple_workspace() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_base = tmp.path().join("workzone/team");
        fs::create_dir_all(&workspace_base).unwrap();

        // Create a minimal team repo with git
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("team/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("agent/agents")).unwrap();
        fs::create_dir_all(team_repo.join("agent/agents")).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        create_workspace(&team_repo, &workspace_base, "arch-01", None, None).unwrap();

        let ws = workspace_base.join("arch-01");
        assert!(ws.join(".botminter").is_dir());
        assert!(ws
            .join("PROMPT.md")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(ws
            .join("CLAUDE.md")
            .symlink_metadata()
            .unwrap()
            .file_type()
            .is_symlink());
        assert!(ws.join("ralph.yml").exists());
        assert!(ws.join(".gitignore").exists());
        assert!(ws.join(".claude").is_dir());
    }

    // ── GitHub remote URL ───────────────────────────────────────────

    #[test]
    fn create_workspace_sets_github_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_base = tmp.path().join("workzone/team");
        fs::create_dir_all(&workspace_base).unwrap();

        // Create a minimal team repo with git
        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("team/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("agent/agents")).unwrap();
        fs::create_dir_all(team_repo.join("agent/agents")).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        create_workspace(
            &team_repo,
            &workspace_base,
            "arch-01",
            None,
            Some("myorg/my-team"),
        )
        .unwrap();

        let ws = workspace_base.join("arch-01");
        let bm_dir = ws.join(".botminter");
        let remote_url =
            git_cmd_output(&bm_dir, &["remote", "get-url", "origin"]).unwrap();
        assert_eq!(
            remote_url.trim(),
            "https://github.com/myorg/my-team.git",
            ".botminter/ remote should point to GitHub"
        );
    }

    #[test]
    fn create_workspace_without_github_keeps_local_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let workspace_base = tmp.path().join("workzone/team");
        fs::create_dir_all(&workspace_base).unwrap();

        let team_repo = tmp.path().join("team_repo");
        let member_cfg = team_repo.join("team/arch-01");
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "v: 1").unwrap();
        fs::create_dir_all(member_cfg.join("agent/agents")).unwrap();
        fs::create_dir_all(team_repo.join("agent/agents")).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        create_workspace(&team_repo, &workspace_base, "arch-01", None, None).unwrap();

        let ws = workspace_base.join("arch-01");
        let bm_dir = ws.join(".botminter");
        let remote_url =
            git_cmd_output(&bm_dir, &["remote", "get-url", "origin"]).unwrap();
        // Without github_repo, the remote should remain a local path
        assert!(
            !remote_url.contains("github.com"),
            "Without github_repo, remote should be local path, got: {}",
            remote_url.trim()
        );
    }

    #[test]
    fn sync_workspace_fixes_stale_local_remote() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member) = setup_syncable_workspace(tmp.path());

        let bm_dir = ws.join(".botminter");

        // Verify remote is initially a local path
        let initial_url =
            git_cmd_output(&bm_dir, &["remote", "get-url", "origin"]).unwrap();
        assert!(
            !initial_url.contains("github.com"),
            "Initial remote should be local"
        );

        // Sync with github_repo — should fix the remote
        sync_workspace(&ws, &member, None, false, Some("myorg/my-team")).unwrap();

        let fixed_url =
            git_cmd_output(&bm_dir, &["remote", "get-url", "origin"]).unwrap();
        assert_eq!(
            fixed_url.trim(),
            "https://github.com/myorg/my-team.git",
            "Sync should fix stale local remote to GitHub URL"
        );
    }

    // ── Symlink edge cases ──────────────────────────────────────────

    #[test]
    fn surface_files_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        let member = ws.join(".botminter/team/dev-01");
        fs::create_dir_all(&member).unwrap();
        fs::write(member.join("PROMPT.md"), "# Prompt").unwrap();
        fs::write(member.join("CLAUDE.md"), "# Claude").unwrap();
        fs::write(member.join("ralph.yml"), "v: 1").unwrap();

        // First call
        surface_files(&ws, "dev-01").unwrap();
        let target_1 = fs::read_link(ws.join("PROMPT.md")).unwrap();

        // Second call — should succeed without error
        surface_files(&ws, "dev-01").unwrap();
        let target_2 = fs::read_link(ws.join("PROMPT.md")).unwrap();

        assert_eq!(target_1, target_2, "Symlink target unchanged after re-surface");
        assert_eq!(fs::read_to_string(ws.join("PROMPT.md")).unwrap(), "# Prompt");
        assert_eq!(fs::read_to_string(ws.join("CLAUDE.md")).unwrap(), "# Claude");
    }

    #[test]
    fn surface_files_updates_wrong_target() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        let member = ws.join(".botminter/team/dev-01");
        fs::create_dir_all(&member).unwrap();
        fs::write(member.join("PROMPT.md"), "# Correct").unwrap();
        fs::write(member.join("CLAUDE.md"), "# C").unwrap();

        // Create a wrong symlink manually
        let wrong_target = tmp.path().join("wrong.md");
        fs::write(&wrong_target, "# Wrong").unwrap();
        unix_fs::symlink(&wrong_target, ws.join("PROMPT.md")).unwrap();

        // surface_files should replace the wrong symlink
        surface_files(&ws, "dev-01").unwrap();

        assert_eq!(
            fs::read_to_string(ws.join("PROMPT.md")).unwrap(),
            "# Correct",
            "Symlink should now point to the correct target"
        );
    }

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

    // ── Sync behavior ───────────────────────────────────────────────

    /// Helper: create a minimal workspace with .botminter/ as a git repo
    /// so sync_workspace can operate without external deps.
    fn setup_syncable_workspace(tmp: &Path) -> (std::path::PathBuf, String) {
        let member = "dev-01";
        let team_repo = tmp.join("team_repo");
        let member_cfg = team_repo.join("team").join(member);
        fs::create_dir_all(&member_cfg).unwrap();
        fs::write(member_cfg.join("PROMPT.md"), "# P").unwrap();
        fs::write(member_cfg.join("CLAUDE.md"), "# C").unwrap();
        fs::write(member_cfg.join("ralph.yml"), "original: true").unwrap();
        fs::create_dir_all(member_cfg.join("agent/agents")).unwrap();
        fs::create_dir_all(team_repo.join("agent/agents")).unwrap();

        git_cmd(&team_repo, &["init", "-b", "main"]).unwrap();
        git_cmd(&team_repo, &["add", "-f", "-A"]).unwrap();
        git_cmd(&team_repo, &["commit", "-m", "init"]).unwrap();

        let workspace_base = tmp.join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();
        create_workspace(&team_repo, &workspace_base, member, None, None).unwrap();

        let ws = workspace_base.join(member);
        (ws, member.to_string())
    }

    #[test]
    fn sync_recopies_changed_ralph_yml() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member) = setup_syncable_workspace(tmp.path());

        // Verify initial content
        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "original: true"
        );

        // Modify ralph.yml in .botminter/ (simulating upstream change)
        let source = ws.join(".botminter/team").join(&member).join("ralph.yml");
        fs::write(&source, "updated: true").unwrap();

        // Ensure source is newer (touch with small delay)
        let now = filetime::FileTime::from_unix_time(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                + 2,
            0,
        );
        filetime::set_file_mtime(&source, now).unwrap();

        sync_workspace(&ws, &member, None, false, None).unwrap();

        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "updated: true",
            "Sync should re-copy the updated ralph.yml"
        );
    }

    #[test]
    fn sync_reassembles_claude_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member) = setup_syncable_workspace(tmp.path());

        // Initially no member-level agents
        let agents_dir = ws.join(".claude/agents");
        let initial_count = fs::read_dir(&agents_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .count();

        // Add a new agent file in .botminter/
        let member_agents = ws
            .join(".botminter/team")
            .join(&member)
            .join("agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("new-agent.md"), "# New Agent").unwrap();

        sync_workspace(&ws, &member, None, false, None).unwrap();

        let new_count = fs::read_dir(&agents_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert!(
            new_count > initial_count,
            "Agent count should increase after sync: {} > {}",
            new_count,
            initial_count
        );
        assert!(
            agents_dir.join("new-agent.md").exists(),
            "new-agent.md should be symlinked after sync"
        );
    }

    #[test]
    fn sync_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member) = setup_syncable_workspace(tmp.path());

        // Run sync twice
        sync_workspace(&ws, &member, None, false, None).unwrap();
        sync_workspace(&ws, &member, None, false, None).unwrap();

        // Verify workspace is still correct
        assert!(ws.join("PROMPT.md").exists());
        assert!(ws.join("CLAUDE.md").exists());
        assert!(ws.join("ralph.yml").exists());
        assert!(ws.join(".claude/agents").is_dir());
        assert_eq!(fs::read_to_string(ws.join("PROMPT.md")).unwrap(), "# P");
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

    // ── assemble_claude_dir multi-scope ─────────────────────────────

    #[test]
    fn assemble_claude_dir_three_scopes() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();
        let bm = ws.join(".botminter");

        // Team-level agent
        let team_agents = bm.join("agent/agents");
        fs::create_dir_all(&team_agents).unwrap();
        fs::write(team_agents.join("team-wide.md"), "# Team").unwrap();

        // Project-level agent
        let proj_agents = bm.join("projects/myproj/agent/agents");
        fs::create_dir_all(&proj_agents).unwrap();
        fs::write(proj_agents.join("project-specific.md"), "# Project").unwrap();

        // Member-level agent
        let member_agents = bm.join("team/arch-01/agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("member-only.md"), "# Member").unwrap();

        assemble_claude_dir(&ws, "arch-01", Some("myproj")).unwrap();

        let agents = ws.join(".claude/agents");
        assert!(agents.join("team-wide.md").exists(), "Team agent missing");
        assert!(
            agents.join("project-specific.md").exists(),
            "Project agent missing"
        );
        assert!(agents.join("member-only.md").exists(), "Member agent missing");

        // All should be symlinks
        for name in &["team-wide.md", "project-specific.md", "member-only.md"] {
            assert!(
                agents
                    .join(name)
                    .symlink_metadata()
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "{} should be a symlink",
                name
            );
        }
    }

    #[test]
    fn assemble_claude_dir_settings_local_json() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();

        // Create settings.local.json in member agent dir
        let agent_dir = ws.join(".botminter/team/dev-01/agent");
        fs::create_dir_all(agent_dir.join("agents")).unwrap();
        fs::write(
            agent_dir.join("settings.local.json"),
            r#"{"key": "value"}"#,
        )
        .unwrap();

        assemble_claude_dir(&ws, "dev-01", None).unwrap();

        let dst = ws.join(".claude/settings.local.json");
        assert!(dst.exists(), "settings.local.json should be copied");
        assert_eq!(
            fs::read_to_string(&dst).unwrap(),
            r#"{"key": "value"}"#
        );
        // Should be a copy, not a symlink
        assert!(
            !dst.symlink_metadata().unwrap().file_type().is_symlink(),
            "settings.local.json should be a copy, not a symlink"
        );
    }

    #[test]
    fn assemble_claude_dir_empty_creates_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();

        // No .botminter/ agent dirs at all
        assemble_claude_dir(&ws, "dev-01", None).unwrap();

        let agents = ws.join(".claude/agents");
        assert!(agents.is_dir(), ".claude/agents/ should exist even with no agents");
        let count = fs::read_dir(&agents)
            .unwrap()
            .filter_map(|e| e.ok())
            .count();
        assert_eq!(count, 0, "Should be empty when no agent sources exist");
    }

    // ── Gitignore / git exclude ─────────────────────────────────────

    #[test]
    fn write_git_exclude_creates_file() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(ws.join(".git")).unwrap();

        write_git_exclude(&ws).unwrap();

        let exclude = ws.join(".git/info/exclude");
        assert!(exclude.exists(), ".git/info/exclude should be created");

        let content = fs::read_to_string(&exclude).unwrap();
        for entry in BM_GITIGNORE_ENTRIES {
            assert!(
                content.contains(entry),
                ".git/info/exclude should contain '{}'",
                entry
            );
        }
    }

    #[test]
    fn write_git_exclude_no_git_dir_noop() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        fs::create_dir_all(&ws).unwrap();
        // No .git/ directory

        // Should return Ok without error
        write_git_exclude(&ws).unwrap();

        // No .git/info/exclude should have been created
        assert!(!ws.join(".git").exists());
    }
}
