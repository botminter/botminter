use std::fs;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::config::{self, TeamEntry};
use crate::formation;
use crate::profile;
use crate::state::{self, MemberRuntime, RuntimeState};
use crate::topology::{self, Endpoint, MemberTopology, Topology};

/// Handles `bm start [-t team] [--formation <name>]`.
pub fn run(team_flag: Option<&str>, formation_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Schema version guard
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        bail!(
            "Team repo at {} has no botminter.yml. Is this a valid team repo?",
            team_repo.display()
        );
    }
    let manifest_contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml")?;
    let team_manifest: serde_yml::Value =
        serde_yml::from_str(&manifest_contents).context("Failed to parse team botminter.yml")?;
    let team_schema = team_manifest["schema_version"]
        .as_str()
        .unwrap_or("");
    let team_profile = team_manifest["profile"]
        .as_str()
        .unwrap_or(&team.profile);
    profile::check_schema_version(team_profile, team_schema)?;

    // Resolve formation
    let resolved_formation = formation::resolve_formation(&team_repo, formation_flag)?;

    // Non-local formations require current schema
    if let Some(ref fname) = resolved_formation {
        if fname != "local" {
            profile::require_current_schema(&team.name, team_schema)?;
            // Non-local formations delegate to formation manager
            let formation_cfg = formation::load(&team_repo, fname)?;
            if !formation_cfg.is_local() {
                return run_formation_manager(team, &team_repo, &formation_cfg, &cfg.workzone);
            }
        }
    }

    // Prerequisite: ralph must be installed
    if which::which("ralph").is_err() {
        bail!("'ralph' not found in PATH. Install ralph-orchestrator first.");
    }

    // Credentials → env vars
    let gh_token = require_gh_token(team)?;
    let telegram_token = team.credentials.telegram_bot_token.as_deref();

    // Discover members
    let members_dir = team_repo.join("team");
    if !members_dir.is_dir() {
        bail!("No members hired. Run `bm hire <role>` first.");
    }

    let member_dirs = list_member_dirs(&members_dir)?;
    if member_dirs.is_empty() {
        bail!("No members hired. Run `bm hire <role>` first.");
    }

    // Load state, clean up stale entries
    let mut state = state::load()?;
    let stale = state::cleanup_stale(&mut state);
    if !stale.is_empty() {
        for key in &stale {
            eprintln!("Cleaned stale entry for {}", key);
        }
        state::save(&state)?;
    }

    // Discover workspaces and launch
    let workzone = &cfg.workzone;
    let team_ws_base = workzone.join(&team.name);
    let mut launched = 0u32;
    let mut skipped = 0u32;
    let mut errors = 0u32;

    for member_dir_name in &member_dirs {
        let state_key = format!("{}/{}", team.name, member_dir_name);

        // Check if already running
        if let Some(rt) = state.members.get(&state_key) {
            if state::is_alive(rt.pid) {
                eprintln!(
                    "{}: already running (PID {})",
                    member_dir_name, rt.pid
                );
                skipped += 1;
                continue;
            }
            // Stale — remove and re-launch
            state.members.remove(&state_key);
        }

        // Find workspace
        let ws = find_workspace(&team_ws_base, member_dir_name);
        let ws = match ws {
            Some(ws) => ws,
            None => {
                eprintln!(
                    "{}: no workspace found. Run `bm teams sync` first.",
                    member_dir_name
                );
                errors += 1;
                continue;
            }
        };

        // Launch ralph
        match launch_ralph(&ws, &gh_token, telegram_token) {
            Ok(pid) => {
                let started_at = chrono::Utc::now().to_rfc3339();
                state.members.insert(
                    state_key,
                    MemberRuntime {
                        pid,
                        started_at,
                        workspace: ws,
                    },
                );
                state::save(&state)?;

                // Verify alive after 2 seconds
                thread::sleep(Duration::from_secs(2));
                if state::is_alive(pid) {
                    eprintln!("{}: started (PID {})", member_dir_name, pid);
                    launched += 1;
                } else {
                    eprintln!(
                        "{}: process exited immediately (PID {}). Check workspace logs.",
                        member_dir_name, pid
                    );
                    state.members.remove(&format!("{}/{}", team.name, member_dir_name));
                    state::save(&state)?;
                    errors += 1;
                }
            }
            Err(e) => {
                eprintln!("{}: failed to launch — {}", member_dir_name, e);
                errors += 1;
            }
        }
    }

    println!(
        "\nStarted {} member(s), skipped {} (already running), {} error(s).",
        launched, skipped, errors
    );

    if errors > 0 {
        bail!("Some members failed to start. See errors above.");
    }

    // Write topology file for v2 teams (when formations dir exists)
    if resolved_formation.is_some() {
        write_local_topology(&cfg.workzone, &team.name, &state)?;
    }

    Ok(())
}

/// Extracts GH_TOKEN from credentials, erroring if missing.
fn require_gh_token(team: &TeamEntry) -> Result<String> {
    team.credentials
        .gh_token
        .clone()
        .with_context(|| {
            format!(
                "No GH token configured for team '{}'. \
                 Run `bm init` or edit `~/.botminter/config.yml`.",
                team.name
            )
        })
}

/// Lists member directory names under `team/`.
fn list_member_dirs(team_dir: &std::path::Path) -> Result<Vec<String>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(team_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        dirs.push(name);
    }
    dirs.sort();
    Ok(dirs)
}

/// Finds the workspace path for a member.
/// Looks for `{team_ws_base}/{member_dir}/{project}/` first (with-project mode),
/// falls back to `{team_ws_base}/{member_dir}/` (no-project mode).
fn find_workspace(
    team_ws_base: &std::path::Path,
    member_dir_name: &str,
) -> Option<std::path::PathBuf> {
    let member_ws = team_ws_base.join(member_dir_name);
    if !member_ws.is_dir() {
        return None;
    }

    // Check for project subdirectories (workspace with project)
    if let Ok(entries) = fs::read_dir(&member_ws) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && path.join(".botminter").is_dir() {
                    return Some(path);
                }
            }
        }
    }

    // No-project mode: member_ws itself is the workspace
    if member_ws.join(".botminter").is_dir() {
        return Some(member_ws);
    }

    None
}

/// Launches `ralph run -p PROMPT.md` in the given workspace directory.
/// Returns the child PID.
fn launch_ralph(
    workspace: &std::path::Path,
    gh_token: &str,
    telegram_token: Option<&str>,
) -> Result<u32> {
    let mut cmd = Command::new("ralph");
    cmd.args(["run", "-p", "PROMPT.md"])
        .current_dir(workspace)
        .env("GH_TOKEN", gh_token)
        // Unset CLAUDECODE to avoid nested-Claude issues
        .env_remove("CLAUDECODE");

    if let Some(token) = telegram_token {
        cmd.env("RALPH_TELEGRAM_BOT_TOKEN", token);
    }

    // Detach from current process group
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn ralph in {}",
            workspace.display()
        )
    })?;

    Ok(child.id())
}

/// Resolves state for display/inspection by external callers.
pub fn resolve_member_status(
    state: &RuntimeState,
    team_name: &str,
    member_dir_name: &str,
) -> MemberStatus {
    let key = format!("{}/{}", team_name, member_dir_name);
    match state.members.get(&key) {
        Some(rt) => {
            if state::is_alive(rt.pid) {
                MemberStatus::Running {
                    pid: rt.pid,
                    started_at: rt.started_at.clone(),
                }
            } else {
                MemberStatus::Crashed {
                    pid: rt.pid,
                    started_at: rt.started_at.clone(),
                }
            }
        }
        None => MemberStatus::Stopped,
    }
}

/// Writes a local topology file after starting members.
fn write_local_topology(
    workzone: &std::path::Path,
    team_name: &str,
    state: &RuntimeState,
) -> Result<()> {
    use std::collections::HashMap;

    let team_prefix = format!("{}/", team_name);
    let mut members = HashMap::new();

    for (key, rt) in &state.members {
        if !key.starts_with(&team_prefix) {
            continue;
        }
        let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);
        members.insert(
            member_name.to_string(),
            MemberTopology {
                status: if crate::state::is_alive(rt.pid) {
                    "running".to_string()
                } else {
                    "stopped".to_string()
                },
                endpoint: Endpoint::Local {
                    pid: rt.pid,
                    workspace: rt.workspace.clone(),
                },
            },
        );
    }

    let topo = Topology {
        formation: "local".to_string(),
        created_at: chrono::Utc::now().to_rfc3339(),
        members,
    };

    let topo_path = topology::topology_path(workzone, team_name);
    topology::save(&topo_path, &topo)?;

    Ok(())
}

/// Runs a non-local formation manager (one-shot Ralph session).
fn run_formation_manager(
    team: &TeamEntry,
    team_repo: &std::path::Path,
    formation_cfg: &formation::FormationConfig,
    workzone: &std::path::Path,
) -> Result<()> {
    let mgr = formation_cfg.manager.as_ref().with_context(|| {
        format!(
            "Formation '{}' has no manager configuration",
            formation_cfg.name
        )
    })?;

    let formation_dir = formation::formations_dir(team_repo).join(&formation_cfg.name);
    let prompt_path = formation_dir.join(&mgr.prompt);
    let ralph_yml_path = formation_dir.join(&mgr.ralph_yml);

    // Prepare env vars
    let mut env_vars = Vec::new();
    if let Some(token) = &team.credentials.gh_token {
        env_vars.push(("GH_TOKEN".to_string(), token.clone()));
    }
    if let Some(token) = &team.credentials.telegram_bot_token {
        env_vars.push(("RALPH_TELEGRAM_BOT_TOKEN".to_string(), token.clone()));
    }
    // Pass workzone and team info to formation manager
    env_vars.push(("BM_WORKZONE".to_string(), workzone.display().to_string()));
    env_vars.push(("BM_TEAM_NAME".to_string(), team.name.clone()));
    env_vars.push(("BM_TEAM_REPO".to_string(), team_repo.display().to_string()));

    eprintln!(
        "Launching formation manager for '{}' formation...",
        formation_cfg.name
    );

    let status = crate::session::oneshot_ralph_session(
        &formation_dir,
        &prompt_path,
        &ralph_yml_path,
        &env_vars,
    )?;

    if !status.success() {
        bail!(
            "Formation manager '{}' failed (exit code: {:?})",
            formation_cfg.name,
            status.code()
        );
    }

    // Verify topology file was written
    let topo_path = topology::topology_path(workzone, &team.name);
    if !topo_path.exists() {
        bail!(
            "Formation manager completed but no topology file was written at {}",
            topo_path.display()
        );
    }

    eprintln!("Formation '{}' deployed successfully.", formation_cfg.name);
    Ok(())
}

/// Status of a team member process.
#[derive(Debug)]
pub enum MemberStatus {
    Running { pid: u32, started_at: String },
    Crashed { pid: u32, started_at: String },
    Stopped,
}

impl MemberStatus {
    pub fn label(&self) -> &'static str {
        match self {
            MemberStatus::Running { .. } => "running",
            MemberStatus::Crashed { .. } => "crashed",
            MemberStatus::Stopped => "stopped",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    // ── list_member_dirs ──────────────────────────────────────────

    #[test]
    fn list_member_dirs_returns_sorted_dirs_only() {
        let tmp = tempfile::tempdir().unwrap();
        // Create directories
        fs::create_dir(tmp.path().join("bob")).unwrap();
        fs::create_dir(tmp.path().join("alice")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        // Create a plain file
        fs::write(tmp.path().join("file.txt"), "hello").unwrap();

        let result = list_member_dirs(tmp.path()).unwrap();
        assert_eq!(result, vec!["alice", "bob"]);
    }

    #[test]
    fn list_member_dirs_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = list_member_dirs(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    // ── find_workspace ────────────────────────────────────────────

    #[test]
    fn find_workspace_project_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let team_ws_base = tmp.path();
        // Create: team_ws_base/member/project/.botminter/
        let project_dir = team_ws_base.join("member").join("my-project");
        fs::create_dir_all(project_dir.join(".botminter")).unwrap();

        let result = find_workspace(team_ws_base, "member");
        assert_eq!(result, Some(project_dir));
    }

    #[test]
    fn find_workspace_no_project_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let team_ws_base = tmp.path();
        // Create: team_ws_base/member/.botminter/ (no project subdir)
        let member_dir = team_ws_base.join("member");
        fs::create_dir_all(member_dir.join(".botminter")).unwrap();

        let result = find_workspace(team_ws_base, "member");
        assert_eq!(result, Some(member_dir));
    }

    #[test]
    fn find_workspace_missing_member_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_workspace(tmp.path(), "nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn find_workspace_no_botminter_marker() {
        let tmp = tempfile::tempdir().unwrap();
        // Create member dir without .botminter/
        fs::create_dir_all(tmp.path().join("member")).unwrap();

        let result = find_workspace(tmp.path(), "member");
        assert_eq!(result, None);
    }

    // ── resolve_member_status ─────────────────────────────────────

    #[test]
    fn resolve_member_status_running() {
        let mut state = RuntimeState::default();
        let alive_pid = std::process::id(); // current process, guaranteed alive
        state.members.insert(
            "team/member".to_string(),
            MemberRuntime {
                pid: alive_pid,
                started_at: "2026-02-21T10:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/ws"),
            },
        );

        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "running");
        match status {
            MemberStatus::Running { pid, .. } => assert_eq!(pid, alive_pid),
            other => panic!("Expected Running, got {:?}", other),
        }
    }

    #[test]
    fn resolve_member_status_crashed() {
        let mut state = RuntimeState::default();
        let dead_pid = 4_000_000u32; // unlikely to exist
        state.members.insert(
            "team/member".to_string(),
            MemberRuntime {
                pid: dead_pid,
                started_at: "2026-02-21T10:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/ws"),
            },
        );

        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "crashed");
        match status {
            MemberStatus::Crashed { pid, .. } => assert_eq!(pid, dead_pid),
            other => panic!("Expected Crashed, got {:?}", other),
        }
    }

    #[test]
    fn resolve_member_status_stopped() {
        let state = RuntimeState::default(); // empty — no entries
        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "stopped");
    }

    // ── require_gh_token ──────────────────────────────────────────

    #[test]
    fn require_gh_token_present() {
        let team = TeamEntry {
            name: "test-team".to_string(),
            path: PathBuf::from("/tmp/team"),
            profile: "scrum".to_string(),
            github_repo: "org/repo".to_string(),
            credentials: config::Credentials {
                gh_token: Some("ghp_test123".to_string()),
                telegram_bot_token: None,
                webhook_secret: None,
            },
        };
        let token = require_gh_token(&team).unwrap();
        assert_eq!(token, "ghp_test123");
    }

    #[test]
    fn require_gh_token_missing_errors_with_team_name() {
        let team = TeamEntry {
            name: "my-team".to_string(),
            path: PathBuf::from("/tmp/team"),
            profile: "scrum".to_string(),
            github_repo: "org/repo".to_string(),
            credentials: config::Credentials {
                gh_token: None,
                telegram_bot_token: None,
                webhook_secret: None,
            },
        };
        let err = require_gh_token(&team).unwrap_err();
        let msg = format!("{}", err);
        assert!(
            msg.contains("my-team"),
            "Error should mention team name, got: {msg}"
        );
    }

    // ── MemberStatus::label ───────────────────────────────────────

    #[test]
    fn member_status_labels() {
        assert_eq!(
            MemberStatus::Running {
                pid: 1,
                started_at: String::new()
            }
            .label(),
            "running"
        );
        assert_eq!(
            MemberStatus::Crashed {
                pid: 1,
                started_at: String::new()
            }
            .label(),
            "crashed"
        );
        assert_eq!(MemberStatus::Stopped.label(), "stopped");
    }
}
