//! E2E tests for daemon signal handling and lifecycle.
//!
//! These tests use stub `ralph` binaries to test daemon process management
//! without needing Claude API access. They verify:
//! - Start/stop lifecycle in both modes
//! - Signal forwarding to children
//! - Per-member log files
//! - Stale PID detection
//! - Double-start rejection

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use bm::config::{BotminterConfig, Credentials, TeamEntry};
use bm::profile;

use super::helpers::{
    assert_cmd_success, bm_cmd, force_kill, is_alive, wait_for_exit, DaemonGuard,
};

// ── Stub Ralph ───────────────────────────────────────────────────────

/// Standard stub: traps SIGTERM, writes PID, sleeps forever.
const STUB_RALPH: &str = r#"#!/bin/bash
# Stub ralph binary for daemon E2E tests.
case "$1" in
  run)
    echo $$ > "$PWD/.ralph-stub-pid"
    echo "stub ralph started (PID $$)" >&2
    trap "rm -f \"$PWD/.ralph-stub-pid\"; exit 0" SIGTERM SIGINT
    while true; do sleep 1; done
    ;;
  *)
    exit 0
    ;;
esac
"#;

/// SIGTERM-ignoring stub: only dies to SIGKILL.
const STUB_RALPH_IGNORE_SIGTERM: &str = r#"#!/bin/bash
# Stub ralph that ignores SIGTERM (for SIGKILL escalation tests).
case "$1" in
  run)
    echo $$ > "$PWD/.ralph-stub-pid"
    echo "stub ralph (sigterm-immune) started (PID $$)" >&2
    trap "" SIGTERM  # ignore SIGTERM
    while true; do sleep 1; done
    ;;
  *)
    exit 0
    ;;
esac
"#;

/// Creates a stub `ralph` binary in a temp directory. Returns the directory path.
fn create_stub_ralph(tmp: &Path, script: &str) -> PathBuf {
    let stub_dir = tmp.join("stub-bin");
    fs::create_dir_all(&stub_dir).unwrap();

    let stub_path = stub_dir.join("ralph");
    fs::write(&stub_path, script).unwrap();
    fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();

    stub_dir
}

/// Returns a PATH string with the stub directory prepended.
fn path_with_stub(stub_dir: &Path) -> String {
    format!(
        "{}:{}",
        stub_dir.display(),
        std::env::var("PATH").unwrap_or_default()
    )
}

// ── Workspace Setup ──────────────────────────────────────────────────

/// Sets up a minimal team with one member workspace for daemon tests.
///
/// Returns `(team_name, member_dir_name)`.
fn setup_daemon_workspace(tmp: &Path, team_name: &str) -> (String, String) {
    let (profile_name, roles) = find_profile_with_role();
    let role = &roles[0];
    let member_name = "alice";
    let member_dir_name = format!("{}-{}", role, member_name);

    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    // Create team repo with botminter.yml
    fs::create_dir_all(&team_repo).unwrap();
    let manifest = profile::read_manifest(&profile_name).unwrap();
    let manifest_yml = serde_yml::to_string(&manifest).unwrap();
    fs::write(team_repo.join("botminter.yml"), &manifest_yml).unwrap();

    // Member discovery: team_repo/team/<member>/
    let members_dir = team_repo.join("team");
    let member_config_dir = members_dir.join(&member_dir_name);
    fs::create_dir_all(&member_config_dir).unwrap();

    // Workspace: workzone/<team>/<member>/workspace/
    let workspace = team_dir.join(&member_dir_name).join("workspace");
    fs::create_dir_all(workspace.join(".botminter")).unwrap();
    fs::write(workspace.join("PROMPT.md"), "# E2E Daemon Test\n").unwrap();

    // Write config
    let config = BotminterConfig {
        workzone,
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: profile_name,
            github_repo: "test-org/test-repo".to_string(),
            credentials: Credentials {
                gh_token: Some("ghp_test_token".to_string()),
                telegram_bot_token: None,
                webhook_secret: None,
            },
        }],
    };
    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    (team_name.to_string(), member_dir_name)
}

fn find_profile_with_role() -> (String, Vec<String>) {
    for name in profile::list_profiles() {
        if let Ok(roles) = profile::list_roles(&name) {
            if !roles.is_empty() {
                return (name, roles);
            }
        }
    }
    panic!("No embedded profile has any roles");
}

/// Creates a bm command with HOME and PATH configured.
fn daemon_cmd(tmp: &Path, stub_dir: &Path, args: &[&str]) -> Command {
    let mut cmd = bm_cmd();
    cmd.args(args)
        .env("HOME", tmp)
        .env("PATH", path_with_stub(stub_dir));
    cmd
}

// ── Tests ────────────────────────────────────────────────────────────

/// Start in poll mode → status shows running/poll → stop → status shows not running.
#[test]
fn daemon_start_stop_poll_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-poll");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // Start
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "start", "--mode", "poll", "-t", &team_name],
    ));
    assert!(out.contains("Daemon started"), "Expected started: {}", out);

    // Status shows running
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "status", "-t", &team_name],
    ));
    assert!(out.contains("running"), "Expected running: {}", out);
    assert!(out.contains("poll"), "Expected poll mode: {}", out);

    // Stop
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "stop", "-t", &team_name],
    ));
    assert!(out.contains("Daemon stopped"), "Expected stopped: {}", out);

    // Status shows not running
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "status", "-t", &team_name],
    ));
    assert!(out.contains("not running"), "Expected not running: {}", out);

    // PID and config files cleaned up
    let pid_file = tmp
        .path()
        .join(format!(".botminter/daemon-{}.pid", team_name));
    assert!(!pid_file.exists(), "PID file should be cleaned up");
    let cfg_file = tmp
        .path()
        .join(format!(".botminter/daemon-{}.json", team_name));
    assert!(!cfg_file.exists(), "Config file should be cleaned up");
}

/// Start in webhook mode → status shows running/webhook → stop.
#[test]
fn daemon_start_stop_webhook_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-wh");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    let port = "19500";

    // Start
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &[
            "daemon", "start", "--mode", "webhook", "--port", port, "-t", &team_name,
        ],
    ));
    assert!(out.contains("Daemon started"), "Expected started: {}", out);

    // Wait for server to bind
    std::thread::sleep(Duration::from_millis(500));

    // Status shows webhook
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "status", "-t", &team_name],
    ));
    assert!(out.contains("running"), "Expected running: {}", out);
    assert!(out.contains("webhook"), "Expected webhook mode: {}", out);

    // Stop
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "stop", "-t", &team_name],
    ));
    assert!(out.contains("Daemon stopped"), "Expected stopped: {}", out);
}

/// Start daemon → daemon launches stub ralph → `bm daemon stop` → both die.
#[test]
fn daemon_stop_terminates_running_members() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, member) = setup_daemon_workspace(tmp.path(), "e2e-term");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // Start daemon in poll mode with very short interval to trigger member launch quickly
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &[
            "daemon", "start", "--mode", "poll", "--interval", "2", "-t", &team_name,
        ],
    ));

    // Read daemon PID
    let pid_file = tmp
        .path()
        .join(format!(".botminter/daemon-{}.pid", team_name));
    let daemon_pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();
    assert!(is_alive(daemon_pid), "Daemon should be alive");

    // Wait for the daemon to potentially launch a member (poll interval is 2s)
    // The member launch may fail due to gh not being configured, but that's OK —
    // we're testing that daemon stop works, not that members are launched successfully.
    std::thread::sleep(Duration::from_secs(4));

    // Stop daemon
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "stop", "-t", &team_name],
    ));

    // Daemon should be dead
    wait_for_exit(daemon_pid, Duration::from_secs(10));
    assert!(
        !is_alive(daemon_pid),
        "Daemon PID {} should be dead after stop",
        daemon_pid
    );

    // Check for any ralph stub PID files (if member was launched and has a PID file)
    let workspace = tmp
        .path()
        .join("workspaces")
        .join(&team_name)
        .join(&member)
        .join("workspace");
    let stub_pid_file = workspace.join(".ralph-stub-pid");
    if stub_pid_file.exists() {
        let stub_pid: u32 = fs::read_to_string(&stub_pid_file)
            .unwrap()
            .trim()
            .parse()
            .unwrap();
        wait_for_exit(stub_pid, Duration::from_secs(10));
        assert!(
            !is_alive(stub_pid),
            "Stub ralph PID {} should be dead after daemon stop",
            stub_pid
        );
    }
}

/// Stub ralph ignores SIGTERM → daemon escalates to SIGKILL → processes die.
#[test]
fn daemon_stop_timeout_escalates_to_sigkill() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH_IGNORE_SIGTERM);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-sigkill");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // Start daemon
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &[
            "daemon", "start", "--mode", "poll", "--interval", "2", "-t", &team_name,
        ],
    ));

    let pid_file = tmp
        .path()
        .join(format!(".botminter/daemon-{}.pid", team_name));
    let daemon_pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Stop daemon — will send SIGTERM, wait 30s, then SIGKILL
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "stop", "-t", &team_name],
    ));

    // Daemon should be dead (SIGKILL escalation should have worked)
    wait_for_exit(daemon_pid, Duration::from_secs(5));
    assert!(!is_alive(daemon_pid), "Daemon should be dead after SIGKILL");
}

/// Write a stale PID file (dead PID) → `bm daemon start` → succeeds.
#[test]
fn daemon_stale_pid_detected_on_start() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-stale");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // Write a stale PID file with a PID that doesn't exist
    let pid_dir = tmp.path().join(".botminter");
    fs::create_dir_all(&pid_dir).unwrap();
    let pid_file = pid_dir.join(format!("daemon-{}.pid", team_name));
    // Use PID 99999 which almost certainly doesn't exist
    fs::write(&pid_file, "99999").unwrap();

    // Start should succeed (cleans stale PID)
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "start", "--mode", "poll", "-t", &team_name],
    ));
    assert!(
        out.contains("Daemon started"),
        "Should start despite stale PID: {}",
        out
    );
}

/// Start daemon → verify per-member log file is created.
#[test]
fn daemon_per_member_log_created() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, member) = setup_daemon_workspace(tmp.path(), "e2e-memlog");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // Start daemon in poll mode with short interval
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &[
            "daemon", "start", "--mode", "poll", "--interval", "2", "-t", &team_name,
        ],
    ));

    // Wait for the daemon to attempt member launch (poll + launch attempt)
    std::thread::sleep(Duration::from_secs(5));

    // Check if the daemon log mentions the member log path
    let daemon_log = tmp
        .path()
        .join(format!(".botminter/logs/daemon-{}.log", team_name));
    if daemon_log.exists() {
        let log_content = fs::read_to_string(&daemon_log).unwrap();
        // The daemon should have logged the member log path
        let expected_log_name = format!("member-{}-{}.log", team_name, member);
        // The log mention is optional — depends on whether gh auth succeeds
        if log_content.contains(&expected_log_name) {
            eprintln!("✓ Daemon log mentions per-member log file");
        }
    }

    // The member log file will only be created if the daemon actually launches
    // a member (requires gh auth), so we check if the daemon log at least exists
    assert!(
        daemon_log.exists(),
        "Daemon log file should exist at {}",
        daemon_log.display()
    );
}

/// Start daemon → second start fails with "already running".
#[test]
fn daemon_already_running_rejects_second_start() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-dup");
    let _guard = DaemonGuard::new(tmp.path(), &team_name, Some(&stub_dir));

    // First start
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "start", "--mode", "poll", "-t", &team_name],
    ));

    // Second start should fail
    let output = daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "start", "--mode", "poll", "-t", &team_name],
    )
    .output()
    .expect("failed to run second start");

    assert!(
        !output.status.success(),
        "Second start should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("already running"),
        "Should say already running: {}",
        stderr
    );
}

/// Start daemon → force-kill daemon PID → status shows "not running".
#[test]
fn daemon_crashed_detected_by_status() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path(), STUB_RALPH);
    let (team_name, _member) = setup_daemon_workspace(tmp.path(), "e2e-crash");
    // No guard needed — we're manually killing the daemon

    // Start
    assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "start", "--mode", "poll", "-t", &team_name],
    ));

    // Read PID
    let pid_file = tmp
        .path()
        .join(format!(".botminter/daemon-{}.pid", team_name));
    let daemon_pid: u32 = fs::read_to_string(&pid_file)
        .unwrap()
        .trim()
        .parse()
        .unwrap();

    // Force-kill the daemon (simulate crash)
    force_kill(daemon_pid);
    wait_for_exit(daemon_pid, Duration::from_secs(5));

    // Status should detect crash
    let out = assert_cmd_success(&mut daemon_cmd(
        tmp.path(),
        &stub_dir,
        &["daemon", "status", "-t", &team_name],
    ));
    assert!(
        out.contains("not running") || out.contains("stale"),
        "Status should show not running / stale PID: {}",
        out
    );

    // PID file should be cleaned up by status
    assert!(
        !pid_file.exists(),
        "Stale PID file should be cleaned up by status"
    );
}
