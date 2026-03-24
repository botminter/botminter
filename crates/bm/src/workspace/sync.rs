use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};

use crate::profile::CodingAgentDef;
use super::repo::assemble_agent_dir_submodule;
use super::util::{copy_if_newer_verbose, git_cmd, git_cmd_output};

/// Events emitted during workspace sync for the caller to display.
#[derive(Debug)]
pub enum SyncEvent {
    UpdatingSubmodule(String),
    FileCopied(String),
    FileSkipped(String),
    AgentDirRebuilt,
    ChangesCommitted,
    PushedToRemote,
    NoChanges,
    BranchAlreadyOnIt(String),
    BranchCheckedOut(String),
    BranchCreated(String),
}

/// Result of a workspace sync operation.
#[derive(Debug, Default)]
pub struct SyncResult {
    /// Events that occurred during sync (for verbose display by the caller).
    pub events: Vec<SyncEvent>,
}

/// Lists member directory names from a members/ directory.
/// Returns sorted directory names, skipping hidden entries.
pub fn list_member_dirs(members_dir: &Path) -> Result<Vec<String>> {
    let mut dirs = Vec::new();
    if !members_dir.is_dir() {
        return Ok(dirs);
    }
    for entry in fs::read_dir(members_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('.') {
            dirs.push(name);
        }
    }
    dirs.sort();
    Ok(dirs)
}

/// Finds the workspace path for a member.
/// Returns Some if the member workspace dir exists and has the `.botminter.workspace` marker.
pub fn find_workspace(team_ws_base: &Path, member_dir_name: &str) -> Option<PathBuf> {
    let member_ws = team_ws_base.join(member_dir_name);
    if member_ws.is_dir() && member_ws.join(".botminter.workspace").exists() {
        Some(member_ws)
    } else {
        None
    }
}

/// Writes the `.botminter.workspace` marker file with workspace metadata.
pub(super) fn write_workspace_marker(ws_root: &Path, member_dir_name: &str) -> Result<()> {
    let content = format!(
        "# BotMinter workspace marker — do not delete\nmember: {}\n",
        member_dir_name,
    );
    fs::write(ws_root.join(".botminter.workspace"), content)
        .context("Failed to write .botminter.workspace marker")
}

/// Syncs an existing workspace by updating submodules, re-copying context files,
/// re-assembling agent directory, and committing+pushing any changes.
///
/// Uses the `team/` submodule model. Updates submodules to latest remote content,
/// checks out member branches, re-copies context files when newer, and rebuilds
/// agent dir symlinks idempotently.
///
/// Returns a `SyncResult` with events describing what happened. The caller
/// decides whether and how to display these events (e.g., only in verbose mode).
pub fn sync_workspace(
    ws_root: &Path,
    member_dir_name: &str,
    coding_agent: &CodingAgentDef,
    verbose: bool,
    push: bool,
) -> Result<SyncResult> {
    let mut result = SyncResult::default();
    let team_dir = ws_root.join("team");

    // Update submodules to latest remote content
    if team_dir.is_dir() {
        if verbose {
            result.events.push(SyncEvent::UpdatingSubmodule("team/".to_string()));
        }
        // Fetch and update to latest remote tracking branch
        git_cmd(ws_root, &[
            "-c", "protocol.file.allow=always",
            "submodule", "update", "--remote", "--merge", "team",
        ]).ok();

        // Checkout member branch (avoid detached HEAD)
        checkout_member_branch(&team_dir, member_dir_name, verbose, &mut result)?;
    }

    // Update project submodules
    let projects_dir = ws_root.join("projects");
    if projects_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&projects_dir) {
            for entry in entries.flatten() {
                if entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    let project_name = entry.file_name().to_string_lossy().to_string();
                    let project_path = format!("projects/{}", project_name);
                    if verbose {
                        result.events.push(SyncEvent::UpdatingSubmodule(project_path.clone()));
                    }
                    git_cmd(ws_root, &[
                        "-c", "protocol.file.allow=always",
                        "submodule", "update", "--remote", "--merge", &project_path,
                    ]).ok();

                    // Checkout member branch in project submodule
                    checkout_member_branch(&entry.path(), member_dir_name, verbose, &mut result)?;
                }
            }
        }
    }

    // Re-copy context files from team/members/<member>/
    let member_src = team_dir.join("members").join(member_dir_name);
    let files_to_sync = [
        (member_src.join("ralph.yml"), ws_root.join("ralph.yml"), "ralph.yml"),
        (
            member_src.join(&coding_agent.context_file),
            ws_root.join(&coding_agent.context_file),
            coding_agent.context_file.as_str(),
        ),
        (member_src.join("PROMPT.md"), ws_root.join("PROMPT.md"), "PROMPT.md"),
    ];

    for (src, dst, name) in &files_to_sync {
        let copied = copy_if_newer_verbose(src, dst)?;
        if verbose {
            if copied {
                result.events.push(SyncEvent::FileCopied(name.to_string()));
            } else if src.exists() {
                result.events.push(SyncEvent::FileSkipped(name.to_string()));
            }
        }
    }

    // Re-copy settings.local.json if source is newer
    let settings_src = member_src
        .join("coding-agent")
        .join("settings.local.json");
    let settings_dst = ws_root
        .join(&coding_agent.agent_dir)
        .join("settings.local.json");
    let settings_copied = copy_if_newer_verbose(&settings_src, &settings_dst)?;
    if verbose && settings_src.exists() {
        if settings_copied {
            result.events.push(SyncEvent::FileCopied("settings.local.json".to_string()));
        } else {
            result.events.push(SyncEvent::FileSkipped("settings.local.json".to_string()));
        }
    }

    // Discover project names from projects/ submodules
    let project_names: Vec<String> = if projects_dir.is_dir() {
        fs::read_dir(&projects_dir)
            .ok()
            .map(|entries| {
                entries
                    .flatten()
                    .filter(|e| e.file_type().map(|ft| ft.is_dir()).unwrap_or(false))
                    .map(|e| e.file_name().to_string_lossy().to_string())
                    .collect()
            })
            .unwrap_or_default()
    } else {
        Vec::new()
    };
    let project_name_refs: Vec<&str> = project_names.iter().map(|s| s.as_str()).collect();

    // Re-assemble agent dir from team/ submodule paths (idempotent)
    assemble_agent_dir_submodule(ws_root, member_dir_name, &project_name_refs, coding_agent)?;
    if verbose {
        result.events.push(SyncEvent::AgentDirRebuilt);
    }

    // Commit changes if any, then push
    git_cmd(ws_root, &["add", "-A"])?;
    let has_changes = git_cmd(ws_root, &["diff", "--cached", "--quiet"]).is_err();
    if has_changes {
        git_cmd(ws_root, &["commit", "-m", "Sync workspace with team repo"])?;
        if verbose {
            result.events.push(SyncEvent::ChangesCommitted);
        }
        if push {
            git_cmd(ws_root, &["push"]).with_context(|| {
                "Failed to push workspace changes. \
                 Ensure the workspace repo has a remote configured."
            })?;
            if verbose {
                result.events.push(SyncEvent::PushedToRemote);
            }
        }
    } else if verbose {
        result.events.push(SyncEvent::NoChanges);
    }

    Ok(result)
}

/// Checks out the member branch in a submodule, creating it if needed.
/// Avoids leaving the submodule in detached HEAD state.
fn checkout_member_branch(sub_dir: &Path, member_dir_name: &str, verbose: bool, result: &mut SyncResult) -> Result<()> {
    // Check current branch
    let current = git_cmd_output(sub_dir, &["rev-parse", "--abbrev-ref", "HEAD"])
        .unwrap_or_default();
    let current = current.trim();

    if current == member_dir_name {
        if verbose {
            result.events.push(SyncEvent::BranchAlreadyOnIt(member_dir_name.to_string()));
        }
        return Ok(());
    }

    // Try checkout existing, fall back to creating
    if git_cmd(sub_dir, &["checkout", member_dir_name]).is_ok() {
        if verbose {
            result.events.push(SyncEvent::BranchCheckedOut(member_dir_name.to_string()));
        }
    } else {
        git_cmd(sub_dir, &["checkout", "-b", member_dir_name])?;
        if verbose {
            result.events.push(SyncEvent::BranchCreated(member_dir_name.to_string()));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workspace::repo::tests::{
        claude_code_agent, setup_team_repo_for_ws, test_ws_params,
    };
    use crate::workspace::repo::create_workspace_repo;

    /// Helper: create a workspace using the submodule model for sync tests.
    fn setup_syncable_workspace(tmp: &Path) -> (PathBuf, String, CodingAgentDef) {
        let member = "arch-01"; // Must match setup_team_repo_for_ws member
        let team_repo = setup_team_repo_for_ws(tmp);
        let workspace_base = tmp.join("workzone");
        fs::create_dir_all(&workspace_base).unwrap();
        let agent = claude_code_agent();
        let params = test_ws_params(&team_repo, &workspace_base, member, &[], &agent);
        create_workspace_repo(&params).unwrap();

        let ws = workspace_base.join(member);
        (ws, member.to_string(), agent)
    }

    #[test]
    fn sync_recopies_changed_ralph_yml() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // Verify initial content
        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "v: 1"
        );

        // Modify ralph.yml in team/ submodule (simulating upstream change)
        let source = ws.join("team/members").join(&member).join("ralph.yml");
        fs::write(&source, "updated: true").unwrap();

        // Ensure source is newer
        let now = filetime::FileTime::from_unix_time(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                + 2,
            0,
        );
        filetime::set_file_mtime(&source, now).unwrap();

        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "updated: true",
            "Sync should re-copy the updated ralph.yml"
        );
    }

    #[test]
    fn sync_reassembles_agent_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        let agents_dir = ws.join(".claude/agents");
        let initial_count = fs::read_dir(&agents_dir)
            .unwrap()
            .filter_map(|e| e.ok())
            .count();

        // Add a new agent file in team/ submodule
        let member_agents = ws
            .join("team/members")
            .join(&member)
            .join("coding-agent/agents");
        fs::create_dir_all(&member_agents).unwrap();
        fs::write(member_agents.join("new-agent.md"), "# New Agent").unwrap();

        sync_workspace(&ws, &member, &agent, false, false).unwrap();

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
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // Run sync twice
        sync_workspace(&ws, &member, &agent, false, false).unwrap();
        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        // Verify workspace is still correct
        assert!(ws.join("PROMPT.md").exists());
        assert!(ws.join("AGENTS.md").exists());
        assert!(ws.join("ralph.yml").exists());
        assert!(ws.join(".claude/agents").is_dir());
        assert_eq!(fs::read_to_string(ws.join("PROMPT.md")).unwrap(), "# P");
    }

    #[test]
    fn sync_commits_changes() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // Modify a context file in the team submodule and commit it
        // (simulating an upstream change that arrives via submodule update)
        let team_sub = ws.join("team");
        let source = team_sub.join("members").join(&member).join("ralph.yml");
        fs::write(&source, "updated: true").unwrap();
        git_cmd(&team_sub, &["add", "-A"]).unwrap();
        git_cmd(&team_sub, &["commit", "-m", "upstream change"]).unwrap();

        // Ensure source is newer than workspace copy
        let now = filetime::FileTime::from_unix_time(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs() as i64
                + 2,
            0,
        );
        filetime::set_file_mtime(&source, now).unwrap();

        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        // The workspace ralph.yml should have the updated content
        assert_eq!(
            fs::read_to_string(ws.join("ralph.yml")).unwrap(),
            "updated: true",
            "Sync should re-copy the updated ralph.yml"
        );

        // Working tree should be clean after sync (changes committed)
        let status = git_cmd_output(&ws, &["status", "--porcelain"]).unwrap();
        assert!(
            status.trim().is_empty(),
            "Working tree should be clean after sync, got: {}",
            status
        );
    }

    #[test]
    fn sync_skips_unchanged_files() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // Sync once — no changes expected
        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        // Count commits before and after a second sync
        let log_before = git_cmd_output(&ws, &["rev-list", "--count", "HEAD"]).unwrap();

        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        let log_after = git_cmd_output(&ws, &["rev-list", "--count", "HEAD"]).unwrap();
        assert_eq!(
            log_before.trim(),
            log_after.trim(),
            "No new commits should be created when nothing changed"
        );
    }

    #[test]
    fn sync_member_branch_in_team_submodule() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // After initial create, team/ submodule should be on member branch
        let team_sub = ws.join("team");
        let branch = git_cmd_output(&team_sub, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(branch.trim(), member, "team/ should be on member branch");

        // Sync and verify branch is still correct (not detached HEAD)
        sync_workspace(&ws, &member, &agent, false, false).unwrap();

        let branch = git_cmd_output(&team_sub, &["rev-parse", "--abbrev-ref", "HEAD"]).unwrap();
        assert_eq!(
            branch.trim(),
            member,
            "team/ should remain on member branch after sync (not detached HEAD)"
        );
    }

    #[test]
    fn sync_verbose_runs_without_error() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, member, agent) = setup_syncable_workspace(tmp.path());

        // Verbose mode should complete without error
        sync_workspace(&ws, &member, &agent, true, false).unwrap();

        // Workspace should still be valid
        assert!(ws.join("PROMPT.md").exists());
        assert!(ws.join("AGENTS.md").exists());
        assert!(ws.join("ralph.yml").exists());
    }
}
