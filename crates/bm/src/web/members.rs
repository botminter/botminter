use std::fs;
use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::state::WebState;
use crate::config;

/// GET /api/teams/:team/members — returns list of members with summary info.
pub async fn list_members(
    State(state): State<WebState>,
    AxumPath(team_name): AxumPath<String>,
) -> impl IntoResponse {
    match build_members_list(&state, &team_name) {
        Ok(members) => (StatusCode::OK, Json(serde_json::json!(members))).into_response(),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

/// GET /api/teams/:team/members/:name — returns detailed member info.
pub async fn get_member(
    State(state): State<WebState>,
    AxumPath((team_name, member_name)): AxumPath<(String, String)>,
) -> impl IntoResponse {
    match build_member_detail(&state, &team_name, &member_name) {
        Ok(detail) => (StatusCode::OK, Json(serde_json::json!(detail))).into_response(),
        Err(e) => {
            let status = if e.to_string().contains("not found") {
                StatusCode::NOT_FOUND
            } else {
                StatusCode::INTERNAL_SERVER_ERROR
            };
            (status, Json(serde_json::json!({ "error": e.to_string() }))).into_response()
        }
    }
}

fn resolve_team_path(state: &WebState, team_name: &str) -> anyhow::Result<std::path::PathBuf> {
    let cfg = config::load_from(&state.config_path)?;
    let team = cfg
        .teams
        .iter()
        .find(|t| t.name == team_name)
        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found", team_name))?;
    Ok(team.path.join("team"))
}

fn build_members_list(
    state: &WebState,
    team_name: &str,
) -> anyhow::Result<Vec<MemberSummaryResponse>> {
    let team_path = resolve_team_path(state, team_name)?;
    scan_members(&team_path)
}

fn build_member_detail(
    state: &WebState,
    team_name: &str,
    member_name: &str,
) -> anyhow::Result<MemberDetailResponse> {
    let team_path = resolve_team_path(state, team_name)?;
    let member_dir = team_path.join("members").join(member_name);

    if !member_dir.is_dir() {
        anyhow::bail!("Member '{}' not found", member_name);
    }

    // Read member botminter.yml for role and emoji
    let member_manifest_path = member_dir.join("botminter.yml");
    let (role, comment_emoji) = if member_manifest_path.exists() {
        let content = fs::read_to_string(&member_manifest_path).unwrap_or_default();
        let val: serde_yml::Value =
            serde_yml::from_str(&content).unwrap_or(serde_yml::Value::Null);
        let role = val
            .get("role")
            .and_then(|v| v.as_str())
            .unwrap_or("unknown")
            .to_string();
        let emoji = val
            .get("comment_emoji")
            .and_then(|v| v.as_str())
            .unwrap_or("")
            .to_string();
        (role, emoji)
    } else {
        ("unknown".to_string(), String::new())
    };

    // Read ralph.yml raw content
    let ralph_path = member_dir.join("ralph.yml");
    let ralph_yml = fs::read_to_string(&ralph_path).ok();

    // Parse hats from ralph.yml
    let hats = parse_hats(&ralph_path);

    // Read CLAUDE.md and PROMPT.md
    let claude_md = fs::read_to_string(member_dir.join("CLAUDE.md")).ok();
    let prompt_md = fs::read_to_string(member_dir.join("PROMPT.md")).ok();

    // List knowledge and invariant files
    let knowledge_files = list_dir_files(&member_dir.join("knowledge"));
    let invariant_files = list_dir_files(&member_dir.join("invariants"));

    // List skill directories from coding-agent/skills/
    let skill_dirs = list_subdirs(&member_dir.join("coding-agent").join("skills"));

    Ok(MemberDetailResponse {
        name: member_name.to_string(),
        role,
        comment_emoji,
        ralph_yml,
        claude_md,
        prompt_md,
        hats,
        knowledge_files,
        invariant_files,
        skill_dirs,
    })
}

/// Scans the `members/` directory for each member's summary.
fn scan_members(team_path: &Path) -> anyhow::Result<Vec<MemberSummaryResponse>> {
    let members_dir = team_path.join("members");
    if !members_dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut members = Vec::new();
    let mut entries: Vec<_> = fs::read_dir(&members_dir)?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.file_name());

    for entry in entries {
        let dir = entry.path();
        let member_name = entry.file_name().to_string_lossy().to_string();

        let member_manifest_path = dir.join("botminter.yml");
        let (role, comment_emoji) = if member_manifest_path.exists() {
            let content = fs::read_to_string(&member_manifest_path).unwrap_or_default();
            let val: serde_yml::Value =
                serde_yml::from_str(&content).unwrap_or(serde_yml::Value::Null);
            let role = val
                .get("role")
                .and_then(|v| v.as_str())
                .unwrap_or("unknown")
                .to_string();
            let emoji = val
                .get("comment_emoji")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            (role, emoji)
        } else {
            ("unknown".to_string(), String::new())
        };

        let ralph_path = dir.join("ralph.yml");
        let has_ralph_yml = ralph_path.exists();
        let hat_count = count_hats(&ralph_path);

        members.push(MemberSummaryResponse {
            name: member_name,
            role,
            comment_emoji,
            has_ralph_yml,
            hat_count,
        });
    }

    Ok(members)
}

/// Counts the number of hats in a ralph.yml file.
fn count_hats(ralph_path: &Path) -> usize {
    let content = match fs::read_to_string(ralph_path) {
        Ok(c) => c,
        Err(_) => return 0,
    };
    let val: serde_yml::Value = match serde_yml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return 0,
    };
    val.get("hats")
        .and_then(|v| v.as_mapping())
        .map(|m| m.len())
        .unwrap_or(0)
}

/// Parses hat summaries from a ralph.yml file.
fn parse_hats(ralph_path: &Path) -> Vec<HatSummaryResponse> {
    let content = match fs::read_to_string(ralph_path) {
        Ok(c) => c,
        Err(_) => return Vec::new(),
    };
    let val: serde_yml::Value = match serde_yml::from_str(&content) {
        Ok(v) => v,
        Err(_) => return Vec::new(),
    };

    let hats_map = match val.get("hats").and_then(|v| v.as_mapping()) {
        Some(m) => m,
        None => return Vec::new(),
    };

    let mut hats: Vec<HatSummaryResponse> = hats_map
        .iter()
        .filter_map(|(key, value)| {
            let name = key.as_str()?.to_string();
            let description = value
                .get("description")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .to_string();
            let triggers = value
                .get("triggers")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            let publishes = value
                .get("publishes")
                .and_then(|v| v.as_sequence())
                .map(|seq| {
                    seq.iter()
                        .filter_map(|v| v.as_str().map(String::from))
                        .collect()
                })
                .unwrap_or_default();
            Some(HatSummaryResponse {
                name,
                description,
                triggers,
                publishes,
            })
        })
        .collect();
    hats.sort_by(|a, b| a.name.cmp(&b.name));
    hats
}

/// Lists file names in a directory (non-recursive, files only, excludes .gitkeep).
fn list_dir_files(dir: &Path) -> Vec<String> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<String> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_file())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| name != ".gitkeep")
        .collect();
    files.sort();
    files
}

/// Lists subdirectory names in a directory (non-recursive).
fn list_subdirs(dir: &Path) -> Vec<String> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut dirs: Vec<String> = fs::read_dir(dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    dirs.sort();
    dirs
}

// ── Response types ──────────────────────────────────────────

#[derive(Serialize)]
pub struct MemberSummaryResponse {
    pub name: String,
    pub role: String,
    pub comment_emoji: String,
    pub has_ralph_yml: bool,
    pub hat_count: usize,
}

#[derive(Serialize)]
pub struct MemberDetailResponse {
    pub name: String,
    pub role: String,
    pub comment_emoji: String,
    pub ralph_yml: Option<String>,
    pub claude_md: Option<String>,
    pub prompt_md: Option<String>,
    pub hats: Vec<HatSummaryResponse>,
    pub knowledge_files: Vec<String>,
    pub invariant_files: Vec<String>,
    pub skill_dirs: Vec<String>,
}

#[derive(Serialize)]
pub struct HatSummaryResponse {
    pub name: String,
    pub description: String,
    pub triggers: Vec<String>,
    pub publishes: Vec<String>,
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::web::web_router;
    use std::sync::Arc;

    fn setup_fixture_team(tmp: &std::path::Path) -> std::path::PathBuf {
        let team_dir = tmp.join("my-team");
        let team_repo = team_dir.join("team");
        let fixture_base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.agents/planning/2026-03-22-console-web-ui/fixture-gen/fixtures/team-repo");
        copy_dir_recursive(&fixture_base, &team_repo);
        team_dir
    }

    fn copy_dir_recursive(src: &std::path::Path, dst: &std::path::Path) {
        fs::create_dir_all(dst).unwrap();
        for entry in fs::read_dir(src).unwrap() {
            let entry = entry.unwrap();
            let src_path = entry.path();
            let dst_path = dst.join(entry.file_name());
            if src_path.is_dir() {
                copy_dir_recursive(&src_path, &dst_path);
            } else {
                fs::copy(&src_path, &dst_path).unwrap();
            }
        }
    }

    fn test_app(config_path: std::path::PathBuf) -> axum::Router {
        let state = super::super::state::WebState {
            config_path: Arc::new(config_path),
        };
        web_router(state)
    }

    fn write_config(
        config_path: &std::path::Path,
        team_name: &str,
        team_path: &std::path::Path,
        profile_name: &str,
        github_repo: &str,
    ) {
        let cfg = config::BotminterConfig {
            workzone: team_path.parent().unwrap().to_path_buf(),
            default_team: Some(team_name.to_string()),
            teams: vec![config::TeamEntry {
                name: team_name.to_string(),
                path: team_path.to_path_buf(),
                profile: profile_name.to_string(),
                github_repo: github_repo.to_string(),
                credentials: config::Credentials::default(),
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }],
            vms: Vec::new(),
            keyring_collection: None,
        };
        config::save_to(config_path, &cfg).unwrap();
    }

    #[tokio::test]
    async fn members_list_returns_all_fixture_members() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/members")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let members: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        // Dynamically count expected members from fixture (team repo is at team_dir/team/)
        let expected_count = fs::read_dir(team_path.join("team").join("members"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count();
        assert_eq!(members.len(), expected_count);

        // Each member has required fields
        for member in &members {
            assert!(member["name"].is_string());
            assert!(member["role"].is_string());
            assert!(member["comment_emoji"].is_string());
            assert!(member["has_ralph_yml"].is_boolean());
            assert!(member["hat_count"].is_number());
        }

        // Verify alice has correct data
        let alice = members.iter().find(|m| m["name"] == "superman-alice").unwrap();
        assert_eq!(alice["role"], "superman");
        assert!(alice["has_ralph_yml"].as_bool().unwrap());
        assert!(alice["hat_count"].as_u64().unwrap() > 0);
    }

    #[tokio::test]
    async fn member_detail_returns_full_data_with_parsed_hats() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/members/superman-alice")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let detail: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Basic fields
        assert_eq!(detail["name"], "superman-alice");
        assert_eq!(detail["role"], "superman");
        assert!(detail["comment_emoji"].is_string());

        // Raw YAML content
        assert!(detail["ralph_yml"].is_string());
        let ralph_yml = detail["ralph_yml"].as_str().unwrap();
        assert!(ralph_yml.contains("hats:"), "ralph_yml should contain hats section");

        // CLAUDE.md and PROMPT.md
        assert!(detail["claude_md"].is_string());
        assert!(detail["prompt_md"].is_string());

        // Parsed hats — should have multiple hats with triggers/publishes
        let hats = detail["hats"].as_array().unwrap();
        assert!(hats.len() > 0, "Alice should have hats");

        // Count hats dynamically from ralph.yml on disk
        let ralph_content =
            fs::read_to_string(team_path.join("team/members/superman-alice/ralph.yml")).unwrap();
        let ralph_val: serde_yml::Value = serde_yml::from_str(&ralph_content).unwrap();
        let expected_hat_count = ralph_val
            .get("hats")
            .and_then(|v| v.as_mapping())
            .map(|m| m.len())
            .unwrap_or(0);
        assert_eq!(hats.len(), expected_hat_count, "Hat count should match ralph.yml");

        // Each hat has required fields
        for hat in hats {
            assert!(hat["name"].is_string());
            assert!(hat["description"].is_string());
            assert!(hat["triggers"].is_array());
            assert!(hat["publishes"].is_array());
        }

        // Invariant files — alice has design-quality.md
        let invariant_files = detail["invariant_files"].as_array().unwrap();
        assert!(
            invariant_files.iter().any(|f| f == "design-quality.md"),
            "Alice should have design-quality.md invariant"
        );
    }

    #[tokio::test]
    async fn member_detail_returns_404_for_unknown_member() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "my-team", &team_path, "agentic-sdlc-minimal", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/members/nonexistent")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(err["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn members_list_returns_404_for_unknown_team() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");
        let cfg = config::BotminterConfig {
            workzone: tmp.path().to_path_buf(),
            default_team: None,
            teams: Vec::new(),
            vms: Vec::new(),
            keyring_collection: None,
        };
        config::save_to(&config_path, &cfg).unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/nonexistent/members")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
