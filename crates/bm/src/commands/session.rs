use anyhow::{bail, Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, ContentArrangement, Table};
use serde::Serialize;

use crate::cli::SessionCommand;
use crate::config;
use crate::daemon::DaemonClient;
use crate::daemon::sessions_api::{BulkCleanupRequest, SessionHistoryInfo, SessionInfo};

pub fn run(command: SessionCommand) -> Result<()> {
    match command {
        SessionCommand::Inspect { session_id, team } => run_inspect(&session_id, team.as_deref()),
        SessionCommand::Cleanup {
            session_id,
            all,
            member,
            older_than,
            team,
        } => run_cleanup(session_id.as_deref(), all, member.as_deref(), older_than.as_deref(), team.as_deref()),
        SessionCommand::List { team, json } => run_list(team.as_deref(), json),
        SessionCommand::Finalize { session_id, team } => run_finalize(&session_id, team.as_deref()),
    }
}

fn run_list(team_flag: Option<&str>, json: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let rows = collect_session_rows(&team.name);
    if json {
        println!("{}", serde_json::to_string_pretty(&rows)?);
    } else if rows.is_empty() {
        println!("Sessions: none");
    } else {
        println!("{}", render_session_table(&rows));
    }
    Ok(())
}

fn run_finalize(session_id: &str, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let client = DaemonClient::connect(&team.name)?;
    let resp = client.retrigger_finalization(session_id)?;
    if !resp.ok {
        bail!("finalize failed: {}", resp.error.unwrap_or_default());
    }
    println!("Finalization triggered for session {session_id}");
    Ok(())
}

#[derive(Debug, Serialize)]
struct SessionRow {
    session_id: String,
    member: String,
    #[serde(rename = "type")]
    session_type: String,
    state: String,
    finalization_status: String,
    started_at: String,
    ended_at: String,
}

impl SessionRow {
    fn from_active(info: &SessionInfo) -> Self {
        Self {
            session_id: truncate_id(&info.session_id),
            member: info.member_name.clone(),
            session_type: info.session_type.clone(),
            state: info.current_state.clone(),
            finalization_status: if info.current_state == "Retained" {
                "pending".to_string()
            } else {
                String::new()
            },
            started_at: format_ts(&info.started_at),
            ended_at: String::new(),
        }
    }

    fn from_terminal(info: &SessionHistoryInfo) -> Self {
        Self {
            session_id: truncate_id(&info.session_id),
            member: info.member_name.clone(),
            session_type: info.session_type.clone(),
            state: "terminal".to_string(),
            finalization_status: info.finalization_status.clone(),
            started_at: format_ts(&info.start_time),
            ended_at: format_ts(&info.end_time),
        }
    }
}

fn collect_session_rows(team_name: &str) -> Vec<SessionRow> {
    let Ok(client) = DaemonClient::connect(team_name) else {
        return Vec::new();
    };
    let mut rows = Vec::new();
    if let Ok(resp) = client.list_sessions() {
        rows.extend(resp.sessions.iter().map(SessionRow::from_active));
    }
    if let Ok(resp) = client.list_session_history() {
        rows.extend(resp.sessions.iter().map(SessionRow::from_terminal));
    }
    rows
}

fn render_session_table(rows: &[SessionRow]) -> String {
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::DynamicFullWidth)
        .set_header(vec![
            "Session ID",
            "Member",
            "Type",
            "State",
            "Fin. Status",
            "Started",
            "Ended",
        ]);
    for r in rows {
        let fin = if r.finalization_status.is_empty() { "—" } else { r.finalization_status.as_str() };
        let ended = if r.ended_at.is_empty() { "—" } else { r.ended_at.as_str() };
        table.add_row(vec![
            r.session_id.as_str(),
            r.member.as_str(),
            r.session_type.as_str(),
            r.state.as_str(),
            fin,
            r.started_at.as_str(),
            ended,
        ]);
    }
    table.to_string()
}

fn truncate_id(id: &str) -> String {
    if id.len() > 8 {
        format!("{}\u{2026}", &id[..8])
    } else {
        id.to_string()
    }
}

fn format_ts(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        ts.to_string()
    }
}

fn run_inspect(session_id: &str, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let client = DaemonClient::connect(&team.name)?;
    let resp = client.inspect_session(session_id)?;
    if !resp.ok {
        bail!("inspect failed: {}", resp.error.unwrap_or_default());
    }
    println!("Session ID:   {}", resp.session_id.as_deref().unwrap_or("—"));
    println!("Member:       {}", resp.member_name.as_deref().unwrap_or("—"));
    println!("Type:         {}", resp.session_type.as_deref().unwrap_or("—"));
    println!("State:        {}", resp.current_state.as_deref().unwrap_or("—"));
    println!("Workspace:    {}", resp.workspace_path.as_deref().unwrap_or("—"));
    if let Some(fr) = resp.finalization_results {
        let pretty = serde_json::to_string_pretty(&fr).unwrap_or_else(|_| fr.to_string());
        println!("Finalization:\n{pretty}");
    }
    if let Some(gs) = resp.git_state {
        let pretty = serde_json::to_string_pretty(&gs).unwrap_or_else(|_| gs.to_string());
        println!("Git State:\n{pretty}");
    }
    Ok(())
}

fn run_cleanup(
    session_id: Option<&str>,
    all: bool,
    member: Option<&str>,
    older_than: Option<&str>,
    team_flag: Option<&str>,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let client = DaemonClient::connect(&team.name)?;

    if let Some(id) = session_id {
        let resp = client.cleanup_session(id)?;
        if !resp.ok {
            bail!("cleanup failed: {}", resp.error.unwrap_or_default());
        }
        let sid = resp.session_id.as_deref().unwrap_or(id);
        println!("Cleaned session {sid}");
        println!("  workspace removed: {}", resp.workspace_removed);
        println!("  registry removed:  {}", resp.registry_removed);
    } else if all {
        let req = BulkCleanupRequest { filter: "all".to_string(), value: None };
        let resp = client.bulk_cleanup_sessions(&req)?;
        if !resp.ok {
            bail!("bulk cleanup failed: {}", resp.error.unwrap_or_default());
        }
        println!("Cleaned {} session(s).", resp.cleaned);
    } else if let Some(name) = member {
        let req = BulkCleanupRequest { filter: "member".to_string(), value: Some(name.to_string()) };
        let resp = client.bulk_cleanup_sessions(&req)?;
        if !resp.ok {
            bail!("bulk cleanup failed: {}", resp.error.unwrap_or_default());
        }
        println!("Cleaned {} session(s).", resp.cleaned);
    } else if let Some(duration_str) = older_than {
        let secs = parse_duration_secs(duration_str)?;
        let req = BulkCleanupRequest {
            filter: "older_than".to_string(),
            value: Some(secs.to_string()),
        };
        let resp = client.bulk_cleanup_sessions(&req)?;
        if !resp.ok {
            bail!("bulk cleanup failed: {}", resp.error.unwrap_or_default());
        }
        println!("Cleaned {} session(s).", resp.cleaned);
    } else {
        bail!("specify a session ID, --all, --member <name>, or --older-than <duration>");
    }

    Ok(())
}

fn parse_duration_secs(s: &str) -> Result<u64> {
    let s = s.trim();
    if s.is_empty() {
        bail!("empty duration string");
    }
    if s.len() < 2 {
        bail!("invalid duration '{}': must be a number followed by a unit (s, m, h, d)", s);
    }
    let (num_part, unit) = s.split_at(s.len() - 1);
    let n: u64 = num_part
        .trim()
        .parse()
        .with_context(|| format!("invalid duration '{}': number part must be an integer", s))?;
    match unit {
        "s" => Ok(n),
        "m" => Ok(n * 60),
        "h" => Ok(n * 3_600),
        "d" => Ok(n * 86_400),
        other => bail!("invalid duration unit '{}' in '{}': use s, m, h, or d", other, s),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daemon::sessions_api::SessionInfo;

    #[test]
    fn session_list_shows_pending_for_retained_sessions() {
        let info = SessionInfo {
            session_id: "test-id".to_string(),
            member_name: "alice".to_string(),
            session_type: "engineer".to_string(),
            current_state: "Retained".to_string(),
            started_at: "2026-06-07T00:00:00Z".to_string(),
            state_transitioned_at: None,
            concurrent_count: None,
        };
        let row = SessionRow::from_active(&info);
        assert_eq!(
            row.finalization_status, "pending",
            "Retained sessions must show 'pending' in finalization_status, got '{}'",
            row.finalization_status
        );
    }

    #[test]
    fn parse_duration_secs_hours() {
        assert_eq!(parse_duration_secs("48h").unwrap(), 48 * 3600);
    }

    #[test]
    fn parse_duration_secs_days() {
        assert_eq!(parse_duration_secs("7d").unwrap(), 7 * 86400);
    }

    #[test]
    fn parse_duration_secs_minutes() {
        assert_eq!(parse_duration_secs("30m").unwrap(), 30 * 60);
    }

    #[test]
    fn parse_duration_secs_seconds() {
        assert_eq!(parse_duration_secs("120s").unwrap(), 120);
    }

    #[test]
    fn parse_duration_secs_invalid_unit() {
        assert!(parse_duration_secs("5w").is_err());
    }

    #[test]
    fn parse_duration_secs_empty() {
        assert!(parse_duration_secs("").is_err());
    }

    #[test]
    fn parse_duration_secs_non_numeric() {
        assert!(parse_duration_secs("abch").is_err());
    }

    #[test]
    fn parse_duration_secs_single_char() {
        assert!(parse_duration_secs("h").is_err());
    }
}
