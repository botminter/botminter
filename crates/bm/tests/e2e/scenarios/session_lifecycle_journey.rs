//! Session Lifecycle Journey E2E tests — CT-154-06
//!
//! Covers all 5 gap fixes from Story #154:
//!   GAP-01 / CT-154-01: Finalization lifecycle (commit+push dirty state, failed finalization)
//!   GAP-02 / CT-154-02: .claude/ directory assembly in session workspace
//!   GAP-03 / CT-154-03: Credential relay (GH_CONFIG_DIR passed to member agent)
//!   GAP-04 / CT-154-04: Work-item lock acquire/release via bm-agent CLI
//!   GAP-05 / CT-154-05: bm session list --json and bm session finalize commands

use std::collections::HashSet;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::time::Duration;

use bm::profile::CodingAgentDef;
use bm::workspace;
use libtest_mimic::Trial;

use super::super::helpers::{
    is_alive, list_session_workspaces, wait_for_exit, wait_for_new_session_workspace,
    wait_for_stub_pid, E2eConfig, GithubSuite, ProcessGuard,
};
use super::super::test_env::TestEnv;

const TEAM_NAME: &str = "e2e-slj";
const PROFILE: &str = "agentic-sdlc-minimal";
const ROLE: &str = "engineer";
const MEMBER_NAME: &str = "carol";
const MEMBER_DIR: &str = "engineer-carol";

const STUB_FINALIZATION_SCRIPT: &str =
    concat!(env!("CARGO_MANIFEST_DIR"), "/tests/e2e/stub-finalization.sh");

const MEETING_NAME: &str = "e2e-planning";
const MEETING_MEMBER_ROLE: &str = "engineer";

// ── Helpers ───────────────────────────────────────────────────────────

/// Appends a test meeting definition to the team repo's botminter.yml.
/// Required because agentic-sdlc-minimal has no meetings in its profile.
fn inject_meeting_into_manifest(team_repo: &Path, meeting_name: &str, member_role: &str) {
    let manifest_path = team_repo.join("botminter.yml");
    let existing = fs::read_to_string(&manifest_path)
        .expect("botminter.yml must be readable for meeting injection");
    let meeting_yaml = format!(
        "\nmeetings:\n  - name: \"{name}\"\n    description: \"E2E test meeting\"\n    member: \"{role}\"\n    instructions: \"You are in a test meeting. Exit when done.\"\n",
        name = meeting_name,
        role = member_role,
    );
    fs::write(&manifest_path, format!("{}\n{}", existing.trim_end(), meeting_yaml))
        .expect("botminter.yml must be writable for meeting injection");
}

fn read_daemon_port(home: &Path, team_name: &str) -> u16 {
    let cfg_path = home.join(format!(".botminter/daemon-{}.json", team_name));
    let raw = fs::read_to_string(&cfg_path)
        .unwrap_or_else(|e| panic!("failed to read daemon config {}: {}", cfg_path.display(), e));
    let cfg: serde_json::Value =
        serde_json::from_str(&raw).expect("daemon config must be valid JSON");
    cfg["port"].as_u64().expect("daemon config must have port field") as u16
}

/// Poll `GET /api/sessions/:id` until the session's current_state matches one of the
/// expected states, or panic after timeout.
///
/// Works for all states (Active, Retained, Finalizing, Completed, Failed) because terminal
/// sessions remain in the registry until cleaned up.
fn poll_session_state(
    env: &TestEnv,
    session_id: &str,
    expected_states: &[&str],
    timeout: Duration,
    team_name: &str,
) -> String {
    let port = read_daemon_port(&env.home, team_name);
    let url = format!("http://127.0.0.1:{}/api/sessions/{}", port, session_id);
    let start = std::time::Instant::now();
    let mut last_state = String::new();
    while start.elapsed() < timeout {
        let out = env.command("curl").args(["-s", &url]).output();
        if out.status.success() {
            if let Ok(v) = serde_json::from_slice::<serde_json::Value>(&out.stdout) {
                if let Some(state) = v["session"]["current_state"].as_str() {
                    last_state = state.to_string();
                    if expected_states.contains(&state) {
                        return last_state;
                    }
                }
            }
        }
        std::thread::sleep(Duration::from_millis(500));
    }
    panic!(
        "session {session_id} did not reach {:?} within {:?} (last state: {:?})",
        expected_states, timeout, last_state
    );
}

/// Install stub-finalization.sh as stub-bin/claude (exits 0, commits+pushes dirty state).
fn install_stub_claude(home: &Path) {
    let dest = home.join("stub-bin/claude");
    fs::copy(STUB_FINALIZATION_SCRIPT, &dest)
        .expect("failed to copy stub-finalization.sh to stub-bin/claude");
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod stub-bin/claude");
}

/// Install a claude stub that always exits 1 (simulates finalization failure).
fn install_fail_claude(home: &Path) {
    let dest = home.join("stub-bin/claude");
    fs::write(&dest, "#!/bin/bash\nexit 1\n").expect("failed to write fail-claude script");
    fs::set_permissions(&dest, fs::Permissions::from_mode(0o755))
        .expect("failed to chmod fail-claude");
}

// ── Suite setup ────────────────────────────────────────────────────────

fn setup_fn(
    gh_org: String,
    _gh_token: String,
    app_id: String,
    app_client_id: String,
    app_installation_id: String,
    app_private_key_file: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let workzone = env.home.join("workspaces");
        let repo_name = env.repo_full_name.split('/').next_back().unwrap().to_string();
        let ts = repo_name.split('-').next_back().unwrap_or("0");
        let board_title = format!("e2e-slj-board-{}", ts);

        // 1. Init (no bridge — lifecycle journey does not need a bridge)
        let init_out = env
            .command("bm")
            .args([
                "init",
                "--non-interactive",
                "--profile",
                PROFILE,
                "--team-name",
                TEAM_NAME,
                "--org",
                &gh_org,
                "--repo",
                &repo_name,
                "--github-project-board",
                &board_title,
                "--workzone",
                &workzone.to_string_lossy(),
            ])
            .output();
        assert!(
            init_out.status.success(),
            "bm init failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&init_out.stdout),
            String::from_utf8_lossy(&init_out.stderr)
        );

        let team_dir = workzone.join(TEAM_NAME);
        let team_repo = team_dir.join("team");
        assert!(team_repo.join(".git").is_dir(), "team repo must exist after init");

        // 2. Hire (reuse pre-provisioned App)
        let hire_out = env
            .command("bm")
            .args([
                "hire",
                ROLE,
                "--name",
                MEMBER_NAME,
                "-t",
                TEAM_NAME,
                "--reuse-app",
                "--app-id",
                &app_id,
                "--client-id",
                &app_client_id,
                "--private-key-file",
                &app_private_key_file,
                "--installation-id",
                &app_installation_id,
            ])
            .output();
        assert!(
            hire_out.status.success(),
            "bm hire failed:\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&hire_out.stdout),
            String::from_utf8_lossy(&hire_out.stderr)
        );

        // 3. Install stub-claude BEFORE bm start so daemon inherits PATH with it.
        //    The daemon will use stub-bin/claude as the finalization subagent.
        install_stub_claude(&env.home);

        // 4. Provision static workspace (member config, CLAUDE.md, ralph.yml, submodules).
        let coding_agent = CodingAgentDef {
            name: "claude-code".to_string(),
            display_name: "Claude Code".to_string(),
            context_file: "CLAUDE.md".to_string(),
            agent_dir: ".claude".to_string(),
            binary: "claude".to_string(),
            system_prompt_flag: Some("--append-system-prompt-file".to_string()),
            skip_permissions_flag: Some("--dangerously-skip-permissions".to_string()),
        };

        let params = workspace::WorkspaceRepoParams {
            team_repo_path: &team_repo,
            workspace_base: &team_dir,
            member_dir_name: MEMBER_DIR,
            team_name: TEAM_NAME,
            projects: &[],
            github_repo: None,
            project_number: None,
            push: false,
            coding_agent: &coding_agent,
            remote_ops: None,
            team_submodule_url: None,
        };
        workspace::create_workspace_repo(&params).expect("create_workspace_repo must succeed");

        let static_ws = team_dir.join(MEMBER_DIR);
        workspace::inject_robot_enabled(&static_ws.join("ralph.yml"), false)
            .expect("inject_robot_enabled must succeed");
        assert!(
            static_ws.join(".botminter.workspace").exists(),
            ".botminter.workspace must exist in static workspace"
        );
    }
}

// ── Case 1: .claude/ assembly (GAP-02) ────────────────────────────────

fn claude_dir_assembly_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        if let Some(pid) = wait_for_stub_pid(&ws, Duration::from_secs(15)) {
            guard.set_pid(pid);
        }

        // GAP-02: The .claude/ directory and its agent files must be assembled
        // in the session workspace from the team repo's profile.
        assert!(
            ws.join(".claude").is_dir(),
            ".claude/ must exist in session workspace (GAP-02)"
        );
        assert!(
            ws.join(".claude/agents").is_dir(),
            ".claude/agents/ must exist (GAP-02)"
        );
        assert!(
            ws.join(".claude/agents/finalization.md").exists(),
            ".claude/agents/finalization.md must be surfaced from team repo (GAP-02)"
        );
        assert!(
            ws.join(".claude/settings.json").exists(),
            ".claude/settings.json must be surfaced from team repo (GAP-02)"
        );

        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
    }
}

// ── Case 2: Credential relay (GAP-03) ─────────────────────────────────

fn credential_relay_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        if let Some(pid) = wait_for_stub_pid(&ws, Duration::from_secs(15)) {
            guard.set_pid(pid);
        }

        // Wait for .ralph-stub-env (written after stub-ralph finishes startup polling)
        let env_file = ws.join(".ralph-stub-env");
        let deadline = std::time::Instant::now() + Duration::from_secs(20);
        while !env_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(200));
        }
        let env_content = fs::read_to_string(&env_file)
            .expect(".ralph-stub-env must exist after stub-ralph writes it (GAP-03)");

        // GAP-03: GH_CONFIG_DIR must be forwarded to the member agent so it can authenticate.
        assert!(
            env_content.contains("GH_CONFIG_DIR="),
            "GH_CONFIG_DIR must be present in member agent env (GAP-03), got:\n{env_content}"
        );

        // Extract the path and verify hosts.yml was written there by the credential relay.
        let gh_config_dir = env_content
            .lines()
            .find(|l| l.starts_with("GH_CONFIG_DIR="))
            .and_then(|l| l.strip_prefix("GH_CONFIG_DIR="))
            .map(|v| v.trim().to_string())
            .expect("GH_CONFIG_DIR line must be parseable");

        let hosts_yml = PathBuf::from(&gh_config_dir).join("hosts.yml");
        assert!(
            hosts_yml.exists(),
            "hosts.yml must exist at {gh_config_dir} (credential relay must have written it, GAP-03)"
        );

        let gh_api_out = std::process::Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .env("GH_CONFIG_DIR", &gh_config_dir)
            .output()
            .expect("gh binary must be available in E2E test environment");
        assert!(
            gh_api_out.status.success(),
            "gh api user must succeed using GH_CONFIG_DIR={gh_config_dir} \
             (credential relay must write a valid App token, GAP-03 AC-09), stderr: {}",
            String::from_utf8_lossy(&gh_api_out.stderr)
        );

        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
    }
}

// ── Case 3: Finalization lifecycle (GAP-01) ────────────────────────────

fn finalization_lifecycle_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        let pid = wait_for_stub_pid(&ws, Duration::from_secs(15))
            .expect("stub-ralph must write PID file");
        guard.set_pid(pid);

        let team_repo = env.home.join("workspaces").join(TEAM_NAME).join("team");

        // Clone team repo into the session workspace so the finalization stub can commit.
        // The remote is repointed to the GitHub URL so the stub's push reaches the remote.
        let team_in_ws = ws.join("team");
        env.command("git")
            .args(["clone", &team_repo.to_string_lossy(), &team_in_ws.to_string_lossy()])
            .run();
        let github_url = env
            .command("git")
            .args(["remote", "get-url", "origin"])
            .current_dir(&team_repo)
            .run()
            .trim()
            .to_string();
        env.command("git")
            .args(["remote", "set-url", "origin", &github_url])
            .current_dir(&team_in_ws)
            .run();

        // Record current main branch hash to verify git push happened later.
        let hash_before = env
            .command("git")
            .args(["ls-remote", "origin", "refs/heads/main"])
            .current_dir(&team_repo)
            .run()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();

        // Create dirty state: uncommitted file in team/specs/ triggers CommitAndPush finalization.
        let specs_dir = team_in_ws.join("specs");
        fs::create_dir_all(&specs_dir).expect("team/specs must be creatable");
        fs::write(
            specs_dir.join("test-e2e-fin.md"),
            "E2E finalization lifecycle sentinel\n",
        )
        .expect("failed to create dirty state file");

        let session_id = ws.file_name().unwrap().to_string_lossy().to_string();

        // Graceful stop: SIGTERM → stub-ralph exits → daemon inspects dirty state → spawns
        // stub-claude → stub-claude commits+pushes → session transitions to Completed.
        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
        wait_for_exit(pid, Duration::from_secs(10));
        assert!(!is_alive(pid), "stub-ralph must be dead after bm stop");

        // Poll for Completed (or Failed) within 90s ceiling (CRITICAL: max 120s per invariant).
        let final_state = poll_session_state(
            env,
            &session_id,
            &["Completed", "Failed"],
            Duration::from_secs(90),
            TEAM_NAME,
        );
        assert_eq!(
            final_state, "Completed",
            "session must reach Completed after finalization stub commits+pushes (GAP-01 / AC-02)"
        );

        // Verify the finalization stub actually pushed: main hash must have advanced.
        let hash_after = env
            .command("git")
            .args(["ls-remote", "origin", "refs/heads/main"])
            .current_dir(&team_repo)
            .run()
            .split_whitespace()
            .next()
            .unwrap_or("")
            .to_string();
        assert_ne!(
            hash_before, hash_after,
            "a new commit must have been pushed to main by the finalization stub (GAP-01)"
        );

        // Guard already stopped — tell Drop not to stop again.
        std::mem::forget(guard);
    }
}

// ── Case 4: Work-item lock (GAP-04) ───────────────────────────────────

fn work_item_lock_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        let pid = wait_for_stub_pid(&ws, Duration::from_secs(15))
            .expect("stub-ralph must write PID file");
        guard.set_pid(pid);

        // Fake workspace for session B: a directory with a .botminter.workspace marker
        // containing a fake session_id. bm-agent reads this to get the session_id.
        let fake_ws = env.home.join("lock-test-ws-b");
        fs::create_dir_all(&fake_ws).expect("failed to create fake workspace dir");
        fs::write(
            fake_ws.join(".botminter.workspace"),
            "session_id: fake-lock-session-b\nteam_name: e2e-slj\nmember: engineer-carol\n",
        )
        .expect("failed to write fake workspace marker");

        // Build env for bm-agent with BM_TEAM_NAME set (required by connect_daemon()).
        let bm_agent_bin = env!("CARGO_BIN_EXE_bm-agent");
        let mut agent_env_a = env.resolved_env("bm-agent");
        agent_env_a.insert("BM_TEAM_NAME".to_string(), TEAM_NAME.to_string());
        let agent_env_b = agent_env_a.clone();

        let ws_a = ws.clone();
        let ws_b = fake_ws.clone();
        let (tx_a, rx_a) = std::sync::mpsc::channel::<i32>();
        let (tx_b, rx_b) = std::sync::mpsc::channel::<i32>();

        let bin_a = bm_agent_bin.to_string();
        let bin_b = bm_agent_bin.to_string();

        // GAP-04: Parallel lock acquire — exactly one session must succeed (exit 0),
        // the other must see contention (exit 1).
        std::thread::spawn(move || {
            let code = std::process::Command::new(&bin_a)
                .env_clear()
                .envs(&agent_env_a)
                .args(["lock", "acquire", "ISSUE-99"])
                .current_dir(&ws_a)
                .status()
                .map(|s| s.code().unwrap_or(127))
                .unwrap_or(127);
            let _ = tx_a.send(code);
        });

        std::thread::spawn(move || {
            let code = std::process::Command::new(&bin_b)
                .env_clear()
                .envs(&agent_env_b)
                .args(["lock", "acquire", "ISSUE-99"])
                .current_dir(&ws_b)
                .status()
                .map(|s| s.code().unwrap_or(127))
                .unwrap_or(127);
            let _ = tx_b.send(code);
        });

        let code_a = rx_a.recv().expect("thread A must complete");
        let code_b = rx_b.recv().expect("thread B must complete");

        eprintln!("  lock acquire: session-A={code_a}, session-B={code_b}");

        let one_acquired = (code_a == 0) ^ (code_b == 0);
        let both_valid = (code_a == 0 || code_a == 1) && (code_b == 0 || code_b == 1);
        assert!(
            one_acquired && both_valid,
            "exactly one acquire must succeed (exit 0) and one see contention (exit 1) (GAP-04), \
             got session-A={code_a}, session-B={code_b}"
        );

        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
        wait_for_exit(pid, Duration::from_secs(10));
    }
}

// ── Case 5: bm session list --json (GAP-05) ───────────────────────────

fn session_list_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        let pid = wait_for_stub_pid(&ws, Duration::from_secs(15))
            .expect("stub-ralph must write PID file");
        guard.set_pid(pid);

        // GAP-05: bm session list --json must return a JSON array with required fields.
        let list_out = env
            .command("bm")
            .args(["session", "list", "--json", "-t", TEAM_NAME])
            .run();
        let parsed: serde_json::Value = serde_json::from_str(&list_out).unwrap_or_else(|e| {
            panic!(
                "bm session list --json must return valid JSON: {e}\nraw output: {list_out}"
            )
        });
        assert!(
            parsed.is_array(),
            "bm session list --json must return a JSON array (GAP-05), got: {parsed}"
        );
        let rows = parsed.as_array().unwrap();
        assert!(
            !rows.is_empty(),
            "bm session list --json must include at least one session (GAP-05)"
        );

        // Every row must have the required fields.
        for row in rows {
            assert!(
                row["session_id"].is_string(),
                "row must have session_id field (GAP-05): {row}"
            );
            assert!(
                row["member"].is_string(),
                "row must have member field (GAP-05): {row}"
            );
            assert!(
                row["state"].is_string(),
                "row must have state field (GAP-05): {row}"
            );
            assert!(
                row["finalization_status"].is_string(),
                "row must have finalization_status field (GAP-05): {row}"
            );
            assert!(
                row["started_at"].is_string(),
                "row must have started_at field (GAP-05): {row}"
            );
            assert!(
                row["ended_at"].is_string(),
                "row must have ended_at field (GAP-05): {row}"
            );
        }

        // The running session must appear in the list.
        let has_active = rows.iter().any(|r| r["state"].as_str() == Some("Active"));
        assert!(
            has_active,
            "bm session list must include the running Active session (GAP-05): {rows:?}"
        );

        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
        wait_for_exit(pid, Duration::from_secs(10));
    }
}

// ── Case 6: bm session finalize (GAP-05) ──────────────────────────────

fn session_finalize_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        let pid = wait_for_stub_pid(&ws, Duration::from_secs(15))
            .expect("stub-ralph must write PID file");
        guard.set_pid(pid);

        // Write SIGTERM-ignore file so stub-ralph ignores the graceful stop signal.
        // This drives the session through Active → Finalizing → Retained path.
        fs::write(ws.join(".ralph-stub-ignore-sigterm"), "")
            .expect("failed to write SIGTERM ignore file");

        // Graceful stop → SIGTERM → stub-ralph ignores → session: Active → Finalizing.
        // Force stop  → SIGKILL → stub-ralph dies   → session: Finalizing → Retained.
        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
        env.command("bm")
            .args(["stop", "--force", "-t", TEAM_NAME])
            .run();
        wait_for_exit(pid, Duration::from_secs(5));
        assert!(!is_alive(pid), "stub-ralph must be dead after force stop");

        let session_id = ws.file_name().unwrap().to_string_lossy().to_string();

        // Pre-condition: session must be Retained before triggering finalization.
        poll_session_state(
            env,
            &session_id,
            &["Retained"],
            Duration::from_secs(10),
            TEAM_NAME,
        );

        // GAP-05: bm session finalize re-triggers finalization on a Retained session.
        let finalize_out = env
            .command("bm")
            .args(["session", "finalize", session_id.as_str(), "-t", TEAM_NAME])
            .run();
        assert!(
            finalize_out.contains("Finalization triggered"),
            "bm session finalize must confirm trigger (GAP-05): {finalize_out}"
        );

        // Session must transition to Finalizing (stub-claude runs) and eventually Completed.
        let post_state = poll_session_state(
            env,
            &session_id,
            &["Finalizing", "Completed"],
            Duration::from_secs(30),
            TEAM_NAME,
        );
        assert!(
            post_state == "Finalizing" || post_state == "Completed",
            "session must reach Finalizing or Completed after bm session finalize (GAP-05), got: {post_state}"
        );

        // Cleanup the terminal session before the suite ends.
        let _ = env
            .command("bm")
            .args(["session", "cleanup", session_id.as_str(), "-t", TEAM_NAME])
            .output();

        std::mem::forget(guard);
    }
}

// ── Case 7: Failed finalization (GAP-01 / AC-03) ──────────────────────

fn failed_finalization_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        // Replace stub-claude with a script that always exits 1, simulating a finalization failure.
        install_fail_claude(&env.home);

        let mut guard = ProcessGuard::new(env, TEAM_NAME);

        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        let stdout = env.command("bm").args(["start", "-t", TEAM_NAME]).run();
        assert!(
            stdout.contains("Started 1 member"),
            "bm start must report 1 member started: {stdout}"
        );

        let ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(20),
        )
        .expect("session workspace must appear within 20s");

        let pid = wait_for_stub_pid(&ws, Duration::from_secs(15))
            .expect("stub-ralph must write PID file");
        guard.set_pid(pid);

        // Clone team repo into the session workspace so inspect_dirty_state can detect the dirty
        // file (team/ must be a git repo). Fail-claude does not push, so no remote fixup needed.
        let team_repo = env.home.join("workspaces").join(TEAM_NAME).join("team");
        let team_in_ws = ws.join("team");
        env.command("git")
            .args(["clone", &team_repo.to_string_lossy(), &team_in_ws.to_string_lossy()])
            .run();

        // Create dirty state so finalization is triggered (not skipped) on stop.
        let specs_dir = team_in_ws.join("specs");
        fs::create_dir_all(&specs_dir).expect("team/specs must be creatable");
        fs::write(
            specs_dir.join("test-e2e-fail.md"),
            "E2E failed finalization sentinel\n",
        )
        .expect("failed to create dirty state file for failure test");

        let session_id = ws.file_name().unwrap().to_string_lossy().to_string();

        // Graceful stop: stub-ralph exits → daemon inspects dirty → spawns fail-claude → exits 1
        // → daemon background watcher transitions session to Failed.
        env.command("bm").args(["stop", "-t", TEAM_NAME]).run();
        wait_for_exit(pid, Duration::from_secs(10));
        assert!(!is_alive(pid), "stub-ralph must be dead after bm stop");

        // Poll for Failed (or Completed, which would be a test failure).
        // 90s ceiling — well under the 120s CRITICAL constraint.
        let final_state = poll_session_state(
            env,
            &session_id,
            &["Failed", "Completed"],
            Duration::from_secs(90),
            TEAM_NAME,
        );
        assert_eq!(
            final_state, "Failed",
            "session must reach Failed when finalization binary exits 1 (GAP-01 / AC-03)"
        );

        // Cleanup the failed session.
        let _ = env
            .command("bm")
            .args(["session", "cleanup", session_id.as_str(), "-t", TEAM_NAME])
            .output();

        // Restore stub-claude for clean teardown state.
        install_stub_claude(&env.home);

        std::mem::forget(guard);
    }
}

// ── Case 8: Meetings ephemeral session lifecycle (CT-154-01-MEETINGS) ─────

/// Verifies that `bm meetings` wires to the ephemeral session lifecycle:
/// start_session → ephemeral workspace → launch meeting → stop_session → finalization.
///
/// FAILS against current code: `bm meetings` uses the old permanent workspace
/// path and never calls start_session, so no ephemeral session workspace is created.
fn meetings_finalization_fn(
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    |env| {
        let team_dir = env.home.join("workspaces").join(TEAM_NAME);
        let team_repo = team_dir.join("team");

        // Inject a meeting into the team manifest (profile has none by default).
        inject_meeting_into_manifest(&team_repo, MEETING_NAME, MEETING_MEMBER_ROLE);

        // Record pre-existing session workspaces (expected: 0 — no daemon sessions yet).
        let pre_existing: HashSet<PathBuf> =
            list_session_workspaces(&env.home, TEAM_NAME, MEMBER_DIR)
                .into_iter()
                .collect();

        // Install stub-claude (exits 0 quickly — simulates the meeting participant).
        install_stub_claude(&env.home);

        // Run bm meetings. It must call start_session on the daemon to create an ephemeral
        // session workspace, launch the meeting participant, then call stop_session on exit.
        // Run from the test home so detect_meetings_from_workspace() doesn't walk up and
        // find an outer .botminter.workspace marker from the test-runner's working directory.
        let meetings_out = env
            .command("bm")
            .current_dir(&env.home)
            .args(["meetings", MEETING_NAME, "-t", TEAM_NAME, "-a"])
            .output();
        let meetings_stdout = String::from_utf8_lossy(&meetings_out.stdout).to_string();
        let meetings_stderr = String::from_utf8_lossy(&meetings_out.stderr).to_string();

        // Wait for a new ephemeral session workspace to appear.
        let new_ws = wait_for_new_session_workspace(
            &env.home,
            TEAM_NAME,
            MEMBER_DIR,
            &pre_existing,
            Duration::from_secs(10),
        );
        assert!(
            new_ws.is_some(),
            "bm meetings must create an ephemeral session workspace via the daemon \
             (CT-154-01-MEETINGS). exit={}, stdout={:?}, stderr={:?}",
            meetings_out.status.code().unwrap_or(-1),
            meetings_stdout,
            meetings_stderr,
        );
    }
}

// ── Suite builders ─────────────────────────────────────────────────────

fn build_suite(config: &E2eConfig, repo_full_name: &str) -> GithubSuite {
    GithubSuite::new_self_managed("scenario_session_lifecycle_journey", repo_full_name)
        .setup(setup_fn(
            config.gh_org.clone(),
            config.gh_token.clone(),
            config.app_id.clone(),
            config.app_client_id.clone(),
            config.app_installation_id.clone(),
            config.app_private_key_file.clone(),
        ))
        .case("claude_dir_assembly_e2e", claude_dir_assembly_fn())
        .case("credential_relay_e2e", credential_relay_fn())
        .case("finalization_lifecycle_e2e", finalization_lifecycle_fn())
        .case("work_item_lock_e2e", work_item_lock_fn())
        .case("session_list_e2e", session_list_fn())
        .case("session_finalize_e2e", session_finalize_fn())
        .case("failed_finalization_e2e", failed_finalization_fn())
        .case("meetings_finalization_e2e", meetings_finalization_fn())
}

pub fn scenario(config: &E2eConfig) -> Trial {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let repo_full_name = format!("{}/bm-e2e-slj-{}", config.gh_org, timestamp);
    build_suite(config, &repo_full_name).build(config)
}

pub fn scenario_progressive(config: &E2eConfig) -> Trial {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let repo_full_name = format!("{}/bm-e2e-slj-{}", config.gh_org, timestamp);
    build_suite(config, &repo_full_name).build_progressive(config)
}
