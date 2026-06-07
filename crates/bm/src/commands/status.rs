use anyhow::Result;
use comfy_table::{
    ContentArrangement, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table,
};

use serde::Serialize;

use crate::config;
use crate::daemon::DaemonClient;
use crate::daemon::sessions_api::SessionInfo;
use crate::state::{self, MemberStatus};

#[derive(Debug, Clone, Serialize)]
struct SessionDisplayRow {
    session_id: String,
    member: String,
    session_type: String,
    state: String,
    start_time: String,
    elapsed_time: String,
    concurrent_count: String,
}

impl SessionDisplayRow {
    fn fields(&self) -> [&str; 7] {
        [
            &self.session_id,
            &self.member,
            &self.session_type,
            &self.state,
            &self.start_time,
            &self.elapsed_time,
            &self.concurrent_count,
        ]
    }
}

/// Handles `bm status [-t team] [-v] [--json] [--history]`.
pub fn run(team_flag: Option<&str>, verbose: bool, json: bool, history: bool) -> Result<()> {
    if history {
        eprintln!("--history has been removed. Use 'bm session list' instead.");
        std::process::exit(1);
    }
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

    // Sessions
    let session_output = build_session_output(&team.name, json, |team_name| {
        DaemonClient::connect(team_name)
            .and_then(|c: DaemonClient| c.list_sessions())
            .ok()
            .map(|r| r.sessions)
    });
    println!();
    println!("{session_output}");

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

fn truncate_session_id(id: &str) -> String {
    if id.len() > 8 {
        format!("{}\u{2026}", &id[..8])
    } else {
        id.to_string()
    }
}

fn format_elapsed(timestamp: &str) -> String {
    let Ok(dt) = chrono::DateTime::parse_from_rfc3339(timestamp) else {
        return String::new();
    };
    let elapsed = chrono::Utc::now().signed_duration_since(dt);
    if elapsed.num_seconds() < 0 {
        return String::new();
    }
    let total_hours = elapsed.num_hours();
    let minutes = (elapsed.num_minutes() % 60) as u64;
    if total_hours >= 24 {
        let days = total_hours / 24;
        let hours = (total_hours % 24) as u64;
        format!("{days}d {hours}h")
    } else {
        format!("{total_hours}h {minutes}m")
    }
}

fn session_info_to_display_row(info: &SessionInfo) -> SessionDisplayRow {
    let elapsed_time = info
        .state_transitioned_at
        .as_deref()
        .map(format_elapsed)
        .unwrap_or_default();
    SessionDisplayRow {
        session_id: truncate_session_id(&info.session_id),
        member: info.member_name.clone(),
        session_type: info.session_type.clone(),
        state: info.current_state.clone(),
        start_time: format_timestamp(&info.started_at),
        elapsed_time,
        concurrent_count: "0".to_string(),
    }
}

fn render_sessions_section(sessions: Option<&[SessionDisplayRow]>) -> String {
    match sessions {
        None => "Sessions: none (daemon not running)".to_string(),
        Some([]) => "Sessions: none".to_string(),
        Some(rows) => {
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
            "Started",
            "Elapsed",
            "Concurrent",
        ]);
            for row in rows {
                table.add_row(row.fields());
            }
            table.to_string()
        }
    }
}

fn render_status_json(sessions: Option<&[SessionDisplayRow]>) -> serde_json::Value {
    let sessions_slice = sessions.unwrap_or(&[]);
    serde_json::json!({ "sessions": sessions_slice })
}

fn build_session_output(
    team_name: &str,
    json: bool,
    fetch_sessions: impl FnOnce(&str) -> Option<Vec<SessionInfo>>,
) -> String {
    let sessions = fetch_sessions(team_name);
    let rows = sessions.as_ref().map(|infos| {
        infos
            .iter()
            .map(|info| {
                let mut row = session_info_to_display_row(info);
                if let Some(count) = info.concurrent_count {
                    row.concurrent_count = count.to_string();
                }
                row
            })
            .collect::<Vec<_>>()
    });
    if json {
        let value = render_status_json(rows.as_deref());
        serde_json::to_string_pretty(&value).unwrap()
    } else {
        render_sessions_section(rows.as_deref())
    }
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

    // ── CT-05: bm status Session Display ──────────────────────────

    fn sample_session_info() -> SessionInfo {
        SessionInfo {
            session_id: "a1b2c3d4e5f6g7h8".to_string(),
            member_name: "alice".to_string(),
            session_type: "Interactive".to_string(),
            current_state: "Active".to_string(),
            started_at: "2026-06-03T10:00:00+00:00".to_string(),
            state_transitioned_at: None,
            concurrent_count: None,
        }
    }

    fn sample_display_row() -> SessionDisplayRow {
        SessionDisplayRow {
            session_id: "a1b2c3d4\u{2026}".to_string(),
            member: "alice".to_string(),
            session_type: "Interactive".to_string(),
            state: "Active".to_string(),
            start_time: "2026-06-03 10:00:00".to_string(),
            elapsed_time: String::new(),
            concurrent_count: "0".to_string(),
        }
    }

    // AC-1: Sessions appear in bm status — truncated IDs

    #[test]
    fn truncate_session_id_to_8_chars() {
        let result = truncate_session_id("a1b2c3d4e5f6g7h8");
        assert_eq!(result, "a1b2c3d4\u{2026}");
    }

    #[test]
    fn truncate_session_id_short_passthrough() {
        let result = truncate_session_id("abc");
        assert_eq!(result, "abc");
    }

    #[test]
    fn truncate_session_id_exact_8_no_ellipsis() {
        let result = truncate_session_id("a1b2c3d4");
        assert_eq!(result, "a1b2c3d4");
    }

    // AC-1: SessionInfo → SessionDisplayRow conversion

    #[test]
    fn session_info_to_display_row_truncates_id() {
        let info = sample_session_info();
        let row = session_info_to_display_row(&info);
        assert_eq!(row.session_id, "a1b2c3d4\u{2026}");
        assert_eq!(row.member, "alice");
        assert_eq!(row.session_type, "Interactive");
        assert_eq!(row.state, "Active");
    }

    #[test]
    fn session_info_to_display_row_formats_timestamp() {
        let info = SessionInfo {
            session_id: "abcdef01".to_string(),
            member_name: "bob".to_string(),
            session_type: "Loop".to_string(),
            current_state: "Active".to_string(),
            started_at: "2026-06-03T10:30:00+00:00".to_string(),
            state_transitioned_at: None,
            concurrent_count: None,
        };
        let row = session_info_to_display_row(&info);
        assert_eq!(row.start_time, "2026-06-03 10:30:00");
    }

    // AC-1: Sessions section renders table with active sessions

    #[test]
    fn sessions_section_with_active_sessions_shows_data() {
        let sessions = vec![sample_display_row()];
        let output = render_sessions_section(Some(&sessions));
        assert!(
            output.contains("a1b2c3d4"),
            "must show truncated session ID, got:\n{output}"
        );
        assert!(
            output.contains("alice"),
            "must show member name, got:\n{output}"
        );
        assert!(
            output.contains("Interactive"),
            "must show session type, got:\n{output}"
        );
        assert!(
            output.contains("Active"),
            "must show session state, got:\n{output}"
        );
    }

    // AC-2: Graceful when daemon not running

    #[test]
    fn sessions_section_daemon_not_running() {
        let output = render_sessions_section(None);
        assert!(
            output.contains("Sessions: none (daemon not running)"),
            "must show daemon offline message, got:\n{output}"
        );
    }

    // AC-3: JSON output flag

    #[test]
    fn status_json_includes_sessions_array() {
        let sessions = vec![sample_display_row()];
        let json = render_status_json(Some(&sessions));
        assert!(
            json["sessions"].is_array(),
            "JSON must include sessions array, got: {json}"
        );
        assert_eq!(json["sessions"].as_array().unwrap().len(), 1);
    }

    #[test]
    fn status_json_sessions_contain_all_fields() {
        let sessions = vec![sample_display_row()];
        let json = render_status_json(Some(&sessions));
        let entry = &json["sessions"][0];
        assert!(entry["session_id"].is_string(), "must have session_id");
        assert!(entry["member"].is_string(), "must have member");
        assert!(entry["session_type"].is_string(), "must have session_type");
        assert!(entry["state"].is_string(), "must have state");
        assert!(entry["start_time"].is_string(), "must have start_time");
    }

    #[test]
    fn status_json_daemon_offline_empty_sessions() {
        let json = render_status_json(None);
        assert!(
            json["sessions"].is_array(),
            "JSON must include sessions array even when daemon offline"
        );
        assert!(
            json["sessions"].as_array().unwrap().is_empty(),
            "sessions must be empty when daemon offline"
        );
    }

    // AC-4: Empty session list

    #[test]
    fn sessions_section_empty_list() {
        let output = render_sessions_section(Some(&[]));
        assert!(
            output.contains("Sessions: none"),
            "empty sessions must show 'Sessions: none', got:\n{output}"
        );
        assert!(
            !output.contains("daemon not running"),
            "empty sessions must NOT mention daemon offline, got:\n{output}"
        );
    }

    // ── CT-05 fix: wiring tests — session display must be called from run() ──

    fn mock_fetch_active(_team: &str) -> Option<Vec<SessionInfo>> {
        Some(vec![sample_session_info()])
    }

    fn mock_fetch_offline(_team: &str) -> Option<Vec<SessionInfo>> {
        None
    }

    #[test]
    fn build_session_output_text_shows_session_data() {
        let output = build_session_output("test-team", false, mock_fetch_active);
        assert!(
            output.contains("a1b2c3d4"),
            "text output must contain truncated session ID, got:\n{output}"
        );
        assert!(
            output.contains("alice"),
            "text output must contain member name, got:\n{output}"
        );
        assert!(
            output.contains("Interactive"),
            "text output must contain session type, got:\n{output}"
        );
        assert!(
            output.contains("Active"),
            "text output must contain session state, got:\n{output}"
        );
    }

    #[test]
    fn build_session_output_json_produces_valid_json() {
        let output = build_session_output("test-team", true, mock_fetch_active);
        let json: serde_json::Value = serde_json::from_str(&output)
            .expect("json mode must produce valid JSON");
        assert!(
            json["sessions"].is_array(),
            "JSON must have sessions array, got: {json}"
        );
        assert_eq!(
            json["sessions"].as_array().unwrap().len(),
            1,
            "JSON sessions array must contain one entry"
        );
    }

    #[test]
    fn build_session_output_offline_text_graceful() {
        let output = build_session_output("test-team", false, mock_fetch_offline);
        assert!(
            output.contains("daemon not running"),
            "offline text must mention daemon not running, got:\n{output}"
        );
    }

    #[test]
    fn build_session_output_offline_json_graceful() {
        let output = build_session_output("test-team", true, mock_fetch_offline);
        let json: serde_json::Value = serde_json::from_str(&output)
            .expect("json mode must produce valid JSON even when daemon offline");
        assert!(
            json["sessions"].is_array(),
            "JSON must have sessions array even when offline"
        );
        assert!(
            json["sessions"].as_array().unwrap().is_empty(),
            "sessions must be empty when daemon offline"
        );
    }

    // --- CT-89-01 QE re-entry: AC-10 fix ---

    #[test]
    fn session_info_to_display_row_computes_elapsed_time() {
        let two_hours_ago = (chrono::Utc::now() - chrono::Duration::hours(2)).to_rfc3339();
        let info = SessionInfo {
            session_id: "abc12345".to_string(),
            member_name: "alice".to_string(),
            session_type: "Interactive".to_string(),
            current_state: "Active".to_string(),
            started_at: two_hours_ago.clone(),
            state_transitioned_at: Some(two_hours_ago),
            concurrent_count: None,
        };
        let row = session_info_to_display_row(&info);
        assert!(
            !row.elapsed_time.is_empty(),
            "elapsed_time must not be empty for active session with state_transitioned_at set"
        );
    }

    #[test]
    fn build_session_output_shows_correct_concurrent_count() {
        let now = chrono::Utc::now().to_rfc3339();
        let started = now.clone();
        let output = build_session_output("test", true, |_| {
            Some(vec![
                SessionInfo {
                    session_id: "session-a".to_string(),
                    member_name: "alice".to_string(),
                    session_type: "Loop".to_string(),
                    current_state: "Active".to_string(),
                    started_at: started.clone(),
                    state_transitioned_at: Some(started.clone()),
                    concurrent_count: Some(2),
                },
                SessionInfo {
                    session_id: "session-b".to_string(),
                    member_name: "alice".to_string(),
                    session_type: "Brain".to_string(),
                    current_state: "Active".to_string(),
                    started_at: started.clone(),
                    state_transitioned_at: Some(started.clone()),
                    concurrent_count: Some(2),
                },
            ])
        });
        let json: serde_json::Value =
            serde_json::from_str(&output).expect("JSON output must be valid");
        let sessions = json["sessions"].as_array().unwrap();
        assert_eq!(sessions.len(), 2, "must have 2 sessions");
        for s in sessions {
            assert_eq!(
                s["concurrent_count"].as_str().unwrap(),
                "2",
                "concurrent_count must be 2 for alice with 2 active sessions, got: {}",
                s["concurrent_count"]
            );
        }
    }
}
