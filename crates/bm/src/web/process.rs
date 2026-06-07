use std::fs;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::state::WebState;
use crate::config;
use crate::profile::{LabelDef, ProfileManifest, StatusDef, ViewDef};

/// GET /api/teams/:team/process — returns process pipeline data.
pub async fn team_process(
    State(state): State<WebState>,
    AxumPath(team_name): AxumPath<String>,
) -> impl IntoResponse {
    match build_process(&state, &team_name) {
        Ok(info) => (StatusCode::OK, Json(serde_json::json!(info))).into_response(),
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

fn build_process(state: &WebState, team_name: &str) -> anyhow::Result<ProcessResponse> {
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
        serde_yml::from_str(&content)
            .map_err(|e| anyhow::anyhow!("Failed to parse botminter.yml: {}", e))?
    };

    // Read PROCESS.md (optional)
    let process_md_path = team_path.join("PROCESS.md");
    let markdown = fs::read_to_string(&process_md_path).ok();

    // Read workflows/*.dot (graceful degradation if dir missing)
    let workflows = read_workflows(&team_path.join("workflows"));

    Ok(ProcessResponse {
        markdown,
        workflows,
        statuses: manifest.statuses,
        labels: manifest.labels,
        views: manifest.views,
    })
}

/// Reads all .dot files from the workflows directory, sorted by name.
fn read_workflows(workflows_dir: &std::path::Path) -> Vec<WorkflowResponse> {
    if !workflows_dir.is_dir() {
        return Vec::new();
    }

    let mut workflows: Vec<WorkflowResponse> = fs::read_dir(workflows_dir)
        .ok()
        .into_iter()
        .flatten()
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .map(|ext| ext == "dot")
                .unwrap_or(false)
        })
        .filter_map(|e| {
            let path = e.path();
            let name = path.file_stem()?.to_string_lossy().to_string();
            let dot = fs::read_to_string(&path).ok()?;
            Some(WorkflowResponse { name, dot })
        })
        .collect();
    workflows.sort_by(|a, b| a.name.cmp(&b.name));
    workflows
}

// ── Response types ──────────────────────────────────────────

#[derive(Serialize)]
pub struct ProcessResponse {
    pub markdown: Option<String>,
    pub workflows: Vec<WorkflowResponse>,
    pub statuses: Vec<StatusDef>,
    pub labels: Vec<LabelDef>,
    pub views: Vec<ViewDef>,
}

#[derive(Serialize)]
pub struct WorkflowResponse {
    pub name: String,
    pub dot: String,
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
        let state = WebState {
            config_path: Arc::new(config_path),
            sessions_state: None,
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

    fn read_fixture_manifest(team_dir: &std::path::Path) -> ProfileManifest {
        let content = fs::read_to_string(team_dir.join("team").join("botminter.yml")).unwrap();
        serde_yml::from_str(&content).unwrap()
    }

    #[tokio::test]
    async fn process_returns_full_data_from_fixtures() {
        let tmp = tempfile::tempdir().unwrap();
        let team_path = setup_fixture_team(tmp.path());
        let manifest = read_fixture_manifest(&team_path);
        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(
            &config_path,
            "my-team",
            &team_path,
            &manifest.name,
            "org/test",
        );

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/my-team/process")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let data: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // PROCESS.md content
        assert!(data["markdown"].is_string(), "markdown must be present");
        let md = data["markdown"].as_str().unwrap();
        assert!(!md.is_empty(), "PROCESS.md should not be empty");

        // Workflows — should match DOT files in fixture
        let workflows = data["workflows"].as_array().unwrap();
        let expected_dot_files: Vec<String> = fs::read_dir(team_path.join("team").join("workflows"))
            .unwrap()
            .filter_map(|e| e.ok())
            .filter(|e| {
                e.path()
                    .extension()
                    .map(|ext| ext == "dot")
                    .unwrap_or(false)
            })
            .map(|e| {
                e.path()
                    .file_stem()
                    .unwrap()
                    .to_string_lossy()
                    .to_string()
            })
            .collect();
        assert_eq!(
            workflows.len(),
            expected_dot_files.len(),
            "workflow count should match fixture DOT files"
        );
        for wf in workflows {
            assert!(wf["name"].is_string());
            assert!(wf["dot"].is_string());
            let dot_content = wf["dot"].as_str().unwrap();
            assert!(
                dot_content.contains("digraph"),
                "DOT content should contain 'digraph'"
            );
        }

        // Statuses — count from manifest
        let statuses = data["statuses"].as_array().unwrap();
        assert_eq!(
            statuses.len(),
            manifest.statuses.len(),
            "status count should match manifest"
        );
        for status in statuses {
            assert!(status["name"].is_string());
            assert!(status["description"].is_string());
        }

        // Labels
        let labels = data["labels"].as_array().unwrap();
        assert_eq!(
            labels.len(),
            manifest.labels.len(),
            "label count should match manifest"
        );
        for label in labels {
            assert!(label["name"].is_string());
            assert!(label["color"].is_string());
            assert!(label["description"].is_string());
        }

        // Views
        let views = data["views"].as_array().unwrap();
        assert_eq!(
            views.len(),
            manifest.views.len(),
            "view count should match manifest"
        );
        for view in views {
            assert!(view["name"].is_string());
            assert!(view["prefixes"].is_array());
        }
    }

    #[tokio::test]
    async fn process_graceful_degradation_without_workflows() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("no-wf-team");
        let team_repo = team_dir.join("team");
        fs::create_dir_all(&team_repo).unwrap();

        // Minimal botminter.yml with no workflows/ dir
        let manifest_yml = r#"
name: test
display_name: "Test"
description: "Test team"
version: "1.0.0"
schema_version: "1.0"
statuses:
  - name: "po:triage"
    description: "Triage"
labels:
  - name: "kind/epic"
    color: "0E8A16"
    description: "Epic"
"#;
        fs::write(team_repo.join("botminter.yml"), manifest_yml).unwrap();

        let config_path = tmp.path().join(".botminter").join("config.yml");
        write_config(&config_path, "no-wf", &team_dir, "test", "org/test");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/no-wf/process")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let data: serde_json::Value = serde_json::from_slice(&body).unwrap();

        // Workflows should be empty array, not null or error
        assert_eq!(
            data["workflows"].as_array().unwrap().len(),
            0,
            "workflows should be empty when dir is missing"
        );
        // Markdown should be null when PROCESS.md is missing
        assert!(data["markdown"].is_null());
        // Statuses and labels still present
        assert_eq!(data["statuses"].as_array().unwrap().len(), 1);
        assert_eq!(data["labels"].as_array().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn process_returns_404_for_unknown_team() {
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
                    .uri("/api/teams/nonexistent/process")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::NOT_FOUND);
    }
}
