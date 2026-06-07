//! Telegram Operator Journey
//!
//! Exercises the Telegram (external) bridge flow as a lighter scenario:
//! init -> hire -> identity add -> provision workspace -> start -> verify env vars -> stop -> cleanup
//!
//! No daemon tests, no per-member start/stop, no idempotency second pass.
//! Just the Telegram-specific bridge lifecycle.
//!
//! Requires tg-mock (Podman). Skipped if Podman is not available.

use std::fs;
use std::time::Duration;

use libtest_mimic::Trial;

use bm::workspace;
use bm::profile::CodingAgentDef;

use super::super::helpers::{
    cleanup_project_boards, find_session_workspace, list_session_workspaces,
    wait_for_new_session_workspace, read_stub_pid, wait_for_stub_pid, wait_for_exit,
    E2eConfig, GithubSuite, ProcessGuard,
};
use super::super::telegram;
use super::super::test_env::TestEnv;

// ── Constants ─────────────────────────────────────────────────────────

const TEAM_NAME: &str = "e2e-tg";
const PROFILE: &str = "agentic-sdlc-minimal";
const ROLE: &str = "engineer";
const MEMBER_NAME: &str = "tg-alice";
const MEMBER_DIR: &str = "engineer-tg-alice";
const BOT_TOKEN: &str = "123456789:ABCDEFGhijklmnopqrstuvwxyz-e2e";

// ── Reusable case functions ───────────────────────────────────────────

fn init_with_tg_bridge_fn(
    gh_org: String,
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let workzone = env.home.join("workspaces");
        let repo_name = env.repo_full_name.split('/').next_back().unwrap().to_string();
        let board_title = format!("{} Board", TEAM_NAME);

        let output = env.command("bm")
            .args([
                "init", "--non-interactive",
                "--profile", PROFILE,
                "--team-name", TEAM_NAME,
                "--org", &gh_org,
                "--repo", &repo_name,
                "--bridge", "telegram",
                "--github-project-board", &board_title,
                "--workzone", &workzone.to_string_lossy(),
            ])
            .output();
        assert!(output.status.success(), "bm init failed: {}", String::from_utf8_lossy(&output.stderr));

        let team_repo = workzone.join(TEAM_NAME).join("team");
        assert!(team_repo.join(".git").is_dir(), "team repo should have .git");
        assert!(team_repo.join("botminter.yml").exists(), "should have botminter.yml");
    }
}

fn hire_member_fn(
    _gh_token: String,
    app_id: String,
    app_client_id: String,
    app_installation_id: String,
    app_private_key_file: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args([
                "hire", ROLE, "--name", MEMBER_NAME, "-t", TEAM_NAME,
                "--reuse-app",
                "--app-id", &app_id,
                "--client-id", &app_client_id,
                "--private-key-file", &app_private_key_file,
                "--installation-id", &app_installation_id,
            ])
            .run();
        assert!(stdout.contains(MEMBER_DIR) || stdout.contains(MEMBER_NAME));
    }
}

fn bridge_identity_add_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["bridge", "identity", "add", MEMBER_DIR, "-t", TEAM_NAME])
            .env(
                &format!("BM_BRIDGE_TOKEN_{}", MEMBER_DIR.to_uppercase().replace('-', "_")),
                BOT_TOKEN,
            )
            .run();
        assert!(stdout.contains(MEMBER_DIR));

        // Verify token was stored
        let list_out = env.command("bm")
            .args(["bridge", "identity", "list", "-t", TEAM_NAME])
            .run();
        assert!(list_out.contains(MEMBER_DIR), "identity should appear in list after add");
    }
}

fn provision_workspace_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let team_dir = env.home.join("workspaces").join(TEAM_NAME);
        let team_repo = team_dir.join("team");

        let coding_agent = CodingAgentDef {
            name: "claude-code".to_string(),
            display_name: "Claude Code".to_string(),
            context_file: "CLAUDE.md".to_string(),
            agent_dir: ".claude".to_string(),
            binary: "claude".to_string(),
            system_prompt_flag: Some("--append-system-prompt-file".to_string()),
            skip_permissions_flag: Some("--dangerously-skip-permissions".to_string()),
        };

        let manifest_path = team_repo.join("botminter.yml");
        let projects: Vec<(String, String)> = if manifest_path.exists() {
            let contents = fs::read_to_string(&manifest_path).unwrap();
            let manifest: serde_yml::Value = serde_yml::from_str(&contents).unwrap();
            manifest["projects"]
                .as_sequence()
                .map(|ps| {
                    ps.iter()
                        .filter_map(|p| {
                            let name = p["name"].as_str()?;
                            let url = p["fork_url"].as_str()?;
                            Some((name.to_string(), url.to_string()))
                        })
                        .collect()
                })
                .unwrap_or_default()
        } else {
            Vec::new()
        };
        let project_refs: Vec<(&str, &str)> = projects
            .iter()
            .map(|(n, u)| (n.as_str(), u.as_str()))
            .collect();

        let params = workspace::WorkspaceRepoParams {
            team_repo_path: &team_repo,
            workspace_base: &team_dir,
            member_dir_name: MEMBER_DIR,
            team_name: TEAM_NAME,
            projects: &project_refs,
            github_repo: None,
            project_number: None,
            push: false,
            coding_agent: &coding_agent,
            remote_ops: None,
            team_submodule_url: None,
        };

        workspace::create_workspace_repo(&params).unwrap();

        let ws = team_dir.join(MEMBER_DIR);
        assert!(ws.join(".botminter.workspace").exists());
        assert!(ws.join("ralph.yml").exists());

        workspace::inject_robot_enabled(&ws.join("ralph.yml"), true).unwrap();
    }
}

fn start_and_verify_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        // Snapshot existing session workspaces before start (for deduplication in wait below)
        let before: std::collections::HashSet<_> = list_session_workspaces(
            &env.home, TEAM_NAME, MEMBER_DIR,
        ).into_iter().collect();

        let mut guard = ProcessGuard::new(env, TEAM_NAME);
        let mut cmd = env.command("bm");
        cmd.args(["start", "-t", TEAM_NAME]);
        if let Some(url) = env.get_export("tg_mock_url") {
            cmd.env("RALPH_TELEGRAM_API_URL", &url)
                .env("RALPH_TELEGRAM_BOT_TOKEN", BOT_TOKEN);
        }
        let stdout = cmd.run();
        assert!(stdout.contains("Started 1 member"), "bm start output: {}", stdout);

        // Find the ephemeral session workspace the daemon created for this start.
        // The workspace lives at ~/.botminter/sessions/<team>/<member>/<session_id>/
        let session_ws = wait_for_new_session_workspace(
            &env.home, TEAM_NAME, MEMBER_DIR, &before,
            Duration::from_secs(15),
        ).expect("session workspace should appear after bm start");

        // Wait for stub-ralph to write its PID file into the session workspace
        let pid = wait_for_stub_pid(&session_ws, Duration::from_secs(10))
            .expect("stub-ralph should write .ralph-stub-pid after starting");
        guard.set_pid(pid);

        // Verify Telegram env vars captured by stub-ralph in the session workspace
        if env.get_export("tg_mock_url").is_some() {
            // stub-ralph.sh polls for .ralph-stub-ignore-sigterm for up to 5 s before writing
            // .ralph-stub-env — poll for the file rather than sleeping a fixed duration
            let env_file = session_ws.join(".ralph-stub-env");
            let deadline = std::time::Instant::now() + Duration::from_secs(10);
            while !env_file.exists() && std::time::Instant::now() < deadline {
                std::thread::sleep(Duration::from_millis(200));
            }
            let env_content = fs::read_to_string(&env_file)
                .expect(".ralph-stub-env must exist in session workspace");
            assert!(
                env_content.contains("RALPH_TELEGRAM_API_URL="),
                "stub env should contain RALPH_TELEGRAM_API_URL:\n{}", env_content
            );
            assert!(
                env_content.contains(&format!("RALPH_TELEGRAM_BOT_TOKEN={}", BOT_TOKEN)),
                "stub env should contain RALPH_TELEGRAM_BOT_TOKEN:\n{}", env_content
            );
            assert!(
                env_content.contains("GH_CONFIG_DIR="),
                "stub env should contain GH_CONFIG_DIR (App credential path):\n{}", env_content
            );
            let tg_response = fs::read_to_string(session_ws.join(".ralph-stub-tg-response"))
                .expect(".ralph-stub-tg-response must exist in session workspace");
            assert!(
                tg_response.contains("ok"),
                "stub should have received ok from tg-mock: {}", tg_response
            );
        }

        std::mem::forget(guard);
    }
}

fn stop_fn() -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        // Read stub PID from the session workspace before stopping so we can wait for exit
        let pid_before = find_session_workspace(&env.home, TEAM_NAME, MEMBER_DIR)
            .and_then(|ws| read_stub_pid(&ws));

        let stdout = env.command("bm")
            .args(["stop", "-t", TEAM_NAME])
            .run();
        assert!(stdout.contains("Stopped 1 member"), "bm stop output: {}", stdout);
        if let Some(pid) = pid_before {
            wait_for_exit(pid, Duration::from_secs(5));
        }
    }
}

// ── Scenario construction ────────────────────────────────────────────

fn build_suite(gh_org: String, gh_token: String, config: &E2eConfig) -> GithubSuite {
    let app_id = config.app_id.clone();
    let app_client_id = config.app_client_id.clone();
    let app_installation_id = config.app_installation_id.clone();
    let app_private_key_file = config.app_private_key_file.clone();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let repo_full_name = format!("{}/bm-e2e-tg-{}", gh_org, timestamp);

    GithubSuite::new_self_managed("scenario_tg_operator_journey", &repo_full_name)
        .setup({
            move |env: &mut TestEnv| {
                // Start tg-mock if podman is available
                if let Some(url) = env.get_export("tg_mock_url") {
                    // Check if the container from a previous progressive run is still alive
                    if let Some(cid) = env.get_export("tg_mock_container_id") {
                        let mock = telegram::TgMock::from_existing(
                            cid.to_string(),
                            url.rsplit(':').next().unwrap().parse().unwrap(),
                        );
                        if mock.is_running() {
                            eprintln!("tg-mock already running, reusing");
                            std::mem::forget(mock);
                            return;
                        }
                        drop(mock);
                    }
                }
                if telegram::podman_available() {
                    let mock = telegram::TgMock::start();
                    env.export("tg_mock_url", &mock.api_url());
                    let (container_id, _) = mock.into_parts();
                    env.export("tg_mock_container_id", &container_id);
                } else {
                    eprintln!("SKIP tg-mock: podman not available");
                }
            }
        })
        .case("01_init_with_tg_bridge", init_with_tg_bridge_fn(gh_org.clone(), gh_token.clone()))
        .case("02_hire_member", hire_member_fn(gh_token.clone(), app_id.clone(), app_client_id.clone(), app_installation_id.clone(), app_private_key_file.clone()))
        .case("03_bridge_identity_add", bridge_identity_add_fn(gh_token.clone()))
        .case("04_provision_workspace", provision_workspace_fn(gh_token.clone()))
        .case("05_start_and_verify", start_and_verify_fn(gh_token.clone()))
        .case("06_stop", stop_fn())
        // ── Cleanup ──────────────────────────────────────────────────
        .case("cleanup", {
            let gh_org_c = gh_org.clone();
            let gh_token_c = gh_token.clone();
            move |env: &mut TestEnv| {
                eprintln!("TG journey cleanup...");
                // Stop tg-mock container
                if let Some(cid) = env.get_export("tg_mock_container_id") {
                    let _ = env.command("podman")
                        .args(["stop", "-t", "2", cid])
                        .output();
                    let _ = env.command("podman")
                        .args(["rm", "-f", cid])
                        .output();
                }
                // Delete workspace repo
                let ws_repo = format!("{}/{}-{}", gh_org_c, TEAM_NAME, MEMBER_DIR);
                let _ = env.command("gh")
                    .args(["repo", "delete", &ws_repo, "--yes"])
                    .output();
                // Delete team repo
                let _ = env.command("gh")
                    .args(["repo", "delete", &env.repo_full_name, "--yes"])
                    .output();
                cleanup_project_boards(&gh_org_c, &gh_token_c, TEAM_NAME);
            }
        })
        // Group start + stop (cases 4-5, 0-indexed)
        .group(4, 5)
}

pub fn scenario(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone(), config).build(config)
}

pub fn scenario_progressive(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone(), config).build_progressive(config)
}
