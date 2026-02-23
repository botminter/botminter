//! E2E tests for the start → status → stop lifecycle.
//!
//! These tests use a stub `ralph` binary (a bash script that sleeps) to test
//! `bm`'s process management without needing Claude API access.
//!
//! The stub responds to:
//! - `ralph run -p PROMPT.md` — writes PID, optionally calls tg-mock, sleeps
//! - `ralph loops stop` — kills the running stub via the PID file
//! - Other commands — exits 0
//!
//! `start.rs` and `status.rs` derive `team_repo = team.path.join("team")`
//! and discover members at `team_repo.join("team")`, matching `hire.rs`
//! and `members.rs`.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::Duration;

use bm::config::{BotminterConfig, Credentials, TeamEntry};
use bm::profile;

use super::helpers::{assert_cmd_fails, assert_cmd_success, bm_cmd, force_kill, is_alive, wait_for_exit};

// ── Stub Ralph ───────────────────────────────────────────────────────

/// Stub ralph script content.
///
/// This script mimics ralph's behavior for process management tests:
/// - `run`: sleeps forever (mimicking a running orchestrator)
/// - `loops stop`: signals the running stub to exit
const STUB_RALPH: &str = r#"#!/bin/bash
# Stub ralph binary for E2E testing.
# Mimics ralph CLI behavior for bm start/stop tests.

case "$1" in
  run)
    # Write PID for coordination with 'loops stop'
    echo $$ > "$PWD/.ralph-stub-pid"

    # If Telegram env vars are set, make a getUpdates call
    if [ -n "$RALPH_TELEGRAM_API_URL" ] && [ -n "$RALPH_TELEGRAM_BOT_TOKEN" ]; then
      curl -s "${RALPH_TELEGRAM_API_URL}/bot${RALPH_TELEGRAM_BOT_TOKEN}/getUpdates" \
        > "$PWD/.ralph-stub-tg-response" 2>&1
    fi

    # Write received env vars for verification
    env | grep -E '^(RALPH_|GH_TOKEN)' | sort > "$PWD/.ralph-stub-env"

    # Trap SIGTERM for graceful shutdown
    trap "rm -f \"$PWD/.ralph-stub-pid\"; exit 0" SIGTERM SIGINT

    # Stay alive (bm start checks after 2s)
    while true; do
      sleep 1
    done
    ;;
  loops)
    if [ "$2" = "stop" ]; then
      pid_file="$PWD/.ralph-stub-pid"
      if [ -f "$pid_file" ]; then
        kill "$(cat "$pid_file")" 2>/dev/null
        rm -f "$pid_file"
      fi
      exit 0
    fi
    ;;
  *)
    exit 0
    ;;
esac
"#;

/// Creates a stub `ralph` binary in a temp directory. Returns the directory path
/// (to be prepended to PATH).
fn create_stub_ralph(tmp: &Path) -> PathBuf {
    let stub_dir = tmp.join("stub-bin");
    fs::create_dir_all(&stub_dir).unwrap();

    let stub_path = stub_dir.join("ralph");
    fs::write(&stub_path, STUB_RALPH).unwrap();
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

/// Sets up a minimal workspace for `bm start` / `bm status` / `bm stop`.
///
/// Creates the structure that `start.rs` expects:
/// - `team.path/team/botminter.yml` — schema version check (team repo root)
/// - `team.path/team/team/<member>/` — member discovery
/// - `workzone/<team>/<member>/workspace/.botminter/` — workspace marker
/// - `workzone/<team>/<member>/workspace/PROMPT.md` — required by ralph
///
/// Returns `(team_name, member_dir_name, workspace_path)`.
fn setup_workspace_for_start(tmp: &Path) -> (String, String, PathBuf) {
    let team_name = "e2e-start";

    // Find a profile dynamically (profile-agnostic)
    let (profile_name, roles) = find_profile_with_role();
    let role = &roles[0];
    let member_name = "alice";
    let member_dir_name = format!("{}-{}", role, member_name);

    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);

    // team_repo = team.path.join("team") — the team repo root
    let team_repo = team_dir.join("team");

    // Place botminter.yml in the team repo
    fs::create_dir_all(&team_repo).unwrap();
    let manifest = profile::read_manifest(&profile_name).unwrap();
    let manifest_yml = serde_yml::to_string(&manifest).unwrap();
    fs::write(team_repo.join("botminter.yml"), &manifest_yml).unwrap();

    // Member discovery: start.rs reads team_repo.join("team")/<member>/
    let members_dir = team_repo.join("team");
    let member_config_dir = members_dir.join(&member_dir_name);
    fs::create_dir_all(&member_config_dir).unwrap();

    // Workspace: find_workspace looks at workzone/<team>/<member>/
    let workspace = team_dir.join(&member_dir_name).join("workspace");
    fs::create_dir_all(workspace.join(".botminter")).unwrap();
    fs::write(workspace.join("PROMPT.md"), "# E2E Test Prompt\n").unwrap();

    // Write config
    let config = BotminterConfig {
        workzone,
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: profile_name,
            github_repo: "devguyio-bot-squad/e2e-placeholder".to_string(),
            credentials: Credentials {
                gh_token: Some("ghp_e2e_test_token".to_string()),
                telegram_bot_token: None,
                webhook_secret: None,
            },
        }],
    };
    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    (team_name.to_string(), member_dir_name, workspace)
}

/// Finds a profile with at least 1 role. Returns (profile_name, roles).
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

// ── Helpers ──────────────────────────────────────────────────────────

/// Creates a `bm` command with HOME and PATH configured.
fn start_cmd(tmp: &Path, stub_dir: &Path, args: &[&str]) -> Command {
    let mut cmd = bm_cmd();
    cmd.args(args)
        .env("HOME", tmp)
        .env("PATH", path_with_stub(stub_dir));
    cmd
}

/// Reads the first PID from state.json.
///
/// More reliable than parsing the status table (which uses Unicode
/// box-drawing characters that are fragile to parse).
fn read_pid_from_state(home: &Path) -> Option<u32> {
    let state_path = home.join(".botminter").join("state.json");
    if !state_path.exists() {
        return None;
    }
    let contents = fs::read_to_string(&state_path).ok()?;
    let state: bm::state::RuntimeState = serde_json::from_str(&contents).ok()?;
    state.members.values().next().map(|rt| rt.pid)
}

/// Cleanup guard that force-kills a PID on drop.
struct ProcessGuard {
    pid: Option<u32>,
    home: PathBuf,
    stub_dir: PathBuf,
    team_name: String,
}

impl ProcessGuard {
    fn new(home: &Path, stub_dir: &Path, team_name: &str) -> Self {
        ProcessGuard {
            pid: None,
            home: home.to_path_buf(),
            stub_dir: stub_dir.to_path_buf(),
            team_name: team_name.to_string(),
        }
    }

    fn set_pid(&mut self, pid: u32) {
        self.pid = Some(pid);
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        // Try graceful stop via bm first
        let _ = bm_cmd()
            .args(["stop", "--force", "-t", &self.team_name])
            .env("HOME", &self.home)
            .env("PATH", path_with_stub(&self.stub_dir))
            .output();

        // Force kill if still alive
        if let Some(pid) = self.pid {
            if is_alive(pid) {
                force_kill(pid);
                // Brief wait for cleanup
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────

/// Full lifecycle: start → status(running) → stop → status(stopped).
/// Verifies state.json transitions correctly.
#[test]
fn e2e_start_status_stop_lifecycle() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path());
    let (team_name, member_dir_name, _workspace) =
        setup_workspace_for_start(tmp.path());

    let mut guard = ProcessGuard::new(tmp.path(), &stub_dir, &team_name);

    // ── Start ────────────────────────────────────────────────────────
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["start", "-t", &team_name]);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("start: {}", out.trim());
    assert!(
        out.contains("Started 1 member"),
        "Expected 'Started 1 member' in output: {}",
        out
    );

    // ── Status: running ──────────────────────────────────────────────
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["status", "-t", &team_name]);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("status (running): {}", out.trim());
    assert!(
        out.contains("running"),
        "Expected 'running' in status output: {}",
        out
    );
    assert!(
        out.contains(&member_dir_name),
        "Expected member '{}' in status output: {}",
        member_dir_name,
        out
    );

    // Extract PID for cleanup guard
    if let Some(pid) = read_pid_from_state(tmp.path()) {
        guard.set_pid(pid);
    }

    // ── Stop ─────────────────────────────────────────────────────────
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["stop", "-t", &team_name]);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("stop: {}", out.trim());
    assert!(
        out.contains("Stopped 1 member"),
        "Expected 'Stopped 1 member' in output: {}",
        out
    );

    // ── Status: stopped ──────────────────────────────────────────────
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["status", "-t", &team_name]);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("status (stopped): {}", out.trim());
    assert!(
        out.contains("stopped"),
        "Expected 'stopped' in status output: {}",
        out
    );

    // ── State file clean ─────────────────────────────────────────────
    let state_path = tmp.path().join(".botminter").join("state.json");
    if state_path.exists() {
        let state: bm::state::RuntimeState =
            serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
        assert!(
            state.members.is_empty(),
            "state.json should have no members after stop, found: {:?}",
            state.members.keys().collect::<Vec<_>>()
        );
    }
    // If state.json doesn't exist, that's also fine (clean state)
}

/// Second `bm start` when member is already running says "already running"
/// and doesn't spawn a duplicate process.
#[test]
fn e2e_start_already_running_skips() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path());
    let (team_name, _member_dir, _workspace) =
        setup_workspace_for_start(tmp.path());

    let mut guard = ProcessGuard::new(tmp.path(), &stub_dir, &team_name);

    // First start
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["start", "-t", &team_name]);
    let out1 = assert_cmd_success(&mut cmd);
    eprintln!("start 1: {}", out1.trim());
    assert!(out1.contains("Started 1 member"));

    // Get PID from state.json
    let pid1 = read_pid_from_state(tmp.path());
    if let Some(pid) = pid1 {
        guard.set_pid(pid);
    }

    // Second start — should say "already running"
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["start", "-t", &team_name]);
    let out2 = assert_cmd_success(&mut cmd);
    eprintln!("start 2: {}", out2.trim());
    assert!(
        out2.contains("already running"),
        "Second start should say 'already running', got: {}",
        out2
    );
    // No new members started
    assert!(
        out2.contains("Started 0 member") || out2.contains("skipped 1"),
        "Second start should skip the running member, got: {}",
        out2
    );

    // Verify same PID (no duplicate process)
    let pid2 = read_pid_from_state(tmp.path());
    assert_eq!(
        pid1, pid2,
        "PID should remain the same: first={:?}, second={:?}",
        pid1, pid2
    );
}

/// `bm stop --force` terminates the process immediately and cleans state.
#[test]
fn e2e_stop_force_kills() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path());
    let (team_name, _member_dir, _workspace) =
        setup_workspace_for_start(tmp.path());

    let mut guard = ProcessGuard::new(tmp.path(), &stub_dir, &team_name);

    // Start
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["start", "-t", &team_name]);
    assert_cmd_success(&mut cmd);

    // Get PID from state.json
    let pid = read_pid_from_state(tmp.path()).expect("should have a PID in state.json");
    guard.set_pid(pid);
    assert!(is_alive(pid), "Process {} should be alive before force stop", pid);

    // Force stop
    let mut cmd = start_cmd(
        tmp.path(),
        &stub_dir,
        &["stop", "--force", "-t", &team_name],
    );
    let out = assert_cmd_success(&mut cmd);
    eprintln!("force stop: {}", out.trim());

    // Process should be dead
    wait_for_exit(pid, Duration::from_secs(5));
    assert!(!is_alive(pid), "Process {} should be dead after force stop", pid);

    // State should be clean
    let state_path = tmp.path().join(".botminter").join("state.json");
    if state_path.exists() {
        let state: bm::state::RuntimeState =
            serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
        assert!(
            state.members.is_empty(),
            "state.json should be clean after force stop"
        );
    }
}

/// Kill the Ralph process externally, then `bm status` detects "crashed"
/// and cleans up state.
#[test]
fn e2e_status_detects_crashed_member() {
    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path());
    let (team_name, member_dir_name, _workspace) =
        setup_workspace_for_start(tmp.path());

    let _guard = ProcessGuard::new(tmp.path(), &stub_dir, &team_name);

    // Start
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["start", "-t", &team_name]);
    assert_cmd_success(&mut cmd);

    // Get PID from state.json
    let pid = read_pid_from_state(tmp.path()).expect("should have a PID in state.json");

    // Kill externally with SIGKILL (simulates crash — not interceptable)
    force_kill(pid);
    wait_for_exit(pid, Duration::from_secs(5));

    // Status should detect crash
    let mut cmd = start_cmd(tmp.path(), &stub_dir, &["status", "-t", &team_name]);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("status (crashed): {}", out.trim());
    assert!(
        out.contains("crashed"),
        "Status should show 'crashed' for killed member, got: {}",
        out
    );
    assert!(
        out.contains(&member_dir_name),
        "Status should show member name '{}', got: {}",
        member_dir_name,
        out
    );

    // State should be cleaned up (crashed entry removed)
    let state_path = tmp.path().join(".botminter").join("state.json");
    if state_path.exists() {
        let state: bm::state::RuntimeState =
            serde_json::from_str(&fs::read_to_string(&state_path).unwrap()).unwrap();
        assert!(
            state.members.is_empty(),
            "state.json should be clean after crash detection, found: {:?}",
            state.members.keys().collect::<Vec<_>>()
        );
    }
}

/// After starting with tg-mock configured, the stub ralph makes a Bot API call.
/// Verifies env var propagation and tg-mock reachability.
#[test]
fn e2e_tg_mock_receives_bot_messages() {
    if !super::telegram::podman_available() {
        eprintln!("SKIP: podman not available — skipping tg-mock test");
        return;
    }

    let tmp = tempfile::tempdir().unwrap();
    let stub_dir = create_stub_ralph(tmp.path());
    let (team_name, _member_dir, workspace) =
        setup_workspace_for_start(tmp.path());

    let mut guard = ProcessGuard::new(tmp.path(), &stub_dir, &team_name);

    // Start tg-mock
    let mock = super::telegram::TgMock::start();
    // Use a realistic Telegram bot token format: <bot_id>:<secret>
    let bot_token = "123456789:ABCDEFGhijklmnopqrstuvwxyz";

    // Start bm with Telegram env vars
    // RALPH_TELEGRAM_API_URL is inherited by the spawned ralph process
    let mut cmd = bm_cmd();
    cmd.args(["start", "-t", &team_name])
        .env("HOME", tmp.path())
        .env("PATH", path_with_stub(&stub_dir))
        .env("RALPH_TELEGRAM_API_URL", mock.api_url())
        .env("RALPH_TELEGRAM_BOT_TOKEN", bot_token);
    let out = assert_cmd_success(&mut cmd);
    eprintln!("start with tg-mock: {}", out.trim());
    assert!(out.contains("Started 1 member"));

    // Get PID for cleanup
    if let Some(pid) = read_pid_from_state(tmp.path()) {
        guard.set_pid(pid);
    }

    // Wait for the stub to write its env file and tg response
    // The stub writes these on startup before sleeping
    std::thread::sleep(Duration::from_secs(3));

    // Verify env vars were propagated to the ralph process
    let env_file = workspace.join(".ralph-stub-env");
    assert!(
        env_file.exists(),
        "Stub should have written .ralph-stub-env file at {}",
        env_file.display()
    );
    let env_content = fs::read_to_string(&env_file).unwrap();
    assert!(
        env_content.contains("RALPH_TELEGRAM_API_URL="),
        "Env should contain RALPH_TELEGRAM_API_URL, got: {}",
        env_content
    );
    assert!(
        env_content.contains("GH_TOKEN="),
        "Env should contain GH_TOKEN, got: {}",
        env_content
    );

    // Verify the stub made a Bot API call to tg-mock
    let tg_response_file = workspace.join(".ralph-stub-tg-response");
    assert!(
        tg_response_file.exists(),
        "Stub should have written .ralph-stub-tg-response (Bot API call made)"
    );
    let tg_response = fs::read_to_string(&tg_response_file).unwrap();
    eprintln!("tg-mock response: {}", tg_response.trim());
    // tg-mock responds with valid Bot API JSON containing "ok"
    assert!(
        tg_response.contains("ok"),
        "tg-mock response should contain 'ok' (valid Bot API response), got: {}",
        tg_response
    );
}

/// When `ralph` is not in PATH, `bm start` produces a clear error.
#[test]
fn e2e_start_without_ralph_errors() {
    let tmp = tempfile::tempdir().unwrap();
    // Set up workspace but DON'T create a stub ralph
    let (team_name, _member_dir, _workspace) =
        setup_workspace_for_start(tmp.path());

    // Use a restricted PATH that excludes ralph
    // Include /usr/bin and /bin for basic utilities but not the ralph binary
    let restricted_path = "/usr/bin:/bin:/usr/sbin:/sbin";

    let mut cmd = bm_cmd();
    cmd.args(["start", "-t", &team_name])
        .env("HOME", tmp.path())
        .env("PATH", restricted_path);
    let stderr = assert_cmd_fails(&mut cmd);
    eprintln!("start without ralph: {}", stderr.trim());
    assert!(
        stderr.contains("ralph") && stderr.contains("not found"),
        "Error should mention 'ralph' and 'not found', got: {}",
        stderr
    );
}
