use anyhow::{bail, Result};

use crate::config;
use crate::daemon::{self, DaemonClient};
use crate::daemon::sessions_api::StopBulkRequest;
use crate::formation::{self, BridgeStopOutcome};

/// Handles `bm stop [member] [-t team] [--force] [--bridge] [--all]`.
pub fn run(
    team_flag: Option<&str>,
    force: bool,
    member_filter: Option<&str>,
    bridge_flag: bool,
    stop_all: bool,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    // Try to connect to daemon
    match DaemonClient::connect(&team.name) {
        Ok(client) => {
            let req = if let Some(member) = member_filter {
                StopBulkRequest {
                    mode: "member".to_string(),
                    member: Some(member.to_string()),
                    force,
                }
            } else {
                StopBulkRequest {
                    mode: "autonomous".to_string(),
                    member: None,
                    force,
                }
            };

            match client.stop_sessions_bulk(&req) {
                Ok(resp) => {
                    let total_stopped = resp.deactivated + resp.killed;

                    for err in &resp.errors {
                        eprintln!("Error: {}", err);
                    }

                    if total_stopped == 0 && resp.errors.is_empty() {
                        if let Some(target) = member_filter {
                            println!(
                                "Member '{}' is not running for team '{}'.",
                                target, team.name
                            );
                        } else {
                            println!("No members running for team '{}'.", team.name);
                        }
                    } else {
                        println!(
                            "\nStopped {} member(s), {} error(s).",
                            total_stopped,
                            resp.errors.len()
                        );
                    }

                    if !resp.errors.is_empty() {
                        bail!(
                            "Some members could not be stopped gracefully. \
                             Use `bm stop -f` to force-kill."
                        );
                    }
                }
                Err(e) => {
                    bail!("Failed to stop sessions: {}", e);
                }
            }
        }
        Err(_) => {
            // Daemon not running — nothing to stop
            if let Some(target) = member_filter {
                println!(
                    "Member '{}' is not running for team '{}'.",
                    target, team.name
                );
            } else {
                println!("No members running for team '{}'.", team.name);
            }
        }
    }

    // Handle bridge (only when not stopping a specific member)
    if member_filter.is_none() {
        match formation::stop_bridge(team, &cfg, bridge_flag) {
            Ok(Some(BridgeStopOutcome::Stopped(name))) => {
                println!("Bridge '{}' stopped.", name);
            }
            Ok(Some(BridgeStopOutcome::LeftRunning(name))) => {
                println!("Bridge '{}' left running. Use `bm stop --bridge` to stop it.", name);
            }
            Ok(None) => {}
            Err(e) => eprintln!("Warning: bridge stop error: {}", e),
        }
    }

    // Stop daemon if --all is requested
    if stop_all {
        // Daemon already stopped or not found — that's fine
        if let Ok(()) = daemon::stop_daemon(&team.name) {
            eprintln!("Daemon stopped.");
        }
    }

    Ok(())
}
