use std::path::Path;

use anyhow::Result;

use crate::bridge;
use crate::config::{BotminterConfig, TeamEntry};
use crate::daemon;
use crate::profile;
use crate::topology;
use crate::workspace;

use super::MemberStatus;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Aggregated status data for the team dashboard display.
pub struct StatusInfo {
    pub formation: Option<String>,
    pub project_names: Vec<String>,
    pub daemon: Option<DaemonDisplay>,
    pub members: Vec<MemberRow>,
    pub has_members: bool,
    pub crashed_cleaned: usize,
    pub bridge: Option<BridgeDisplay>,
    pub verbose: Option<VerboseDisplay>,
}

/// Daemon status info for display.
pub struct DaemonDisplay {
    pub pid: u32,
    pub mode: String,
    pub port: u16,
    pub interval_secs: u64,
}

/// A single member's status row.
pub struct MemberRow {
    pub name: String,
    pub role: String,
    pub status: MemberStatus,
    pub branch: String,
    pub enabled: bool,
}

/// Bridge status info for display.
pub struct BridgeDisplay {
    pub name: String,
    pub bridge_type: String,
    pub status: String,
    pub url: Option<String>,
    pub identities: Vec<BridgeIdentityRow>,
}

/// A single bridge identity mapping.
pub struct BridgeIdentityRow {
    pub member: String,
    pub bridge_user: String,
    pub user_id: String,
}

/// Verbose display info (workspace submodules + ralph CLI output).
pub struct VerboseDisplay {
    pub workspaces: Vec<WorkspaceVerbose>,
    pub ralph_sections: Vec<RalphMemberInfo>,
}

/// Workspace submodule info for a single member.
pub struct WorkspaceVerbose {
    pub member: String,
    pub submodules: Vec<SubmoduleRow>,
}

/// A submodule status row.
pub struct SubmoduleRow {
    pub name: String,
    pub status_label: String,
}

/// Ralph runtime info for a running member.
pub struct RalphMemberInfo {
    pub member: String,
    pub pid: u32,
    pub sections: Vec<(String, String)>,
}

// ---------------------------------------------------------------------------
// Gather
// ---------------------------------------------------------------------------

/// Gathers all status dashboard data. Cleans up crashed entries as a side effect.
pub fn gather_status(
    team: &TeamEntry,
    cfg: &BotminterConfig,
    verbose: bool,
) -> Result<StatusInfo> {
    let team_repo = team.path.join("team");
    let team_name = &team.name;

    // Formation
    let topo_path = topology::topology_path(&cfg.workzone, team_name);
    let topo = topology::load(&topo_path)?;
    let formation = topo.as_ref().map(|t| t.formation.clone());

    // Projects
    let project_names = match profile::read_team_repo_manifest(&team_repo) {
        Ok(m) => m.projects.iter().map(|p| p.name.clone()).collect(),
        Err(_) => Vec::new(),
    };

    // Daemon
    let daemon = gather_daemon_info(team_name);

    // Members
    let members_dir = team_repo.join("members");
    let member_dirs = profile::discover_member_dirs(&team_repo);
    let has_members = !member_dirs.is_empty();
    let mut runtime_state = super::load()?;

    let mut members = Vec::new();
    let mut crashed_keys: Vec<String> = Vec::new();

    for name in &member_dirs {
        let role = profile::read_member_role(&members_dir, name);
        let status = super::resolve_member_status(&runtime_state, team_name, name);
        let ws_path = team.path.join(name);
        let branch = if ws_path.join(".botminter.workspace").exists() {
            workspace::workspace_git_branch(&ws_path)
        } else {
            "—".to_string()
        };
        if matches!(&status, MemberStatus::Crashed { .. }) {
            crashed_keys.push(format!("{}/{}", team_name, name));
        }
        let key = format!("{}/{}", team_name, name);
        let enabled = super::is_enabled(&runtime_state, &key);
        members.push(MemberRow {
            name: name.clone(),
            role,
            status,
            branch,
            enabled,
        });
    }

    // Clean crashed
    let crashed_cleaned = crashed_keys.len();
    if !crashed_keys.is_empty() {
        for key in &crashed_keys {
            runtime_state.members.remove(key);
        }
        super::save(&runtime_state)?;
    }

    // Bridge
    let bridge_display = gather_bridge_info(&team_repo, team_name, cfg);

    // Verbose
    let verbose_display = if verbose {
        Some(gather_verbose(
            &member_dirs,
            &team.path,
            team_name,
        )?)
    } else {
        None
    };

    Ok(StatusInfo {
        formation,
        project_names,
        daemon,
        members,
        has_members,
        crashed_cleaned,
        bridge: bridge_display,
        verbose: verbose_display,
    })
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn gather_daemon_info(team_name: &str) -> Option<DaemonDisplay> {
    match daemon::query_status(team_name) {
        Ok(daemon::DaemonStatusInfo::Running { pid, config }) => Some(DaemonDisplay {
            pid,
            mode: config
                .as_ref()
                .map(|c| c.mode.clone())
                .unwrap_or_default(),
            port: config.as_ref().map(|c| c.port).unwrap_or(0),
            interval_secs: config.as_ref().map(|c| c.interval_secs).unwrap_or(0),
        }),
        _ => None,
    }
}

fn gather_bridge_info(
    team_repo: &Path,
    team_name: &str,
    cfg: &BotminterConfig,
) -> Option<BridgeDisplay> {
    let bridge_dir = match bridge::discover(team_repo, team_name) {
        Ok(Some(dir)) => dir,
        _ => return None,
    };
    let state_path = bridge::state_path(&cfg.workzone, team_name);
    let b = match bridge::Bridge::new(bridge_dir, state_path, team_name.to_string()) {
        Ok(b) if b.is_active() => b,
        _ => return None,
    };

    let mut identities = Vec::new();
    let mut entries: Vec<_> = b.identities().iter().collect();
    entries.sort_by_key(|(k, _)| (*k).clone());
    for (username, identity) in entries {
        identities.push(BridgeIdentityRow {
            member: username.clone(),
            bridge_user: identity.username.clone(),
            user_id: identity.user_id.clone(),
        });
    }

    Some(BridgeDisplay {
        name: b.bridge_name().to_string(),
        bridge_type: b.bridge_type().to_string(),
        status: b.status().to_string(),
        url: b.service_url().map(|s| s.to_string()),
        identities,
    })
}

fn gather_verbose(
    member_dirs: &[String],
    team_path: &Path,
    team_name: &str,
) -> Result<VerboseDisplay> {
    let mut workspaces = Vec::new();
    for name in member_dirs {
        let ws_path = team_path.join(name);
        if !ws_path.join(".botminter.workspace").exists() {
            continue;
        }
        let submodules = workspace::workspace_submodule_status(&ws_path);
        if !submodules.is_empty() {
            workspaces.push(WorkspaceVerbose {
                member: name.clone(),
                submodules: submodules
                    .iter()
                    .map(|s| SubmoduleRow {
                        name: s.name.clone(),
                        status_label: s.status.label().to_string(),
                    })
                    .collect(),
            });
        }
    }

    let runtime_state = super::load()?;
    let team_prefix = format!("{}/", team_name);
    let mut ralph_sections = Vec::new();

    for (key, rt) in &runtime_state.members {
        if !key.starts_with(&team_prefix) {
            continue;
        }
        if !super::is_alive(rt.pid) {
            continue;
        }
        let member_name = key.strip_prefix(&team_prefix).unwrap_or(key).to_string();
        let mut sections = Vec::new();
        for (label, args) in &[
            ("Hats", vec!["hats"]),
            ("Loops", vec!["loops", "list"]),
            ("Events", vec!["events"]),
            ("Bot", vec!["bot", "status"]),
        ] {
            if let Ok(output) = crate::session::run_ralph_cmd(&rt.workspace, args) {
                sections.push((label.to_string(), output));
            }
        }
        ralph_sections.push(RalphMemberInfo {
            member: member_name,
            pid: rt.pid,
            sections,
        });
    }

    Ok(VerboseDisplay {
        workspaces,
        ralph_sections,
    })
}
