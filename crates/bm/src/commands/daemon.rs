use anyhow::Result;

use crate::config;
use crate::daemon::{self, DaemonStatusInfo};
use crate::state;

/// Handles `bm daemon start`.
pub fn start(
    team_flag: Option<&str>,
    mode: &str,
    port: u16,
    interval: u64,
    bind: &str,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    eprintln!(
        "Starting daemon for team '{}' in {} mode...",
        team.name, mode
    );

    let result = daemon::start_daemon(&team.name, &team_repo, mode, port, interval, bind)?;

    println!("Daemon started (PID {})", result.pid);
    println!("Console: http://localhost:{}", port);
    Ok(())
}

/// Handles `bm daemon stop`.
pub fn stop(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    daemon::stop_daemon(&team.name)?;

    println!("Daemon stopped");
    Ok(())
}

/// Handles `bm daemon status`.
pub fn status(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let info = daemon::query_status(&team.name)?;

    match info {
        DaemonStatusInfo::Running { pid, config } => {
            println!("Daemon: running (PID {})", pid);
            if let Some(daemon_cfg) = config {
                match daemon_cfg.mode.as_str() {
                    "webhook" => println!("Mode: webhook (port {})", daemon_cfg.port),
                    "poll" => {
                        println!("Mode: poll (interval {}s)", daemon_cfg.interval_secs)
                    }
                    other => println!("Mode: {}", other),
                }
                println!("Console: http://localhost:{}", daemon_cfg.port);
                println!("Team: {}", daemon_cfg.team);
                println!("Started: {}", format_timestamp(&daemon_cfg.started_at));
            } else {
                println!("Team: {}", team.name);
            }

            let runtime_state = state::load().unwrap_or_default();
            let enabled = state::enabled_members(&runtime_state, &team.name);
            if enabled.is_empty() {
                println!("Enabled members: none");
            } else {
                let mut names: Vec<&str> = enabled.iter()
                    .map(|k| k.strip_prefix(&format!("{}/", team.name)).unwrap_or(k))
                    .collect();
                names.sort();
                println!("Enabled members: {}", names.join(", "));
            }
        }
        DaemonStatusInfo::NotRunning { reason } => {
            println!("Daemon: {}", reason);
        }
    }

    println!();
    println!("Enabled = daemon will auto-start this member when GitHub activity is detected (poll/webhook).");
    println!("Change with `bm enable <member>` / `bm disable <member>`.");

    Ok(())
}

/// Handles the hidden `bm daemon-run` command.
pub fn run_daemon(team: &str, mode: &str, port: u16, interval: u64, bind: &str) -> Result<()> {
    daemon::run_daemon(team, mode, port, interval, bind)
}

/// Formats an ISO 8601 timestamp for display.
fn format_timestamp(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        ts.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn format_timestamp_rfc3339() {
        let result = format_timestamp("2026-02-21T10:30:00+00:00");
        assert_eq!(result, "2026-02-21 10:30:00 UTC");
    }

    #[test]
    fn format_timestamp_unparseable() {
        let result = format_timestamp("not-a-timestamp");
        assert_eq!(result, "not-a-timestamp");
    }
}
