use std::sync::atomic::AtomicBool;
use std::sync::Arc;

use anyhow::Result;

use crate::config;
use crate::state;
use crate::workspace;

use super::config::DaemonPaths;
use super::log::daemon_log;

/// Launches enabled team members using the single entry point (`start_local_members`).
///
/// This is called by the daemon poll loop and webhook handler. Only members
/// in the `enabled` set (via `bm enable`) are eligible for event-driven launch.
/// `bm start` bypasses this check — it always starts regardless of enable state.
///
/// Returns the number of members launched.
pub fn launch_members_oneshot(
    team_name: &str,
    paths: &DaemonPaths,
    _shutdown: &Arc<AtomicBool>,
) -> Result<u32> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, Some(team_name))?;
    let team_repo = team.path.join("team");

    // Only launch members that are in the enabled set.
    let runtime_state = state::load()?;
    let enabled = state::enabled_members(&runtime_state, team_name);
    if enabled.is_empty() {
        daemon_log(paths, "DEBUG", "No enabled members — skipping launch");
        return Ok(0);
    }

    // Discover all members, then filter to enabled ones.
    let members_dir = team_repo.join("members");
    let all_members = workspace::list_member_dirs(&members_dir)?;
    let enabled_members: Vec<&str> = all_members.iter()
        .filter(|m| enabled.contains(&format!("{}/{}", team_name, m)))
        .map(|m| m.as_str())
        .collect();

    if enabled_members.is_empty() {
        daemon_log(paths, "DEBUG", "No enabled members match discovered members");
        return Ok(0);
    }

    // Launch each enabled member individually to respect the member_filter.
    let mut total_launched = 0u32;
    for member in &enabled_members {
        let result = crate::formation::start_local_members(
            team,
            &cfg,
            &team_repo,
            Some(member),
            true,   // no_bridge — daemon doesn't manage bridge lifecycle
            None,   // no formation override
        )?;

        for m in &result.launched {
            daemon_log(paths, "INFO", &format!("{}: launched (PID {})", m.name, m.pid));
        }
        for m in &result.skipped {
            daemon_log(paths, "INFO", &format!("{}: already running (PID {})", m.name, m.pid));
        }
        for m in &result.errors {
            daemon_log(paths, "ERROR", &format!("{}: {}", m.name, m.error));
        }
        total_launched += result.launched.len() as u32;
    }

    Ok(total_launched)
}

/// Launches members one-shot with logging.
pub fn handle_member_launch(
    team_name: &str,
    paths: &DaemonPaths,
    shutdown: &Arc<AtomicBool>,
) {
    match launch_members_oneshot(team_name, paths, shutdown) {
        Ok(count) => {
            daemon_log(
                paths,
                "INFO",
                &format!("One-shot run complete: {} member(s) processed", count),
            );
        }
        Err(e) => {
            daemon_log(
                paths,
                "ERROR",
                &format!("Member launch failed: {}", e),
            );
        }
    }
}
