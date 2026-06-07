use anyhow::{bail, Result};
use which::which;

use crate::config;
use crate::daemon::{self, DaemonClient};
use crate::daemon::sessions_api::StartSessionRequest;
use crate::formation;
use crate::profile;

/// Handles `bm start [member] [-t team] [--formation <name>] [--no-bridge] [--bridge-only]`.
pub fn run(
    team_flag: Option<&str>,
    formation_flag: Option<&str>,
    no_bridge: bool,
    bridge_only: bool,
    member_filter: Option<&str>,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Schema version guard
    let manifest = profile::validate_team_manifest(&team_repo, &team.profile)?;

    // Resolve formation
    let resolved_formation = formation::resolve_formation(&team_repo, formation_flag)?;

    // Non-local formations keep old behavior
    if let Some(ref fname) = resolved_formation {
        if fname != "local" {
            profile::require_current_schema(&team.name, &manifest.schema_version)?;
            let formation_cfg = formation::load(&team_repo, fname)?;
            if !formation_cfg.is_local() {
                eprintln!(
                    "Launching formation manager for '{}' formation...",
                    formation_cfg.name
                );
                let result = formation::run_formation_manager(
                    team, &team_repo, &formation_cfg, &cfg.workzone,
                )?;
                eprintln!(
                    "Formation '{}' deployed successfully.",
                    result.formation_name
                );
                return Ok(());
            }
        }
    }

    // Bridge-only mode: start bridge, skip sessions
    if bridge_only {
        if !no_bridge && team.bridge_lifecycle.start_on_up {
            if let Some(outcome) =
                formation::auto_start_bridge(&team_repo, &team.name, &cfg.workzone)
            {
                display_bridge_outcome(&outcome);
            }
        }
        return Ok(());
    }

    // Pre-flight: verify ralph is available (needed by daemon to launch Loop sessions)
    if which("ralph").is_err() {
        bail!("'ralph' not found in PATH. Install ralph-orchestrator first.");
    }

    // Ensure daemon is running, auto-starting if needed
    let client = match DaemonClient::connect(&team.name) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Starting daemon for team '{}'...", team.name);
            let mode = if team.daemon.polling { "poll" } else { "webhook" };
            daemon::start_daemon(
                &team.name,
                &team_repo,
                mode,
                0,
                team.daemon.interval,
                "127.0.0.1",
                "info",
            )?;
            DaemonClient::connect(&team.name)?
        }
    };

    // Bridge auto-start (before launching sessions)
    if !no_bridge && member_filter.is_none() && team.bridge_lifecycle.start_on_up {
        if let Some(outcome) =
            formation::auto_start_bridge(&team_repo, &team.name, &cfg.workzone)
        {
            display_bridge_outcome(&outcome);
        }
    }

    // Determine which members to start sessions for
    let members: Vec<String> = if let Some(m) = member_filter {
        vec![m.to_string()]
    } else {
        profile::discover_member_dirs(&team_repo)
    };

    if members.is_empty() {
        println!("No members hired yet. Run `bm hire <role>` to hire a member.");
        return Ok(());
    }

    // Collect members that already have an active session (to skip them)
    let active_members: Vec<String> = client
        .list_sessions()
        .ok()
        .map(|r| {
            r.sessions
                .into_iter()
                .filter(|s| s.current_state == "Creating" || s.current_state == "Active")
                .map(|s| s.member_name)
                .collect()
        })
        .unwrap_or_default();

    let mut launched = 0usize;
    let mut skipped = 0usize;
    let mut errors: Vec<String> = Vec::new();

    for member in &members {
        if active_members.contains(member) {
            eprintln!("{}: already running", member);
            skipped += 1;
            continue;
        }

        let req = StartSessionRequest {
            member_name: member.clone(),
            session_type: "Loop".to_string(),
            work_item_id: None,
        };
        match client.start_session(&req) {
            Ok(resp) if resp.ok => {
                let session_id = resp.session_id.as_deref().unwrap_or("unknown");
                if let Some(ws) = &resp.workspace_path {
                    eprintln!(
                        "{}: started (session {}, workspace: {})",
                        member, session_id, ws
                    );
                } else {
                    eprintln!("{}: started (session {})", member, session_id);
                }
                launched += 1;
            }
            Ok(resp) => {
                let err_msg = resp.error.as_deref().unwrap_or("unknown error");
                eprintln!("{}: {}", member, err_msg);
                errors.push(format!("{}: {}", member, err_msg));
            }
            Err(e) => {
                eprintln!("{}: {}", member, e);
                errors.push(format!("{}: {}", member, e));
            }
        }
    }

    println!(
        "\nStarted {} member(s), skipped {} (already running), {} error(s).",
        launched,
        skipped,
        errors.len()
    );

    if !errors.is_empty() {
        bail!("Some members failed to start. See errors above.");
    }

    Ok(())
}

fn display_bridge_outcome(outcome: &formation::BridgeAutoStartOutcome) {
    match outcome {
        formation::BridgeAutoStartOutcome::Started(name) => {
            println!("Bridge '{}' started.", name);
        }
        formation::BridgeAutoStartOutcome::Restarted(name) => {
            println!("Bridge '{}' health check failed, restarted.", name);
        }
        formation::BridgeAutoStartOutcome::AlreadyRunning(name) => {
            println!("Bridge '{}' already running.", name);
        }
        formation::BridgeAutoStartOutcome::External(name) => {
            println!("Bridge '{}' is external (managed externally).", name);
        }
        formation::BridgeAutoStartOutcome::JustNotFound => {
            eprintln!(
                "Warning: 'just' not found. Skipping bridge start. \
                 Install: https://just.systems/"
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    // -- list_member_dirs --

    #[test]
    fn list_member_dirs_returns_sorted_dirs_only() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("bob")).unwrap();
        fs::create_dir(tmp.path().join("alice")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join("file.txt"), "hello").unwrap();

        let result = crate::workspace::list_member_dirs(tmp.path()).unwrap();
        assert_eq!(result, vec!["alice", "bob"]);
    }

    #[test]
    fn list_member_dirs_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = crate::workspace::list_member_dirs(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    // -- find_workspace --

    #[test]
    fn find_workspace_with_marker() {
        let tmp = tempfile::tempdir().unwrap();
        let team_ws_base = tmp.path();
        let member_dir = team_ws_base.join("member");
        fs::create_dir_all(&member_dir).unwrap();
        fs::write(member_dir.join(".botminter.workspace"), "member: member\n").unwrap();

        let result = crate::workspace::find_workspace(team_ws_base, "member");
        assert_eq!(result, Some(member_dir));
    }

    #[test]
    fn find_workspace_old_botminter_dir_not_recognized() {
        let tmp = tempfile::tempdir().unwrap();
        let team_ws_base = tmp.path();
        let member_dir = team_ws_base.join("member");
        fs::create_dir_all(member_dir.join(".botminter")).unwrap();

        let result = crate::workspace::find_workspace(team_ws_base, "member");
        assert_eq!(result, None);
    }

    #[test]
    fn find_workspace_missing_member_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let result = crate::workspace::find_workspace(tmp.path(), "nonexistent");
        assert_eq!(result, None);
    }

    #[test]
    fn find_workspace_no_marker() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("member")).unwrap();

        let result = crate::workspace::find_workspace(tmp.path(), "member");
        assert_eq!(result, None);
    }

    // -- Per-member credential resolution tests --

    #[test]
    fn resolve_per_member_credential_from_store() {
        use crate::bridge::{self, CredentialStore, InMemoryCredentialStore};

        let store = InMemoryCredentialStore::new();
        store.store("alice", "alice-token").unwrap();
        store.store("bob", "bob-token").unwrap();

        let alice_token = bridge::resolve_credential_from_store("alice", &store).unwrap();
        let bob_token = bridge::resolve_credential_from_store("bob", &store).unwrap();

        assert_eq!(alice_token, Some("alice-token".to_string()));
        assert_eq!(bob_token, Some("bob-token".to_string()));
    }

    #[test]
    fn resolve_per_member_credential_missing_returns_none() {
        use crate::bridge::{self, InMemoryCredentialStore};

        let store = InMemoryCredentialStore::new();

        let result = bridge::resolve_credential_from_store("charlie", &store).unwrap();
        assert!(
            result.is_none(),
            "member without credential should get None"
        );
    }

    #[test]
    fn resolve_per_member_credential_env_var_priority() {
        use crate::bridge::{self, CredentialStore, InMemoryCredentialStore};

        let store = InMemoryCredentialStore::new();
        store.store("envpritest", "store-token").unwrap();

        // Set env var -- should take priority
        let env_key = "BM_BRIDGE_TOKEN_ENVPRITEST";
        std::env::set_var(env_key, "env-token");

        let result = bridge::resolve_credential_from_store("envpritest", &store).unwrap();
        assert_eq!(
            result,
            Some("env-token".to_string()),
            "env var should take priority over credential store"
        );

        std::env::remove_var(env_key);
    }

    #[test]
    fn launch_ralph_receives_per_member_credential() {
        use crate::formation;
        use anyhow::Result;

        type LaunchRalphFn = fn(&std::path::Path, Option<&str>, Option<&str>, Option<&str>, Option<&std::path::Path>) -> Result<u32>;
        let _: LaunchRalphFn = formation::launch_ralph;
    }

    #[test]
    fn check_robot_enabled_diagnostic() {
        use crate::formation;

        let tmp = tempfile::tempdir().unwrap();
        let ralph_yml = tmp.path().join("ralph.yml");
        fs::write(
            &ralph_yml,
            "preset: feature-development\nRObot:\n  enabled: false\n",
        )
        .unwrap();

        let has_credential = true;
        let robot_enabled = formation::check_robot_enabled_mismatch(&ralph_yml, has_credential);
        assert!(
            robot_enabled,
            "should return true when credential exists but RObot.enabled is false"
        );
    }
}
