//! Unified Operator Journey
//!
//! One suite, two passes: fresh start -> daemon -> reset HOME -> same journey again.
//! The second pass exercises idempotency -- `bm init` hits an existing repo.
//! Cases that expect failure on second pass use `case_expect_error`.
//!
//! The main journey exercises the Tuwunel (Matrix) bridge -- a local bridge
//! whose lifecycle is managed by Podman. Bridge-dependent steps are skipped
//! gracefully when Podman is not available.

use std::fs;
use std::time::Duration;

use bm::profile;
use libtest_mimic::Trial;

use super::super::helpers::{
    cleanup_project_boards, find_free_port, force_kill, is_alive,
    read_pid_from_state, repo_from_config,
    DaemonGuard, E2eConfig, GithubSuite, ProcessGuard,
};
use super::super::telegram;
use super::super::test_env::TestEnv;
use super::super::tuwunel::TuwunelGuard;

// ── Constants ─────────────────────────────────────────────────────────

const TEAM_NAME: &str = "e2e-fresh";
const PROFILE: &str = "scrum-compact";
const ROLE: &str = "superman";
const MEMBER_NAME: &str = "alice";
const MEMBER_DIR: &str = "superman-alice";

// ── Reusable case functions ───────────────────────────────────────────
//
// Each function takes captured values and returns a closure suitable for .case().
// This allows the same logic to be registered in both passes with different names.

fn init_with_bridge_fn(gh_org: String, _gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let workzone = env.home.join("workspaces");
        let repo_name = env.repo_full_name.split('/').next_back().unwrap().to_string();

        // Use a board title that does NOT match the "{team_name} Board" convention.
        // On first pass, the board doesn't exist and gets created.
        // On second pass (after reset_home), the board exists and gets found by title.
        let board_title = env.get_export("board_title")
            .map(|s| s.to_string())
            .unwrap_or_else(|| {
                let ts = repo_name.split('-').next_back().unwrap_or("0");
                format!("e2e-board-{}", ts)
            });
        env.export("board_title", &board_title);
        env.save();

        let output = env.command("bm")
            .args([
                "init", "--non-interactive",
                "--profile", PROFILE,
                "--team-name", TEAM_NAME,
                "--org", &gh_org,
                "--repo", &repo_name,
                "--bridge", "tuwunel",
                "--github-project-board", &board_title,
                "--workzone", &workzone.to_string_lossy(),
            ])
            .output();
        assert!(output.status.success(), "bm init failed: {}", String::from_utf8_lossy(&output.stderr));
        let stderr = String::from_utf8_lossy(&output.stderr).to_string();

        // When a bridge is selected, next-steps should mention --all (not just --repos)
        assert!(
            stderr.contains("--all"),
            "init next-steps should mention '--all' when bridge is selected, got: {}",
            stderr
        );

        let team_repo = workzone.join(TEAM_NAME).join("team");
        assert!(team_repo.join(".git").is_dir(), "team repo should have .git");
        assert!(team_repo.join("botminter.yml").exists(), "should have botminter.yml");
        assert!(team_repo.join("PROCESS.md").exists(), "should have PROCESS.md");

        // Verify bridge directory exists in team repo
        assert!(
            team_repo.join("bridges/tuwunel/bridge.yml").exists(),
            "should have tuwunel bridge.yml"
        );
        assert!(
            team_repo.join("bridges/tuwunel/Justfile").exists(),
            "should have tuwunel Justfile"
        );

        // Verify botminter.yml has bridge: tuwunel
        let manifest_content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
        assert!(
            manifest_content.contains("bridge: tuwunel"),
            "botminter.yml should declare tuwunel bridge"
        );

        let repo = repo_from_config(&env.home);
        let labels = super::super::github::list_labels(&repo);
        let profiles_base = env.home.join(".config/botminter/profiles");
        let manifest = profile::read_manifest_from(PROFILE, &profiles_base).unwrap();
        for expected in &manifest.labels {
            assert!(labels.contains(&expected.name), "Label '{}' missing", expected.name);
        }
    }
}

fn hire_member_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["hire", ROLE, "--name", MEMBER_NAME, "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains(MEMBER_DIR) || stdout.contains(MEMBER_NAME));
    }
}

fn projects_add_fn(gh_org: String, _gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        // Read existing project URL from team repo if available (second pass)
        let team_repo = env.home.join("workspaces").join(TEAM_NAME).join("team");
        let manifest_path = team_repo.join("botminter.yml");
        let existing_url = if manifest_path.exists() {
            let contents = fs::read_to_string(&manifest_path).unwrap();
            let manifest: serde_yml::Value = serde_yml::from_str(&contents).unwrap();
            manifest["projects"].as_sequence()
                .and_then(|ps| ps.first())
                .and_then(|p| p["fork_url"].as_str().map(String::from))
        } else {
            None
        };

        let project_url = if let Some(url) = existing_url {
            url
        } else {
            // First pass -- create a GitHub repo for the project
            let fork = env.home.join("test-project");
            fs::create_dir_all(&fork).unwrap();
            env.command("git").args(["init", "-b", "main"]).current_dir(&fork).run();
            env.command("git").args(["config", "user.email", "e2e@test"]).current_dir(&fork).run();
            env.command("git").args(["config", "user.name", "E2E"]).current_dir(&fork).run();
            fs::write(fork.join("README.md"), "# test").unwrap();
            env.command("git").args(["add", "-A"]).current_dir(&fork).run();
            env.command("git").args(["commit", "-m", "init"]).current_dir(&fork).run();

            let full_name = format!("{}/bm-e2e-project-{}", gh_org,
                std::time::SystemTime::now().duration_since(std::time::UNIX_EPOCH).unwrap().as_secs());
            env.command("gh")
                .args(["repo", "create", &full_name, "--private", "--source", ".", "--push"])
                .current_dir(&fork)
                .run();
            format!("https://github.com/{}.git", full_name)
        };

        let project_name = bm::git::derive_project_name(&project_url);

        env.command("bm")
            .args(["projects", "add", &project_url, "-t", TEAM_NAME])
            .run();

        let repo = repo_from_config(&env.home);
        let labels = super::super::github::list_labels(&repo);
        let expected_label = format!("project/{}", project_name);
        assert!(labels.contains(&expected_label), "Label '{}' missing: {:?}", expected_label, labels);
    }
}

fn teams_show_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["teams", "show", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("Bridge:"));
        assert!(stdout.contains(MEMBER_DIR) || stdout.contains(MEMBER_NAME));
    }
}

fn bridge_start_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if !telegram::podman_available() {
            eprintln!("SKIP: podman not available -- bridge steps will be skipped");
            return;
        }

        let port = find_free_port();
        eprintln!("Tuwunel bridge will use port {}", port);

        // Save port for subsequent cases
        env.export("tuwunel_port", &port.to_string());

        env.command("bm")
            .args(["bridge", "start", "-t", TEAM_NAME])
            .env("TUWUNEL_PORT", &port.to_string())
            .run();

        // Verify bridge-state.json
        let bstate_path = env.home
            .join("workspaces")
            .join(TEAM_NAME)
            .join("bridge-state.json");
        assert!(bstate_path.exists(), "bridge-state.json should exist after bridge start");
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        assert_eq!(
            bstate["status"].as_str(),
            Some("running"),
            "bridge-state.json should show status running"
        );

        // Create TuwunelGuard as panic safety net
        let container_name = format!("bm-tuwunel-{}", TEAM_NAME);
        let guard = TuwunelGuard::new(container_name.clone(), port);

        // Save guard info for progressive mode and subsequent cases
        let (name, p) = guard.into_parts();
        env.export("tuwunel_guard_name", &name);
        env.export("tuwunel_guard_port", &p.to_string());
    }
}

fn bridge_start_idempotent_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if env.get_export("tuwunel_port").is_none() {
            eprintln!("SKIP: bridge not started (no podman)");
            return;
        }

        let port = env.get_export("tuwunel_port").unwrap().to_string();

        let stdout = env.command("bm")
            .args(["bridge", "start", "-t", TEAM_NAME])
            .env("TUWUNEL_PORT", &port)
            .run();
        assert!(
            stdout.contains("already running"),
            "re-starting a running bridge should say 'already running', got: {}",
            stdout
        );
    }
}

fn bridge_identity_add_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if env.get_export("tuwunel_port").is_none() {
            eprintln!("SKIP: bridge not started (no podman)");
            return;
        }

        let port = env.get_export("tuwunel_port").unwrap().to_string();

        // Local bridge auto-provisions -- no BM_BRIDGE_TOKEN_ env var needed
        let stdout = env.command("bm")
            .args(["bridge", "identity", "add", MEMBER_DIR, "-t", TEAM_NAME])
            .env("TUWUNEL_PORT", &port)
            .run();
        assert!(stdout.contains(MEMBER_DIR));

        // Verify token was stored by running `bm bridge identity list`
        let list_out = env.command("bm")
            .args(["bridge", "identity", "list", "-t", TEAM_NAME])
            .run();
        assert!(list_out.contains(MEMBER_DIR), "identity should appear in list after add");
    }
}

fn bridge_identity_list_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["bridge", "identity", "list", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains(MEMBER_DIR));
    }
}

fn bridge_identity_show_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["bridge", "identity", "show", MEMBER_DIR, "--reveal", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains(MEMBER_DIR), "should show username");
        assert!(stdout.contains("Token:"), "should show token field");
    }
}

fn bridge_room_create_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if env.get_export("tuwunel_port").is_none() {
            eprintln!("SKIP: bridge not started (no podman)");
            return;
        }

        let port = env.get_export("tuwunel_port").unwrap().to_string();

        env.command("bm")
            .args(["bridge", "room", "create", "e2e-general", "-t", TEAM_NAME])
            .env("TUWUNEL_PORT", &port)
            .run();

        // Verify bridge-state.json has room
        let bstate_path = env.home
            .join("workspaces")
            .join(TEAM_NAME)
            .join("bridge-state.json");
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        let rooms = bstate["rooms"].as_array().expect("rooms should be array");
        assert!(
            !rooms.is_empty(),
            "should have at least one room after room create"
        );
    }
}

fn start_skips_running_bridge_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        // Member is already running from start_status_healthy. Starting again should
        // report "already running" for both the member and the bridge.
        // We do NOT stop the member — subsequent cases (bridge_functional,
        // stop_clean_shutdown) expect it to still be running.
        let mut cmd = env.command("bm");
        cmd.args(["start", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        let stdout = cmd.run();
        // Should say "already running", not "Starting bridge"
        assert!(
            stdout.contains("already running"),
            "re-start should skip running bridge, got: {}",
            stdout
        );
    }
}

fn start_single_member_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);
        let mut cmd = env.command("bm");
        cmd.args(["start", MEMBER_DIR, "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        let stdout = cmd.run();
        assert!(stdout.contains("Started 1 member"), "should start exactly 1 member, got: {}", stdout);
        // Should NOT mention bridge (single member skips bridge lifecycle)
        assert!(!stdout.contains("Starting bridge") && !stdout.contains("Bridge") ,
            "single member start should skip bridge, got: {}", stdout);

        if let Some(pid) = read_pid_from_state(&env.home) { guard.set_pid(pid); }
        std::mem::forget(guard);
    }
}

fn stop_single_member_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let pid_before = read_pid_from_state(&env.home);
        let stdout = env.command("bm")
            .args(["stop", MEMBER_DIR, "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("Stopped 1 member"), "should stop exactly 1 member, got: {}", stdout);
        // Bridge should NOT be stopped (single member stop skips bridge)
        assert!(!stdout.contains("Stopping bridge"), "single member stop should skip bridge, got: {}", stdout);
        if let Some(pid) = pid_before {
            super::super::helpers::wait_for_exit(pid, Duration::from_secs(5));
            assert!(!is_alive(pid));
        }
    }
}

fn sync_bridge_and_repos_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut cmd = env.command("bm");
        cmd.args(["teams", "sync", "--bridge", "--repos", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
            // NOTE: do NOT apply_real_dbus_env here -- sync only runs
            // `just health` (a curl) and needs the isolated D-Bus for
            // keyring credential lookup.
        }
        let stdout = cmd.run();
        assert!(!stdout.contains("No bridge configured"));

        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        assert!(ws.join(".botminter.workspace").exists());
        assert!(ws.join("team").is_dir());
        for file in ["PROMPT.md", "AGENTS.md", "ralph.yml"] {
            assert!(ws.join(file).exists(), "{} missing", file);
        }

        // If bridge is running, verify ralph.yml has RObot.matrix config
        if env.get_export("tuwunel_port").is_some() {
            let ralph_contents = fs::read_to_string(ws.join("ralph.yml")).unwrap();
            let ralph_doc: serde_yml::Value = serde_yml::from_str(&ralph_contents).unwrap();

            assert_eq!(
                ralph_doc["RObot"]["enabled"].as_bool(),
                Some(true),
                "RObot.enabled should be true"
            );
            assert!(
                ralph_doc["RObot"]["matrix"]["homeserver_url"].as_str().is_some(),
                "RObot.matrix.homeserver_url should be set"
            );
            assert!(
                ralph_doc["RObot"]["matrix"]["room_id"].as_str().is_some(),
                "RObot.matrix.room_id should be set"
            );
        }
    }
}

fn sync_idempotent_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut cmd = env.command("bm");
        cmd.args(["teams", "sync", "--bridge", "--repos", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        cmd.run();

        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        assert!(ws.join(".botminter.workspace").exists());
        assert!(ws.join("PROMPT.md").exists());
    }
}

fn projects_sync_fn(gh_org: String, gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["projects", "sync", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("Status field synced"));

        let _projects = bm::git::list_projects(&gh_token, &gh_org)
            .expect("list_gh_projects should succeed");

        // Idempotency
        let stdout2 = env.command("bm")
            .args(["projects", "sync", "-t", TEAM_NAME])
            .run();
        assert!(stdout2.contains("Status field synced"));
    }
}

fn start_without_ralph_errors_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stderr = env.command("bm")
            .args(["start", "-t", TEAM_NAME])
            .env("PATH", "/usr/bin:/bin:/usr/sbin:/sbin")
            .run_fail();
        // With a local bridge, the error may be "ralph not found" OR
        // "no workspace found" (if sync hasn't run yet) OR a bridge
        // health failure. All are valid pre-start errors.
        assert!(
            (stderr.contains("ralph") && stderr.contains("not found"))
                || stderr.contains("no workspace found")
                || stderr.contains("Bridge recipe"),
            "start should fail with a meaningful error, got: {}",
            stderr
        );
    }
}

fn start_status_healthy_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);
        let mut cmd = env.command("bm");
        cmd.args(["start", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        let stdout = cmd.run();
        assert!(stdout.contains("Started 1 member"));

        if let Some(pid) = read_pid_from_state(&env.home) { guard.set_pid(pid); }

        let stdout = env.command("bm")
            .args(["status", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("running") && stdout.contains(MEMBER_DIR));

        std::mem::forget(guard);
    }
}

fn bridge_functional_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if env.get_export("tuwunel_port").is_none() {
            eprintln!("SKIP: bridge not started (no podman)");
            return;
        }
        std::thread::sleep(Duration::from_secs(3));
        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let env_content = fs::read_to_string(ws.join(".ralph-stub-env")).unwrap();
        assert!(env_content.contains("RALPH_MATRIX_ACCESS_TOKEN="),
            "stub env should contain RALPH_MATRIX_ACCESS_TOKEN, got: {}", env_content);
        assert!(env_content.contains("RALPH_MATRIX_HOMESERVER_URL="),
            "stub env should contain RALPH_MATRIX_HOMESERVER_URL, got: {}", env_content);
        assert!(env_content.contains("GH_TOKEN="),
            "stub env should contain GH_TOKEN, got: {}", env_content);

        // Verify the stub successfully contacted the Matrix homeserver
        let matrix_response = fs::read_to_string(ws.join(".ralph-stub-matrix-response")).unwrap();
        assert!(matrix_response.contains("versions"),
            "stub should have received a valid /_matrix/client/versions response, got: {}", matrix_response);
    }
}

fn stop_clean_shutdown_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let pid_before = read_pid_from_state(&env.home);
        let stdout = env.command("bm")
            .args(["stop", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("Stopped 1 member"));
        if let Some(pid) = pid_before {
            super::super::helpers::wait_for_exit(pid, Duration::from_secs(5));
            assert!(!is_alive(pid));
        }
    }
}

fn stop_force_kills_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut guard = ProcessGuard::new(env, TEAM_NAME);
        let mut cmd = env.command("bm");
        cmd.args(["start", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        cmd.run();
        let pid = read_pid_from_state(&env.home).expect("should have PID");
        guard.set_pid(pid);
        env.command("bm")
            .args(["stop", "--force", "-t", TEAM_NAME])
            .run();
        super::super::helpers::wait_for_exit(pid, Duration::from_secs(5));
        assert!(!is_alive(pid));
    }
}

fn status_detects_crashed_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let mut cmd = env.command("bm");
        cmd.args(["start", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        cmd.run();
        let pid = read_pid_from_state(&env.home).expect("should have PID");
        force_kill(pid);
        super::super::helpers::wait_for_exit(pid, Duration::from_secs(5));
        let stdout = env.command("bm")
            .args(["status", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("crashed"));
    }
}

fn members_list_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["members", "list", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains(MEMBER_DIR) && stdout.contains(ROLE));
    }
}

fn teams_list_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["teams", "list"])
            .run();
        let repo = repo_from_config(&env.home);
        assert!(stdout.contains(&repo), "teams list should show repo '{}'", repo);
    }
}

fn daemon_start_poll_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let _ = fs::remove_file(ws.join(".ralph-stub-pid"));
        let _ = fs::remove_file(ws.join(".ralph-stub-env"));
        let _ = fs::remove_file(ws.join(".ralph-stub-matrix-response"));

        // Pre-seed poll state with the current latest event ID so the daemon
        // doesn't treat pre-existing GitHub events (from the first pass or
        // previous cases) as new activity.
        let latest_event_id = env.command("gh")
            .args(["api", &format!("repos/{}/events", env.repo_full_name),
                   "--jq", ".[0].id"])
            .output();
        let latest_event_id = if latest_event_id.status.success() {
            let id = String::from_utf8_lossy(&latest_event_id.stdout).trim().to_string();
            if id.is_empty() { None } else { Some(id) }
        } else { None };
        if let Some(ref event_id) = latest_event_id {
            let poll_state_dir = env.home.join(".botminter");
            fs::create_dir_all(&poll_state_dir).unwrap();
            let poll_state = serde_json::json!({
                "last_event_id": event_id,
                "last_poll_at": chrono::Utc::now().to_rfc3339()
            });
            fs::write(
                poll_state_dir.join(format!("daemon-{}-poll.json", TEAM_NAME)),
                serde_json::to_string_pretty(&poll_state).unwrap(),
            ).unwrap();
        }

        let mut cmd = env.command("bm");
        cmd.args(["daemon", "start", "--mode", "poll", "--interval", "2", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        let out = cmd.run();
        assert!(out.contains("Daemon started"));

        let out = env.command("bm")
            .args(["daemon", "status", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("running") && out.contains("poll"));

        assert!(!ws.join(".ralph-stub-pid").exists(), "Ralph should NOT be running before any GH event");
    }
}

fn daemon_poll_launches_member_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        env.command("gh")
            .args(["issue", "create", "-R", &env.repo_full_name,
                "--title", "Trigger daemon member launch", "--body", "E2E test trigger"])
            .run();

        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let stub_pid_file = ws.join(".ralph-stub-pid");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !stub_pid_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(stub_pid_file.exists(), "Daemon did not launch member within 30s");
        let stub_pid: u32 = fs::read_to_string(&stub_pid_file).unwrap().trim().parse().unwrap();
        assert!(is_alive(stub_pid));

        let daemon_log = env.home.join(format!(".botminter/logs/daemon-{}.log", TEAM_NAME));
        assert!(daemon_log.exists());
    }
}

fn daemon_stop_poll_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let pid_file = env.home.join(format!(".botminter/daemon-{}.pid", TEAM_NAME));
        let daemon_pid: u32 = fs::read_to_string(&pid_file).expect("daemon PID file").trim().parse().unwrap();
        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let stub_pid: Option<u32> = fs::read_to_string(ws.join(".ralph-stub-pid")).ok().and_then(|s| s.trim().parse().ok());

        let out = env.command("bm")
            .args(["daemon", "stop", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("Daemon stopped"));

        super::super::helpers::wait_for_exit(daemon_pid, Duration::from_secs(10));
        assert!(!is_alive(daemon_pid));
        if let Some(pid) = stub_pid {
            super::super::helpers::wait_for_exit(pid, Duration::from_secs(10));
            assert!(!is_alive(pid));
        }
        let _ = fs::remove_file(ws.join(".ralph-stub-pid"));
        let _ = fs::remove_file(ws.join(".ralph-stub-env"));
        let _ = fs::remove_file(ws.join(".ralph-stub-matrix-response"));
    }
}

fn daemon_start_webhook_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let out = env.command("bm")
            .args(["daemon", "start", "--mode", "webhook", "--port", "19500", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("Daemon started"));
        std::thread::sleep(Duration::from_millis(500));
        let out = env.command("bm")
            .args(["daemon", "status", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("running") && out.contains("webhook"));
        std::mem::forget(DaemonGuard::new(env, TEAM_NAME));
    }
}

fn daemon_stop_webhook_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        env.command("bm")
            .args(["daemon", "stop", "-t", TEAM_NAME])
            .run();
        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let _ = fs::remove_file(ws.join(".ralph-stub-pid"));
        let _ = fs::remove_file(ws.join(".ralph-stub-env"));
    }
}

fn daemon_sigkill_escalation_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let ws = env.home.join("workspaces").join(TEAM_NAME).join(MEMBER_DIR);
        let ignore_file = ws.join(".ralph-stub-ignore-sigterm");
        fs::write(&ignore_file, "").unwrap();
        let sigterm_log = ws.join(".ralph-stub-sigterm.log");
        let _ = fs::remove_file(&sigterm_log);
        let _ = fs::remove_file(ws.join(".ralph-stub-pid"));

        let _guard = DaemonGuard::new(env, TEAM_NAME);
        let mut cmd = env.command("bm");
        cmd.args(["daemon", "start", "--mode", "poll", "--interval", "2", "-t", TEAM_NAME]);
        if let Some(port) = env.get_export("tuwunel_port") {
            cmd.env("TUWUNEL_PORT", port);
        }
        cmd.run();

        env.command("gh")
            .args(["issue", "create", "-R", &env.repo_full_name,
                "--title", "Trigger SIGKILL test", "--body", "E2E"])
            .run();

        let stub_pid_file = ws.join(".ralph-stub-pid");
        let deadline = std::time::Instant::now() + Duration::from_secs(30);
        while !stub_pid_file.exists() && std::time::Instant::now() < deadline {
            std::thread::sleep(Duration::from_millis(500));
        }
        assert!(stub_pid_file.exists(), "Daemon should have launched ralph");
        let ralph_pid: u32 = fs::read_to_string(&stub_pid_file).unwrap().trim().parse().unwrap();
        assert!(is_alive(ralph_pid));
        assert!(sigterm_log.exists(), "Ralph should have logged SIGTERM trap setup");

        env.command("bm")
            .args(["daemon", "stop", "-t", TEAM_NAME])
            .run();

        super::super::helpers::wait_for_exit(ralph_pid, Duration::from_secs(10));
        assert!(!is_alive(ralph_pid));
        let log_content = fs::read_to_string(&sigterm_log).unwrap();
        assert!(log_content.contains("SIGTERM received and ignored"));

        let _ = fs::remove_file(&ignore_file);
        let _ = fs::remove_file(&sigterm_log);
        let _ = fs::remove_file(ws.join(".ralph-stub-pid"));
        let _ = fs::remove_file(ws.join(".ralph-stub-env"));
    }
}

fn daemon_stale_pid_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let _guard = DaemonGuard::new(env, TEAM_NAME);
        let pid_dir = env.home.join(".botminter");
        fs::create_dir_all(&pid_dir).unwrap();
        fs::write(pid_dir.join(format!("daemon-{}.pid", TEAM_NAME)), "99999").unwrap();

        let out = env.command("bm")
            .args(["daemon", "start", "--mode", "poll", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("Daemon started"), "Should start despite stale PID: {}", out);

        env.command("bm")
            .args(["daemon", "stop", "-t", TEAM_NAME])
            .run();
    }
}

fn daemon_already_running_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let _guard = DaemonGuard::new(env, TEAM_NAME);
        env.command("bm")
            .args(["daemon", "start", "--mode", "poll", "-t", TEAM_NAME])
            .run();

        let output = env.command("bm")
            .args(["daemon", "start", "--mode", "poll", "-t", TEAM_NAME])
            .output();
        assert!(!output.status.success());
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(stderr.contains("already running"));
    }
}

fn daemon_crashed_detection_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        env.command("bm")
            .args(["daemon", "start", "--mode", "poll", "-t", TEAM_NAME])
            .run();

        let pid_file = env.home.join(format!(".botminter/daemon-{}.pid", TEAM_NAME));
        let daemon_pid: u32 = fs::read_to_string(&pid_file).unwrap().trim().parse().unwrap();
        force_kill(daemon_pid);
        super::super::helpers::wait_for_exit(daemon_pid, Duration::from_secs(5));

        let out = env.command("bm")
            .args(["daemon", "status", "-t", TEAM_NAME])
            .run();
        assert!(out.contains("not running") || out.contains("stale"));
        assert!(!pid_file.exists(), "Stale PID file should be cleaned up");
    }
}

fn bridge_stop_fn(_gh_token: String) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        if env.get_export("tuwunel_port").is_none() {
            eprintln!("SKIP: bridge not started (no podman)");
            return;
        }

        let port = env.get_export("tuwunel_port").unwrap().to_string();

        env.command("bm")
            .args(["bridge", "stop", "-t", TEAM_NAME])
            .env("TUWUNEL_PORT", &port)
            .run();

        // Consume the TuwunelGuard to prevent double-cleanup
        let guard_name = env.get_export("tuwunel_guard_name")
            .expect("tuwunel_guard_name should be exported").to_string();
        let guard_port: u16 = env.get_export("tuwunel_guard_port")
            .expect("tuwunel_guard_port should be exported")
            .parse().unwrap();
        let guard = TuwunelGuard::from_existing(guard_name, guard_port);
        let _ = guard.into_parts();

        // Verify bridge-state.json shows stopped
        let bstate_path = env.home
            .join("workspaces")
            .join(TEAM_NAME)
            .join("bridge-state.json");
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        assert_eq!(
            bstate["status"].as_str(),
            Some("stopped"),
            "bridge-state.json should show status stopped after bridge stop"
        );
    }
}

// ── Scenario construction ────────────────────────────────────────────

fn build_suite(gh_org: String, gh_token: String) -> GithubSuite {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let repo_full_name = format!("{}/bm-e2e-{}", gh_org, timestamp);

    let suite = GithubSuite::new_self_managed("scenario_operator_journey", &repo_full_name)
        .setup({
            move |_env: &mut TestEnv| {
                if !telegram::podman_available() {
                    eprintln!("WARN: podman not available -- bridge-dependent steps will be skipped");
                }
            }
        })
        // ── First pass: fresh start ──────────────────────────────────
        .case("init_with_bridge_fresh", init_with_bridge_fn(gh_org.clone(), gh_token.clone()))
        .case("hire_member_fresh", hire_member_fn(gh_token.clone()))
        .case("projects_add_fresh", projects_add_fn(gh_org.clone(), gh_token.clone()))
        .case("teams_show_fresh", teams_show_fn())
        .case("bridge_start_fresh", bridge_start_fn(gh_token.clone()))
        .case("bridge_start_idempotent_fresh", bridge_start_idempotent_fn(gh_token.clone()))
        .case("bridge_identity_add_fresh", bridge_identity_add_fn(gh_token.clone()))
        .case("bridge_identity_show_fresh", bridge_identity_show_fn())
        .case("bridge_identity_list_fresh", bridge_identity_list_fn())
        .case("bridge_room_create_fresh", bridge_room_create_fn(gh_token.clone()))
        .case("sync_bridge_and_repos_fresh", sync_bridge_and_repos_fn(gh_token.clone()))
        .case("sync_idempotent_fresh", sync_idempotent_fn(gh_token.clone()))
        .case("projects_sync_fresh", projects_sync_fn(gh_org.clone(), gh_token.clone()))
        .case("start_without_ralph_errors_fresh", start_without_ralph_errors_fn())
        .case("start_status_healthy_fresh", start_status_healthy_fn(gh_token.clone()))
        .case("start_skips_running_bridge_fresh", start_skips_running_bridge_fn(gh_token.clone()))
        .case("bridge_functional_fresh", bridge_functional_fn())
        .case("stop_clean_shutdown_fresh", stop_clean_shutdown_fn())
        .case("start_single_member_fresh", start_single_member_fn(gh_token.clone()))
        .case("stop_single_member_fresh", stop_single_member_fn())
        .case("stop_force_kills_fresh", stop_force_kills_fn(gh_token.clone()))
        .case("status_detects_crashed_fresh", status_detects_crashed_fn(gh_token.clone()))
        .case("members_list_fresh", members_list_fn())
        .case("teams_list_fresh", teams_list_fn())
        .case("daemon_start_poll_fresh", daemon_start_poll_fn(gh_token.clone()))
        .case("daemon_poll_launches_member_fresh", daemon_poll_launches_member_fn(gh_token.clone()))
        .case("daemon_stop_poll_fresh", daemon_stop_poll_fn(gh_token.clone()))
        .case("daemon_start_webhook_fresh", daemon_start_webhook_fn(gh_token.clone()))
        .case("daemon_stop_webhook_fresh", daemon_stop_webhook_fn(gh_token.clone()))
        .case("daemon_sigkill_escalation_fresh", daemon_sigkill_escalation_fn(gh_token.clone()))
        .case("daemon_stale_pid_fresh", daemon_stale_pid_fn(gh_token.clone()))
        .case("daemon_already_running_fresh", daemon_already_running_fn(gh_token.clone()))
        .case("daemon_crashed_detection_fresh", daemon_crashed_detection_fn(gh_token.clone()))
        .case("bridge_stop_fresh", bridge_stop_fn(gh_token.clone()))
        // ── Reset HOME ───────────────────────────────────────────────
        .case("reset_home", |env: &mut TestEnv| {
            eprintln!("Wiping HOME for second pass...");

            // Remove old Tuwunel container and volume so the second pass
            // creates a fresh one instead of trying to restart a stale container
            let container_name = format!("bm-tuwunel-{}", TEAM_NAME);
            let _ = env.command("podman")
                .args(["rm", "-f", &container_name])
                .output();
            let volume_name = format!("{}-data", container_name);
            let _ = env.command("podman")
                .args(["volume", "rm", "-f", &volume_name])
                .output();

            // Remove tuwunel exports so second pass re-discovers bridge availability
            env.remove_export("tuwunel_port");
            env.remove_export("tuwunel_guard_name");
            env.remove_export("tuwunel_guard_port");

            env.reset_home();
        })
        // ── Second pass: existing repo ───────────────────────────────
        .case("verify_board_survives_reset", {
            let gh_org = gh_org.clone();
            let gh_token = gh_token.clone();
            move |env: &mut TestEnv| {
                let board_title = env.get_export("board_title")
                    .expect("board_title export should survive reset_home")
                    .to_string();
                let projects = bm::git::list_projects(&gh_token, &gh_org)
                    .expect("list_projects should succeed");
                assert!(
                    projects.iter().any(|(_, t)| t == &board_title),
                    "Project board '{}' should still exist on GitHub after HOME wipe, found: {:?}",
                    board_title, projects
                );
            }
        })
        .case("init_with_bridge_existing", init_with_bridge_fn(gh_org.clone(), gh_token.clone()))
        .case_expect_error("hire_member_existing", hire_member_fn(gh_token.clone()),
            |err| err.contains("already exists"))
        .case_expect_error("projects_add_existing", projects_add_fn(gh_org.clone(), gh_token.clone()),
            |err| err.contains("already exists"))
        .case("teams_show_existing", teams_show_fn())
        .case("bridge_start_existing", bridge_start_fn(gh_token.clone()))
        .case("bridge_start_idempotent_existing", bridge_start_idempotent_fn(gh_token.clone()))
        .case("bridge_identity_add_existing", bridge_identity_add_fn(gh_token.clone()))
        .case("bridge_identity_show_existing", bridge_identity_show_fn())
        .case("bridge_identity_list_existing", bridge_identity_list_fn())
        .case("bridge_room_create_existing", bridge_room_create_fn(gh_token.clone()))
        .case("sync_bridge_and_repos_existing", sync_bridge_and_repos_fn(gh_token.clone()))
        .case("sync_idempotent_existing", sync_idempotent_fn(gh_token.clone()))
        .case("projects_sync_existing", projects_sync_fn(gh_org.clone(), gh_token.clone()))
        .case("start_without_ralph_errors_existing", start_without_ralph_errors_fn())
        .case("start_status_healthy_existing", start_status_healthy_fn(gh_token.clone()))
        .case("start_skips_running_bridge_existing", start_skips_running_bridge_fn(gh_token.clone()))
        .case("bridge_functional_existing", bridge_functional_fn())
        .case("stop_clean_shutdown_existing", stop_clean_shutdown_fn())
        .case("start_single_member_existing", start_single_member_fn(gh_token.clone()))
        .case("stop_single_member_existing", stop_single_member_fn())
        .case("stop_force_kills_existing", stop_force_kills_fn(gh_token.clone()))
        .case("status_detects_crashed_existing", status_detects_crashed_fn(gh_token.clone()))
        .case("members_list_existing", members_list_fn())
        .case("teams_list_existing", teams_list_fn())
        .case("daemon_start_poll_existing", daemon_start_poll_fn(gh_token.clone()))
        .case("daemon_poll_launches_member_existing", daemon_poll_launches_member_fn(gh_token.clone()))
        .case("daemon_stop_poll_existing", daemon_stop_poll_fn(gh_token.clone()))
        .case("daemon_start_webhook_existing", daemon_start_webhook_fn(gh_token.clone()))
        .case("daemon_stop_webhook_existing", daemon_stop_webhook_fn(gh_token.clone()))
        .case("daemon_sigkill_escalation_existing", daemon_sigkill_escalation_fn(gh_token.clone()))
        .case("daemon_stale_pid_existing", daemon_stale_pid_fn(gh_token.clone()))
        .case("daemon_already_running_existing", daemon_already_running_fn(gh_token.clone()))
        .case("daemon_crashed_detection_existing", daemon_crashed_detection_fn(gh_token.clone()))
        .case("bridge_stop_existing", bridge_stop_fn(gh_token.clone()))
        // ── Cleanup ──────────────────────────────────────────────────
        .case("cleanup", {
            let gh_org_c = gh_org.clone();
            let gh_token_c = gh_token.clone();
            move |env: &mut TestEnv| {
                eprintln!("Final cleanup...");
                // Force-remove Tuwunel container and volume if still around
                let container_name = format!("bm-tuwunel-{}", TEAM_NAME);
                let _ = env.command("podman")
                    .args(["rm", "-f", &container_name])
                    .output();
                let volume_name = format!("{}-data", container_name);
                let _ = env.command("podman")
                    .args(["volume", "rm", "-f", &volume_name])
                    .output();
                // Delete workspace repo
                let ws_repo = format!("{}/{}-{}", gh_org_c, TEAM_NAME, MEMBER_DIR);
                let _ = env.command("gh")
                    .args(["repo", "delete", &ws_repo, "--yes"])
                    .output();
                // Delete project repo (read URL from team repo manifest)
                let manifest_path = env.home.join("workspaces").join(TEAM_NAME).join("team/botminter.yml");
                if let Ok(contents) = fs::read_to_string(&manifest_path) {
                    if let Ok(manifest) = serde_yml::from_str::<serde_yml::Value>(&contents) {
                        if let Some(projects) = manifest["projects"].as_sequence() {
                            for proj in projects {
                                if let Some(url) = proj["fork_url"].as_str() {
                                    let repo_name = url.trim_start_matches("https://github.com/")
                                        .trim_end_matches(".git");
                                    let _ = env.command("gh")
                                        .args(["repo", "delete", repo_name, "--yes"])
                                        .output();
                                }
                            }
                        }
                    }
                }
                // Delete team repo
                let _ = env.command("gh")
                    .args(["repo", "delete", &env.repo_full_name, "--yes"])
                    .output();
                cleanup_project_boards(&gh_org_c, &gh_token_c, TEAM_NAME);
                cleanup_project_boards(&gh_org_c, &gh_token_c, "e2e-board-");
            }
        });

    // Groups: bridge start through bridge_functional in both passes, webhook start→stop in both
    // First pass case indices (0-indexed):
    //   0: init, 1: hire, 2: projects_add, 3: teams_show
    //   4: bridge_start, 5: bridge_start_idempotent
    //   6: bridge_identity_add, 7: bridge_identity_show, 8: bridge_identity_list
    //   9: bridge_room_create
    //   10: sync_bridge_and_repos, 11: sync_idempotent, 12: projects_sync
    //   13: start_without_ralph_errors
    //   14: start_status_healthy, 15: start_skips_running, 16: bridge_functional
    //   17: stop_clean_shutdown
    //   18: start_single_member, 19: stop_single_member
    //   20: stop_force_kills, 21: status_detects_crashed
    //   22: members_list, 23: teams_list
    //   24: daemon_start_poll, 25: daemon_poll_launches, 26: daemon_stop_poll
    //   27: daemon_start_webhook, 28: daemon_stop_webhook
    //   29: daemon_sigkill_escalation, 30: daemon_stale_pid
    //   31: daemon_already_running, 32: daemon_crashed_detection
    //   33: bridge_stop
    //   34: reset_home
    // Second pass offset = 35
    suite
        .group(14, 16).group(27, 28)
        .group(14 + 35, 16 + 35).group(27 + 35, 28 + 35)
}

pub fn scenario(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone()).build(config)
}

pub fn scenario_progressive(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone()).build_progressive(config)
}
