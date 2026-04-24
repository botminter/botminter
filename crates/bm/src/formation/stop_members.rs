use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::config::{BotminterConfig, TeamEntry};
use crate::state;
use crate::topology;

use super::MemberFailed;

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Outcome of stopping the local formation.
pub struct StopResult {
    pub stopped: Vec<MemberStopped>,
    pub errors: Vec<MemberFailed>,
    pub no_members_running: bool,
    pub topology_removed: bool,
}

pub struct MemberStopped {
    pub name: String,
    pub already_exited: bool,
    pub forced: bool,
}

/// What happened when we tried to stop the bridge.
pub enum BridgeStopOutcome {
    Stopped(String),
    LeftRunning(String),
}

// ---------------------------------------------------------------------------
// Stop — stop all members of a local formation
// ---------------------------------------------------------------------------

/// Maximum seconds to wait for graceful stop before giving up per member.
const GRACEFUL_TIMEOUT_SECS: u64 = 60;

/// Stops local formation members, optionally stopping the bridge afterwards.
pub fn stop_local_members(
    team: &TeamEntry,
    cfg: &BotminterConfig,
    member_filter: Option<&str>,
    force: bool,
) -> Result<StopResult> {
    let team_name = &team.name;
    let mut runtime_state = state::load()?;

    // Find running members for this team
    let team_prefix = format!("{}/", team_name);
    let all_running: Vec<(String, u32, PathBuf, bool)> = runtime_state
        .members
        .iter()
        .filter(|(key, _)| key.starts_with(&team_prefix))
        .map(|(key, rt)| (key.clone(), rt.pid, rt.workspace.clone(), rt.brain_mode))
        .collect();

    // Filter to a single member if requested
    let running: Vec<_> = if let Some(target) = member_filter {
        let target_key = format!("{}/{}", team_name, target);
        all_running
            .into_iter()
            .filter(|(k, _, _, _)| *k == target_key)
            .collect()
    } else {
        all_running
    };

    let mut result = StopResult {
        stopped: Vec::new(),
        errors: Vec::new(),
        no_members_running: running.is_empty(),
        topology_removed: false,
    };

    if running.is_empty() && member_filter.is_some() {
        // Specific member requested but not running — not an error, just info
        return Ok(result);
    }

    for (key, pid, workspace, brain_mode) in &running {
        let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);

        if !state::is_alive(*pid) {
            runtime_state.members.remove(key);
            state::save(&runtime_state)?;
            result.stopped.push(MemberStopped {
                name: member_name.to_string(),
                already_exited: true,
                forced: false,
            });
            continue;
        }

        if force {
            force_stop(*pid);
            runtime_state.members.remove(key);
            state::save(&runtime_state)?;
            result.stopped.push(MemberStopped {
                name: member_name.to_string(),
                already_exited: false,
                forced: true,
            });
        } else {
            // Brain members use SIGTERM directly (multiplexer handles shutdown).
            // Ralph members use `ralph loops stop` for graceful shutdown.
            let stop_result = if *brain_mode {
                graceful_stop_brain(*pid)
            } else {
                graceful_stop(workspace, *pid)
            };
            match stop_result {
                Ok(()) => {
                    runtime_state.members.remove(key);
                    state::save(&runtime_state)?;
                    result.stopped.push(MemberStopped {
                        name: member_name.to_string(),
                        already_exited: false,
                        forced: false,
                    });
                }
                Err(e) => {
                    result.errors.push(MemberFailed {
                        name: member_name.to_string(),
                        error: format!(
                            "{}\n  Hint: try `bm stop -f` to force-kill, or check workspace at {}",
                            e,
                            workspace.display()
                        ),
                    });
                }
            }
        }
    }

    // Topology cleanup — skip when stopping a single member
    if member_filter.is_none() {
        let topo_path = topology::topology_path(&cfg.workzone, team_name);
        if topo_path.exists() {
            topology::remove(&topo_path)?;
            result.topology_removed = true;
        }
    }

    Ok(result)
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Graceful stop for brain members: send SIGTERM, then poll for exit,
/// escalating to SIGKILL if the process does not terminate.
fn graceful_stop_brain(pid: u32) -> Result<()> {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }

    for _ in 0..GRACEFUL_TIMEOUT_SECS {
        if !state::is_alive(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }

    // Process ignored SIGTERM — escalate to SIGKILL.
    kill_escalate(pid);
    Ok(())
}

/// Graceful stop: run `ralph loops stop` in the workspace, then poll for exit,
/// escalating through SIGTERM and SIGKILL if needed.
fn graceful_stop(workspace: &Path, pid: u32) -> Result<()> {
    let output = Command::new("ralph")
        .args(["loops", "stop"])
        .current_dir(workspace)
        .output();

    match output {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            bail!("ralph loops stop failed: {}", stderr.trim());
        }
        Err(e) => {
            bail!("Failed to run ralph loops stop: {}", e);
        }
        Ok(_) => {}
    }

    for _ in 0..GRACEFUL_TIMEOUT_SECS {
        if !state::is_alive(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }

    // Process ignored graceful stop — escalate to SIGKILL.
    kill_escalate(pid);
    Ok(())
}

/// Force stop: SIGTERM, brief wait, escalate to SIGKILL if needed.
const FORCE_STOP_GRACE_SECS: u64 = 5;

fn force_stop(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    for _ in 0..FORCE_STOP_GRACE_SECS {
        if !state::is_alive(pid) {
            return;
        }
        thread::sleep(Duration::from_secs(1));
    }
    if state::is_alive(pid) {
        kill_escalate(pid);
    }
}

/// Send SIGKILL and wait briefly for the process to die.
fn kill_escalate(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
    // SIGKILL is unconditional — brief wait for the process to terminate.
    // The reaper thread (from launch) calls waitpid() to clean up the zombie.
    thread::sleep(Duration::from_millis(200));
}

/// Handles bridge stop lifecycle. Returns an outcome the command can display.
///
/// Extracted from commands/stop.rs per ADR-0007 — domain logic must not live
/// in command files.
pub fn stop_bridge(
    team: &TeamEntry,
    cfg: &BotminterConfig,
    bridge_flag: bool,
) -> Result<Option<BridgeStopOutcome>> {
    use crate::bridge::{self, BridgeStopResult};

    let should_stop = bridge_flag || team.bridge_lifecycle.stop_on_down;
    let team_repo = team.path.join("team");

    let bridge_dir = match bridge::discover(&team_repo, &team.name)? {
        Some(d) => d,
        None => return Ok(None),
    };

    let state_path = bridge::state_path(&cfg.workzone, &team.name);
    let mut b = bridge::Bridge::new(bridge_dir, state_path, team.name.clone())?;

    if should_stop {
        if b.is_local() && b.is_running() && which::which("just").is_ok() {
            let bridge_name = b.bridge_name().to_string();
            match b.stop()? {
                BridgeStopResult::Stopped => {
                    b.save()?;
                    return Ok(Some(BridgeStopOutcome::Stopped(bridge_name)));
                }
                BridgeStopResult::External => {}
            }
        }
    } else if b.is_local() && b.is_running() {
        return Ok(Some(BridgeStopOutcome::LeftRunning(
            b.bridge_name().to_string(),
        )));
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_result_tracks_all_outcomes() {
        let result = StopResult {
            stopped: vec![
                MemberStopped {
                    name: "alice".to_string(),
                    already_exited: true,
                    forced: false,
                },
                MemberStopped {
                    name: "bob".to_string(),
                    already_exited: false,
                    forced: true,
                },
            ],
            errors: vec![],
            no_members_running: false,
            topology_removed: true,
        };

        assert_eq!(result.stopped.len(), 2);
        assert!(result.stopped[0].already_exited);
        assert!(result.stopped[1].forced);
        assert!(result.errors.is_empty());
        assert!(!result.no_members_running);
        assert!(result.topology_removed);
    }

    #[test]
    fn stop_result_no_members_running() {
        let result = StopResult {
            stopped: Vec::new(),
            errors: Vec::new(),
            no_members_running: true,
            topology_removed: false,
        };

        assert!(result.no_members_running);
        assert!(result.stopped.is_empty());
    }
}
