use std::fs;
use std::process::Command;

use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table};
use serde::Deserialize;

use crate::commands::daemon;
use crate::commands::start::{resolve_member_status, MemberStatus};
use crate::config;
use crate::state;
use crate::topology;

/// Minimal member manifest for reading role.
#[derive(Debug, Deserialize)]
struct MemberManifest {
    #[serde(default)]
    role: Option<String>,
}

/// Handles `bm status [-t team] [-v]`.
pub fn run(team_flag: Option<&str>, verbose: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");
    let team_name = &team.name;

    // Check for topology file (formation info)
    let topo_path = topology::topology_path(&cfg.workzone, team_name);
    let topo = topology::load(&topo_path)?;

    println!("Team: {}", team_name);
    if let Some(ref t) = topo {
        println!("Formation: {}", t.formation);
    }
    println!("Profile: {}", team.profile);

    // Show daemon status if running
    if let Ok(pid_file) = daemon::pid_path(team_name) {
        if pid_file.exists() {
            if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                if let Ok(pid) = pid_str.trim().parse::<u32>() {
                    if state::is_alive(pid) {
                        if let Ok(cfg_file) = daemon::config_path(team_name) {
                            if let Ok(contents) = fs::read_to_string(&cfg_file) {
                                if let Ok(dcfg) =
                                    serde_json::from_str::<daemon::DaemonConfig>(&contents)
                                {
                                    match dcfg.mode.as_str() {
                                        "webhook" => println!(
                                            "Daemon: running (PID {}, webhook mode, port {})",
                                            pid, dcfg.port
                                        ),
                                        "poll" => println!(
                                            "Daemon: running (PID {}, poll mode, interval {}s)",
                                            pid, dcfg.interval_secs
                                        ),
                                        _ => println!("Daemon: running (PID {})", pid),
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }
    }
    println!();

    // Read members
    let members_dir = team_repo.join("team");
    if !members_dir.is_dir() {
        println!("No members hired yet.");
        return Ok(());
    }

    let mut member_dirs: Vec<String> = Vec::new();
    for entry in fs::read_dir(&members_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if !name.starts_with('.') {
            member_dirs.push(name);
        }
    }
    member_dirs.sort();

    if member_dirs.is_empty() {
        println!("No members hired yet.");
        return Ok(());
    }

    let mut runtime_state = state::load()?;

    // Build table
    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Member", "Role", "Status", "Started", "PID"]);

    let mut crashed_keys: Vec<String> = Vec::new();

    for member_dir_name in &member_dirs {
        let role = read_member_role(&members_dir, member_dir_name);
        let status = resolve_member_status(&runtime_state, team_name, member_dir_name);

        let (status_label, started, pid_str) = match &status {
            MemberStatus::Running { pid, started_at } => {
                ("running", format_timestamp(started_at), pid.to_string())
            }
            MemberStatus::Crashed { pid, started_at } => {
                crashed_keys.push(format!("{}/{}", team_name, member_dir_name));
                ("crashed", format_timestamp(started_at), pid.to_string())
            }
            MemberStatus::Stopped => ("stopped", "—".to_string(), "—".to_string()),
        };

        table.add_row(vec![
            member_dir_name.as_str(),
            &role,
            status_label,
            &started,
            &pid_str,
        ]);
    }

    println!("{table}");

    // Clean up crashed entries
    if !crashed_keys.is_empty() {
        for key in &crashed_keys {
            runtime_state.members.remove(key);
        }
        state::save(&runtime_state)?;
    }

    // Verbose mode: show Ralph runtime details for running members
    if verbose {
        let runtime_state = state::load()?; // reload after cleanup
        let team_prefix = format!("{}/", team_name);

        for (key, rt) in &runtime_state.members {
            if !key.starts_with(&team_prefix) {
                continue;
            }
            if !state::is_alive(rt.pid) {
                continue;
            }

            let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);
            println!("\n── {} (PID {}) ──", member_name, rt.pid);

            // Run Ralph CLI commands from the workspace, skipping unavailable ones
            for (label, args) in &[
                ("Hats", vec!["hats"]),
                ("Loops", vec!["loops", "list"]),
                ("Events", vec!["events"]),
                ("Bot", vec!["bot", "status"]),
            ] {
                match run_ralph_cmd(&rt.workspace, args) {
                    Ok(output) => {
                        println!("\n  {}:", label);
                        for line in output.lines() {
                            println!("    {}", line);
                        }
                    }
                    Err(_) => {
                        // Skip unavailable commands gracefully
                    }
                }
            }
        }
    }

    Ok(())
}

/// Reads the role from a member's botminter.yml, falling back to dir-name inference.
fn read_member_role(members_dir: &std::path::Path, member_dir_name: &str) -> String {
    let manifest_path = members_dir.join(member_dir_name).join("botminter.yml");
    if let Ok(contents) = fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_yml::from_str::<MemberManifest>(&contents) {
            if let Some(role) = manifest.role {
                return role;
            }
        }
    }
    // Infer from dir name
    member_dir_name
        .split('-')
        .next()
        .unwrap_or("unknown")
        .to_string()
}

/// Runs a ralph command in the given workspace and returns stdout.
fn run_ralph_cmd(workspace: &std::path::Path, args: &[&str]) -> Result<String> {
    let output = Command::new("ralph")
        .args(args)
        .current_dir(workspace)
        .output()
        .with_context(|| format!("Failed to run ralph {}", args.join(" ")))?;

    if !output.status.success() {
        anyhow::bail!("ralph {} failed", args.join(" "));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Formats an ISO 8601 timestamp for display, stripping sub-seconds.
fn format_timestamp(ts: &str) -> String {
    // Try to parse and reformat for display
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S").to_string()
    } else {
        ts.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── read_member_role ──────────────────────────────────────────

    #[test]
    fn read_member_role_from_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("architect-alice");
        fs::create_dir(&member_dir).unwrap();
        fs::write(
            member_dir.join("botminter.yml"),
            "role: architect\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "architect-alice");
        assert_eq!(role, "architect");
    }

    #[test]
    fn read_member_role_yaml_with_extra_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("po-bob");
        fs::create_dir(&member_dir).unwrap();
        fs::write(
            member_dir.join("botminter.yml"),
            "role: product-owner\nschema_version: '0.3'\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "po-bob");
        assert_eq!(role, "product-owner");
    }

    #[test]
    fn read_member_role_fallback_no_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        // Dir exists but no botminter.yml
        fs::create_dir(tmp.path().join("architect-alice")).unwrap();

        let role = read_member_role(tmp.path(), "architect-alice");
        assert_eq!(role, "architect");
    }

    #[test]
    fn read_member_role_fallback_no_role_field() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("po-bob");
        fs::create_dir(&member_dir).unwrap();
        // YAML exists but has no 'role' field
        fs::write(
            member_dir.join("botminter.yml"),
            "schema_version: '0.3'\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "po-bob");
        assert_eq!(role, "po");
    }

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
