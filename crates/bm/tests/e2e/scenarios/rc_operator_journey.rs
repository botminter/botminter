//! Rocket.Chat Operator Journey
//!
//! Exercises the full operator lifecycle with a Rocket.Chat bridge:
//! init -> hire -> bridge start -> identity add -> room create -> provision workspace -> health -> stop
//!
//! Requires Podman to be available. The suite is skipped if Podman is not installed.

use std::fs;

use libtest_mimic::Trial;

use bm::workspace;
use bm::profile::CodingAgentDef;

use super::super::helpers::{
    cleanup_project_boards, find_free_port,
    E2eConfig, GithubSuite,
};
use super::super::rocketchat::RcPodGuard;
use super::super::telegram;
use super::super::test_env::TestEnv;

// ── Constants ─────────────────────────────────────────────────────────

const TEAM_NAME: &str = "e2e-rc";
const PROFILE: &str = "agentic-sdlc-minimal";
const ROLE: &str = "engineer";
const MEMBER_NAME: &str = "bot-alice";
const MEMBER_DIR: &str = "engineer-bot-alice";

// ── Reusable case functions ───────────────────────────────────────────

fn init_with_rc_bridge_fn(
    gh_org: String,
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let workzone = env.home.join("workspaces");
        let repo_name = env.repo_full_name.split('/').next_back().unwrap().to_string();
        let board_title = format!("{} Board", TEAM_NAME);

        env.command("bm")
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
                "--bridge",
                "rocketchat",
                "--github-project-board",
                &board_title,
                "--workzone",
                &workzone.to_string_lossy(),
            ])
            .run();

        let team_repo = workzone.join(TEAM_NAME).join("team");
        assert!(
            team_repo.join(".git").is_dir(),
            "team repo should have .git"
        );
        assert!(
            team_repo.join("botminter.yml").exists(),
            "should have botminter.yml"
        );

        // Verify bridge directory exists in team repo
        assert!(
            team_repo.join("bridges/rocketchat/bridge.yml").exists(),
            "should have rocketchat bridge.yml"
        );
        assert!(
            team_repo.join("bridges/rocketchat/Justfile").exists(),
            "should have rocketchat Justfile"
        );

        // Verify botminter.yml has bridge: rocketchat
        let manifest = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
        assert!(
            manifest.contains("bridge: rocketchat"),
            "botminter.yml should declare rocketchat bridge"
        );
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
        assert!(
            stdout.contains(MEMBER_DIR) || stdout.contains(MEMBER_NAME),
            "hire output should mention member"
        );
    }
}

fn bridge_start_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        // Find a free port for RC
        let port = find_free_port();
        eprintln!("RC bridge will use port {}", port);

        // Save port for subsequent cases
        env.export("rc_port", &port.to_string());

        env.command("bm")
            .args(["bridge", "start", "-t", TEAM_NAME])
            .env("RC_PORT", &port.to_string())
            .run();

        // Verify bridge-state.json
        let bstate_path = env
            .home
            .join("workspaces")
            .join(TEAM_NAME)
            .join("bridge-state.json");
        assert!(
            bstate_path.exists(),
            "bridge-state.json should exist after bridge start"
        );
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        assert_eq!(
            bstate["status"].as_str(),
            Some("running"),
            "bridge-state.json should show status running"
        );

        // Create RcPodGuard as panic safety net
        let pod_name = format!("bm-rc-{}", TEAM_NAME);
        let guard = RcPodGuard::new(pod_name.clone(), port);

        // Save guard info for progressive mode and subsequent cases
        env.export("rc_pod_name", &pod_name);

        // Forget guard -- it will be consumed in the stop case
        let (name, p) = guard.into_parts();
        env.export("rc_guard_name", &name);
        env.export("rc_guard_port", &p.to_string());
    }
}

fn bridge_start_idempotent_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let port = env.get_export("rc_port").expect("rc_port not set").to_string();

        // Bridge is already running from the previous step. Starting again should skip.
        let stdout = env.command("bm")
            .args(["bridge", "start", "-t", TEAM_NAME])
            .env("RC_PORT", &port)
            .run();
        assert!(
            stdout.contains("already running"),
            "re-starting a running bridge should say 'already running', got: {}",
            stdout
        );
    }
}

fn identity_add_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let port = env.get_export("rc_port").expect("rc_port not set").to_string();

        let stdout = env.command("bm")
            .args(["bridge", "identity", "add", MEMBER_DIR, "-t", TEAM_NAME])
            .env("RC_PORT", &port)
            .run();
        assert!(
            stdout.contains(MEMBER_DIR),
            "identity add output should mention member"
        );

        // Verify bridge-state.json has identity
        let bstate_path = env
            .home
            .join("workspaces")
            .join(TEAM_NAME)
            .join("bridge-state.json");
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        let identity = &bstate["identities"][MEMBER_DIR];
        assert!(
            !identity["user_id"].as_str().unwrap_or("").is_empty(),
            "identity should have user_id after onboard"
        );
    }
}

fn room_create_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let port = env.get_export("rc_port").expect("rc_port not set").to_string();

        env.command("bm")
            .args([
                "bridge",
                "room",
                "create",
                "e2e-team",
                "-t",
                TEAM_NAME,
            ])
            .env("RC_PORT", &port)
            .run();

        // Verify bridge-state.json has room
        let bstate_path = env
            .home
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
        assert!(
            ws.join(".botminter.workspace").exists(),
            "workspace should have marker file"
        );

        let ralph_yml_path = ws.join("ralph.yml");
        assert!(ralph_yml_path.exists(), "ralph.yml should exist");

        // Read bridge-state.json to get RC config values for RObot injection
        let bstate_path = env.home.join("workspaces").join(TEAM_NAME).join("bridge-state.json");
        let bstate_contents = fs::read_to_string(&bstate_path).unwrap();
        let bstate: serde_json::Value = serde_json::from_str(&bstate_contents).unwrap();
        let bot_user_id = bstate["identities"][MEMBER_DIR]["user_id"]
            .as_str()
            .expect("identity should have user_id")
            .to_string();
        let room_id = bstate["rooms"][0]["room_id"]
            .as_str()
            .expect("should have a room with room_id")
            .to_string();
        let port = env.get_export("rc_port").expect("rc_port not set");
        let server_url = format!("http://127.0.0.1:{}", port);

        let bridge_config = workspace::RobotBridgeConfig {
            bot_user_id,
            room_id,
            server_url,
            operator_id: None,
        };
        workspace::inject_robot_config(&ralph_yml_path, true, Some("rocketchat"), Some(&bridge_config)).unwrap();

        let ralph_contents = fs::read_to_string(&ralph_yml_path).unwrap();
        let ralph_doc: serde_yml::Value =
            serde_yml::from_str(&ralph_contents).unwrap();

        assert_eq!(
            ralph_doc["RObot"]["enabled"].as_bool(),
            Some(true),
            "RObot.enabled should be true"
        );
        assert!(
            ralph_doc["RObot"]["rocketchat"]["bot_user_id"]
                .as_str()
                .is_some(),
            "RObot.rocketchat.bot_user_id should be set"
        );
        assert!(
            ralph_doc["RObot"]["rocketchat"]["room_id"]
                .as_str()
                .is_some(),
            "RObot.rocketchat.room_id should be set"
        );
        assert!(
            ralph_doc["RObot"]["rocketchat"]["server_url"]
                .as_str()
                .is_some(),
            "RObot.rocketchat.server_url should be set"
        );

        assert!(
            !ralph_contents.contains("auth_token"),
            "ralph.yml must NOT contain auth_token"
        );
    }
}

fn bridge_health_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let stdout = env.command("bm")
            .args(["bridge", "status", "-t", TEAM_NAME])
            .run();
        assert!(
            stdout.contains("running") || stdout.contains("healthy"),
            "bridge status should show running or healthy"
        );
    }
}

fn bridge_stop_fn(
    _gh_token: String,
) -> impl Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static {
    move |env| {
        let port = env.get_export("rc_port").expect("rc_port not set").to_string();

        env.command("bm")
            .args(["bridge", "stop", "-t", TEAM_NAME])
            .env("RC_PORT", &port)
            .run();

        // Recreate guard from saved state, then consume it to prevent Drop cleanup
        let guard_name = env.get_export("rc_guard_name").expect("rc_guard_name not set").to_string();
        let guard_port: u16 = env.get_export("rc_guard_port").expect("rc_guard_port not set")
            .parse()
            .unwrap();
        let guard = RcPodGuard::from_existing(guard_name, guard_port);
        // Consume the guard -- pod already stopped by bm bridge stop
        let _ = guard.into_parts();

        // Verify bridge-state.json shows stopped
        let bstate_path = env
            .home
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

fn build_suite(gh_org: String, gh_token: String, config: &E2eConfig) -> GithubSuite {
    let app_id = config.app_id.clone();
    let app_client_id = config.app_client_id.clone();
    let app_installation_id = config.app_installation_id.clone();
    let app_private_key_file = config.app_private_key_file.clone();
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let repo_full_name = format!("{}/bm-e2e-rc-{}", gh_org, timestamp);

    GithubSuite::new_self_managed("scenario_rc_operator_journey", &repo_full_name)
        .setup({
            move |_env: &mut TestEnv| {
                // Verify podman is available
                if !telegram::podman_available() {
                    panic!("SKIP: podman not available -- RC bridge requires Podman");
                }
            }
        })
        .case(
            "01_init_with_rc_bridge",
            init_with_rc_bridge_fn(gh_org.clone(), gh_token.clone()),
        )
        .case("02_hire_member", hire_member_fn(gh_token.clone(), app_id.clone(), app_client_id.clone(), app_installation_id.clone(), app_private_key_file.clone()))
        .case("03_bridge_start", bridge_start_fn(gh_token.clone()))
        .case("03b_bridge_start_idempotent", bridge_start_idempotent_fn(gh_token.clone()))
        .case("04_identity_add", identity_add_fn(gh_token.clone()))
        .case("05_room_create", room_create_fn(gh_token.clone()))
        .case("06_provision_workspace", provision_workspace_fn(gh_token.clone()))
        .case("07_bridge_health", bridge_health_fn(gh_token.clone()))
        .case("08_bridge_stop", bridge_stop_fn(gh_token.clone()))
        // ── Cleanup ──────────────────────────────────────────────────
        .case("cleanup", {
            let gh_org_c = gh_org.clone();
            let gh_token_c = gh_token.clone();
            move |env: &mut TestEnv| {
                eprintln!("RC journey cleanup...");

                // Force-remove RC pod if it's still around
                let pod_name = format!("bm-rc-{}", TEAM_NAME);
                let _ = env.command("podman")
                    .args(["pod", "rm", "-f", &pod_name])
                    .output();

                // Delete team repo
                let _ = env.command("gh")
                    .args(["repo", "delete", &env.repo_full_name, "--yes"])
                    .output();

                // Delete workspace repo
                let ws_repo = format!("{}/{}-{}", gh_org_c, TEAM_NAME, MEMBER_DIR);
                let _ = env.command("gh")
                    .args(["repo", "delete", &ws_repo, "--yes"])
                    .output();

                // Clean up project boards
                cleanup_project_boards(
                    &gh_org_c,
                    &gh_token_c,
                    TEAM_NAME,
                );
            }
        })
        // Group bridge start through stop as atomic (cases 2-7, 0-indexed)
        .group(2, 7)
}

pub fn scenario(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone(), config).build(config)
}

pub fn scenario_progressive(config: &E2eConfig) -> Trial {
    build_suite(config.gh_org.clone(), config.gh_token.clone(), config).build_progressive(config)
}
