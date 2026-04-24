use anyhow::Result;
use comfy_table::{
    ContentArrangement, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table,
};

use crate::config;
use crate::state::{self, MemberStatus};

/// Handles `bm status [-t team] [-v]`.
pub fn run(team_flag: Option<&str>, verbose: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let info = state::gather_status(team, &cfg, verbose)?;

    // Header
    println!("Team: {}", team.name);
    if let Some(f) = &info.formation {
        println!("Formation: {}", f);
    }
    println!("Profile: {}", team.profile);
    if !team.github_repo.is_empty() {
        println!("GitHub: {}", team.github_repo);
    }
    if !info.project_names.is_empty() {
        println!("Projects: {}", info.project_names.join(", "));
    }
    if let Some(d) = &info.daemon {
        match d.mode.as_str() {
            "webhook" => println!(
                "Daemon: running (PID {}, webhook mode, port {})",
                d.pid, d.port
            ),
            "poll" => println!(
                "Daemon: running (PID {}, poll mode, interval {}s)",
                d.pid, d.interval_secs
            ),
            _ => println!("Daemon: running (PID {})", d.pid),
        }
        if d.port > 0 {
            println!("Console: http://localhost:{}", d.port);
        }
    }
    println!();

    // Members
    if !info.has_members {
        println!("No members hired yet.");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::DynamicFullWidth)
        .set_header(vec!["Member", "Role", "Status", "Enabled", "Branch", "Started", "PID"]);

    for m in &info.members {
        let (label, started, pid_str) = match &m.status {
            MemberStatus::Running { pid, started_at, brain_mode } => {
                let status = if *brain_mode { "brain" } else { "running" };
                (status, format_timestamp(started_at), pid.to_string())
            }
            MemberStatus::Crashed { pid, started_at } => {
                ("crashed", format_timestamp(started_at), pid.to_string())
            }
            MemberStatus::Stopped => ("stopped", "—".to_string(), "—".to_string()),
        };
        let enabled = if m.enabled { "yes" } else { "-" };
        table.add_row(vec![
            m.name.as_str(),
            &m.role,
            label,
            enabled,
            &m.branch,
            &started,
            &pid_str,
        ]);
    }
    println!("{table}");
    println!();
    println!("Enabled = daemon will auto-start this member when GitHub activity is detected (poll/webhook).");
    println!("Change with `bm enable <member>` / `bm disable <member>`.");

    // Bridge
    if let Some(b) = &info.bridge {
        println!();
        println!("Bridge: {} ({})", b.name, b.bridge_type);
        println!("Status: {}", b.status);
        if let Some(url) = &b.url {
            println!("URL: {}", url);
        }
        if !b.identities.is_empty() {
            println!();
            let mut bt = Table::new();
            bt.load_preset(UTF8_FULL_CONDENSED)
                .apply_modifier(UTF8_ROUND_CORNERS)
                .set_content_arrangement(ContentArrangement::DynamicFullWidth)
                .set_header(vec!["Member", "Bridge User", "User ID"]);
            for id in &b.identities {
                bt.add_row(vec![&id.member, &id.bridge_user, &id.user_id]);
            }
            println!("{bt}");
        }
    }

    // Verbose
    if let Some(v) = &info.verbose {
        for ws in &v.workspaces {
            println!("\n── {} workspace ──", ws.member);
            println!("  Submodules:");
            for s in &ws.submodules {
                println!("    {}: {}", s.name, s.status_label);
            }
        }
        for ri in &v.ralph_sections {
            println!("\n── {} (PID {}) ──", ri.member, ri.pid);
            for (label, output) in &ri.sections {
                println!("\n  {}:", label);
                for line in output.lines() {
                    println!("    {}", line);
                }
            }
        }
    }

    Ok(())
}

/// Formats an ISO 8601 timestamp for display, stripping sub-seconds.
fn format_timestamp(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        ts.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── format_timestamp ──────────────────────────────────────────

    #[test]
    fn format_timestamp_rfc3339() {
        let result = format_timestamp("2026-02-21T10:30:00+00:00");
        assert_eq!(result, "2026-02-21 10:30:00");
    }

    #[test]
    fn format_timestamp_with_offset() {
        let result = format_timestamp("2026-02-21T12:30:00+02:00");
        assert_eq!(result, "2026-02-21 12:30:00");
    }

    #[test]
    fn format_timestamp_unparseable_passthrough() {
        let result = format_timestamp("not-a-timestamp");
        assert_eq!(result, "not-a-timestamp");
    }

    #[test]
    fn format_timestamp_empty_passthrough() {
        let result = format_timestamp("");
        assert_eq!(result, "");
    }
}
