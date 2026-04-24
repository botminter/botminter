use anyhow::{bail, Result};

use crate::config;
use crate::formation;
use crate::team::Team;

/// Handles `bm stop [member] [-t team] [--force] [--bridge] [--all]`.
pub fn run(team_flag: Option<&str>, force: bool, member_filter: Option<&str>, bridge_flag: bool, stop_all: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let local_formation = formation::create_local_formation(&team.name)?;
    let team_api = Team::new(team, local_formation);

    let result = team_api.stop(&cfg, member_filter, force, bridge_flag, stop_all)?;

    // Display: specific member not running
    if let (true, Some(target)) = (result.members.no_members_running, member_filter) {
        println!(
            "Member '{}' is not running for team '{}'.",
            target, team.name
        );
        return Ok(());
    }

    // Display: no members running at all
    if result.members.no_members_running {
        println!("No members running for team '{}'.", team.name);
    } else {
        for m in &result.members.stopped {
            if m.already_exited {
                eprintln!("{}... already exited", m.name);
            } else if m.forced {
                eprintln!("Stopping {} (force)... done", m.name);
            } else {
                eprintln!("Stopping {}... done", m.name);
            }
        }
        for m in &result.members.errors {
            eprintln!("Stopping {}... failed: {}", m.name, m.error);
        }

        println!(
            "\nStopped {} member(s), {} error(s).",
            result.members.stopped.len(),
            result.members.errors.len()
        );

        if !result.members.errors.is_empty() {
            bail!(
                "Some members could not be stopped gracefully. \
                 Use `bm stop -f` to force-kill."
            );
        }
    }

    // Display: bridge outcome
    match &result.bridge {
        Some(formation::BridgeStopOutcome::Stopped(name)) => {
            println!("Bridge '{}' stopped.", name);
        }
        Some(formation::BridgeStopOutcome::LeftRunning(name)) => {
            println!(
                "Bridge '{}' left running. Use `bm stop --bridge` to stop it.",
                name
            );
        }
        None => {}
    }

    // Display: daemon lifecycle
    if result.daemon_stopped {
        eprintln!("Daemon stopped.");
    } else if result.daemon_events_active {
        eprintln!(
            "\nNote: Daemon is running with polling enabled. \
             Enabled members may restart on GitHub events.\n\
             Use `bm disable <member>` to prevent auto-restart, \
             or `bm stop --all` to stop the daemon."
        );
    }

    Ok(())
}
