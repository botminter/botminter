use std::fs;
use std::path::Path;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::state::WebState;
use crate::config;
use crate::profile::ProfileManifest;

/// GET /api/teams/:team/overview — returns rich team overview data.
pub async fn team_overview(
    State(state): State<WebState>,
    AxumPath(team_name): AxumPath<String>,
) -> impl IntoResponse {
    match build_overview(&state, &team_name) {
        Ok(overview) => (StatusCode::OK, Json(serde_json::json!(overview))).into_response(),
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

fn build_overview(state: &WebState, team_name: &str) -> anyhow::Result<TeamOverviewResponse> {
    let cfg = config::load_from(&state.config_path)?;

    let team = cfg
        .teams
        .iter()
        .find(|t| t.name == team_name)
        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found", team_name))?;

    let team_path = team.path.join("team");
    let manifest_path = team_path.join("botminter.yml");
    let manifest: ProfileManifest = {
        let content = fs::read_to_string(&manifest_path).map_err(|e| {
            anyhow::anyhow!(
                "Failed to read botminter.yml at {}: {}",
                manifest_path.display(),
                e
            )
        })?;
        serde_yml::from_str(&content).map_err(|e| {
            anyhow::anyhow!("Failed to parse botminter.yml: {}", e)
        })?
    };

    let members = scan_members(&team_path)?;
    let knowledge_files = list_dir_files(&team_path.join("knowledge"));
    let invariant_files = list_dir_files(&team_path.join("invariants"));

    let bridge = BridgeOverview {
        selected: manifest.bridge.clone(),
        available: manifest
            .bridges
            .iter()
            .map(|b| b.name.clone())
            .collect(),
    };

    let default_coding_agent_display = manifest
        .coding_agents
        .get(&manifest.default_coding_agent)
        .map(|ca| ca.display_name.clone());

    Ok(TeamOverviewResponse {
        name: team.name.clone(),
        profile: manifest.name.clone(),
        display_name: manifest.display_name.clone(),
        description: manifest.description.clone(),
        version: manifest.version.clone(),
        github_repo: team.github_repo.clone(),
        default_coding_agent: default_coding_agent_display,
        roles: manifest
            .roles
            .iter()
            .map(|r| RoleResponse {
                name: r.name.clone(),
                description: r.description.clone(),
            })
            .collect(),
        members,
        status_count: manifest.statuses.len(),
        label_count: manifest.labels.len(),
        projects: manifest
            .projects
            .iter()
            .map(|p| ProjectResponse {
                name: p.name.clone(),
                fork_url: p.fork_url.clone(),
            })
            .collect(),
        bridge,
        knowledge_files,
        invariant_files,
    })
}

/// Scans the `members/` directory for each member's metadata.
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

        // Read member botminter.yml for role and emoji
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

        // Count hats from ralph.yml
        let ralph_path = dir.join("ralph.yml");
        let hat_count = count_hats(&ralph_path);

        members.push(MemberSummaryResponse {
            name: member_name,
            role,
            comment_emoji,
            hat_count,
        });
    }

    Ok(members)
}

/// Counts the number of hats in a ralph.yml file.
/// Hats are stored as a YAML map under the `hats:` key.
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

/// Lists file names in a directory (non-recursive, files only).
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
        .collect();
    files.sort();
    files
}

// ── Response types ──────────────────────────────────────────

#[derive(Serialize)]
pub struct TeamOverviewResponse {
    pub name: String,
    pub profile: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub github_repo: String,
    pub default_coding_agent: Option<String>,
    pub roles: Vec<RoleResponse>,
    pub members: Vec<MemberSummaryResponse>,
    pub status_count: usize,
    pub label_count: usize,
    pub projects: Vec<ProjectResponse>,
    pub bridge: BridgeOverview,
    pub knowledge_files: Vec<String>,
    pub invariant_files: Vec<String>,
}

#[derive(Serialize)]
pub struct RoleResponse {
    pub name: String,
    pub description: String,
}

#[derive(Serialize)]
pub struct MemberSummaryResponse {
    pub name: String,
    pub role: String,
    pub comment_emoji: String,
    pub hat_count: usize,
}

#[derive(Serialize)]
pub struct ProjectResponse {
    pub name: String,
    pub fork_url: String,
}

#[derive(Serialize)]
pub struct BridgeOverview {
    pub selected: Option<String>,
    pub available: Vec<String>,
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::web::web_router;
    use std::sync::Arc;

    /// Set up a realistic team repo from the fixture data in a tempdir.
    /// Returns the team directory (parent of team/). The actual team repo
    /// files are at team_dir/team/ — matching production layout.
    fn setup_fixture_team(tmp: &std::path::Path) -> std::path::PathBuf {
        let team_dir = tmp.join("my-team");
        let team_repo = team_dir.join("team");
        let fixture_base = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../.agents/planning/2026-03-22-console-web-ui/fixture-gen/fixtures/team-repo");

        // Copy the entire fixture team-repo into team_dir/team/ (production layout)
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
        let state = WebState {
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

    /// Reads expected values directly from the fixture's botminter.yml.
    fn read_fixture_manifest(team_dir: &std::path::Path) -> ProfileManifest {
        let content = fs::read_to_string(team_dir.join("team").join("botminter.yml")).unwrap();
        serde_yml::from_str(&content).unwrap()
    }

    #[tokio::test]
    async fn overview_returns_team_data_from_fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let config_path = tmp.path().join(".botminter").join("config.yml");

        // Read expected values from the fixture manifest dynamically
        let manifest = read_fixture_manifest(&team_path);
        let team_name = "my-team";
        let github_repo = "myorg/my-team";
        write_config(&config_path, team_name, &team_path, &manifest.name, github_repo);

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri(&format!("/api/teams/{team_name}/overview"))
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let overview: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Profile info — compare against dynamically read manifest
        assert_eq!(overview["name"], team_name);
        assert_eq!(overview["profile"], manifest.name);
        assert_eq!(overview["display_name"], manifest.display_name);
        assert_eq!(overview["description"], manifest.description);
        assert_eq!(overview["version"], manifest.version);
        assert_eq!(overview["github_repo"], github_repo);
        assert!(overview["default_coding_agent"].is_string());

        // Roles — count and names should match manifest
        let roles = overview["roles"].as_array().unwrap();
        assert_eq!(
            roles.len(),
            manifest.roles.len(),
            "API should return same number of roles as manifest"
        );
        for manifest_role in &manifest.roles {
            assert!(
                roles.iter().any(|r| r["name"] == manifest_role.name),
                "API should include role '{}'",
                manifest_role.name
            );
        }

        // Members — scan fixture members/ dir to get expected count
        let expected_member_count = fs::read_dir(team_path.join("team").join("members"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| e.path().is_dir())
            .count();
        let members = overview["members"].as_array().unwrap();
        assert_eq!(
            members.len(),
            expected_member_count,
            "API should return all members from fixture"
        );

        // Each member should have required fields
        for member in members {
            assert!(member["name"].is_string(), "Member must have name");
            assert!(member["role"].is_string(), "Member must have role");
            assert!(member["comment_emoji"].is_string(), "Member must have comment_emoji");
            assert!(member["hat_count"].is_number(), "Member must have hat_count");
            // hat_count should be non-negative
            assert!(member["hat_count"].as_u64().is_some());
        }

        // Counts — should reflect manifest data
        assert_eq!(
            overview["status_count"].as_u64().unwrap() as usize,
            manifest.statuses.len(),
            "status_count should match manifest"
        );
        assert_eq!(
            overview["label_count"].as_u64().unwrap() as usize,
            manifest.labels.len(),
            "label_count should match manifest"
        );

        // Bridge — available should match manifest bridges
        let available = overview["bridge"]["available"].as_array().unwrap();
        assert_eq!(
            available.len(),
            manifest.bridges.len(),
            "Available bridges should match manifest"
        );

        // Knowledge — count should match actual files on disk
        let expected_knowledge = list_dir_files(&team_path.join("team").join("knowledge"));
        let knowledge = overview["knowledge_files"].as_array().unwrap();
        assert_eq!(knowledge.len(), expected_knowledge.len());

        // Invariants — count should match actual files on disk
        let expected_invariants = list_dir_files(&team_path.join("team").join("invariants"));
        let invariants = overview["invariant_files"].as_array().unwrap();
        assert_eq!(invariants.len(), expected_invariants.len());

        // Credentials must NOT leak
        let body_str = String::from_utf8(body.to_vec()).unwrap();
        assert!(
            !body_str.contains("gh_token"),
            "Credentials must not appear in overview response"
        );
        assert!(
            !body_str.contains("credentials"),
            "Credentials field must not appear in overview response"
        );
    }

    #[tokio::test]
    async fn overview_returns_404_for_unknown_team() {
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
                    .uri("/api/teams/nonexistent/overview")
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
    async fn overview_returns_500_on_missing_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("empty-team");
        let team_repo = team_dir.join("team");
        fs::create_dir_all(&team_repo).unwrap();
        // No botminter.yml in team/

        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "empty-team", &team_dir, "test-profile", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/empty-team/overview")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let err: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(err["error"].as_str().unwrap().contains("botminter.yml"));
    }

    #[tokio::test]
    async fn overview_handles_empty_members_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("minimal-team");
        let team_repo = team_dir.join("team");

        // Create a minimal team repo with just botminter.yml
        fs::create_dir_all(&team_repo).unwrap();
        let manifest = r#"
name: test
display_name: "Test Team"
description: "A test team"
version: "1.0.0"
schema_version: "1.0"
roles:
  - name: dev
    description: "Developer"
"#;
        fs::write(team_repo.join("botminter.yml"), manifest).unwrap();

        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "minimal-team", &team_dir, "test", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/minimal-team/overview")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let overview: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(overview["members"].as_array().unwrap().len(), 0);
        assert_eq!(overview["knowledge_files"].as_array().unwrap().len(), 0);
        assert_eq!(overview["invariant_files"].as_array().unwrap().len(), 0);
    }
}
