use std::sync::Arc;
use std::time::Duration;

use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::state::WebState;
use crate::config;
use crate::profile;
use crate::workspace;

/// POST /api/teams/:team/sync — trigger workspace sync for a team.
pub async fn team_sync(
    State(state): State<WebState>,
    AxumPath(team_name): AxumPath<String>,
) -> impl IntoResponse {
    let config_path = Arc::clone(&state.config_path);

    let result = tokio::time::timeout(
        Duration::from_secs(60),
        tokio::task::spawn_blocking(move || do_sync(&config_path, &team_name)),
    )
    .await;

    match result {
        Ok(Ok(Ok(resp))) => (StatusCode::OK, Json(serde_json::json!(resp))).into_response(),
        Ok(Ok(Err(e))) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": e.to_string() })),
        )
            .into_response(),
        Ok(Err(e)) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Sync task failed: {}", e) })),
        )
            .into_response(),
        Err(_) => (
            StatusCode::GATEWAY_TIMEOUT,
            Json(serde_json::json!({
                "error": "Sync timed out after 60 seconds"
            })),
        )
            .into_response(),
    }
}

fn do_sync(
    config_path: &std::path::Path,
    team_name: &str,
) -> anyhow::Result<SyncResponse> {
    let cfg = config::load_from(config_path)?;

    let team = cfg
        .teams
        .iter()
        .find(|t| t.name == team_name)
        .ok_or_else(|| anyhow::anyhow!("Team '{}' not found", team_name))?;

    let team_repo = team.path.join("team");
    let manifest: profile::ProfileManifest = {
        let content = std::fs::read_to_string(team_repo.join("botminter.yml"))?;
        serde_yml::from_str(&content)?
    };

    let coding_agent = profile::resolve_coding_agent(team, &manifest)?;

    let gh = if team.github_repo.is_empty() {
        None
    } else {
        Some(team.github_repo.as_str())
    };

    let team_path = team_repo
        .parent()
        .ok_or_else(|| anyhow::anyhow!("Team repo has no parent directory"))?;

    let params = workspace::TeamSyncParams {
        team_repo: &team_repo,
        team_path,
        team_name,
        manifest: &manifest,
        coding_agent,
        github_repo: gh,
        project_number: team.project_number,
        repos: false,
        verbose: true,
        bridge_flag: false,
        workzone: &cfg.workzone,
        keyring_collection: cfg.keyring_collection.clone(),
    };

    let result = workspace::sync_team_workspaces(&params)?;

    let mut changed_files = Vec::new();
    for event in &result.events {
        match event {
            workspace::TeamSyncEvent::WorkspaceCreated(name) => {
                changed_files.push(format!("created: {}", name));
            }
            workspace::TeamSyncEvent::WorkspaceSynced { name, events } => {
                for sync_event in events {
                    changed_files.push(format!("{}: {:?}", name, sync_event));
                }
            }
            workspace::TeamSyncEvent::NoMembers => {
                changed_files.push("no members found".to_string());
            }
            _ => {}
        }
    }

    let message = format!(
        "Sync complete: {} created, {} updated, {} failures",
        result.created, result.updated, result.failures.len()
    );

    Ok(SyncResponse {
        ok: result.failures.is_empty(),
        message,
        changed_files,
    })
}

#[derive(Debug, Serialize)]
pub struct SyncResponse {
    pub ok: bool,
    pub message: String,
    pub changed_files: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::web::web_router;
    use crate::web::state::WebState;

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

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
        let content = format!(
            "workzone: {}\nteams:\n- name: {}\n  path: {}\n  profile: {}\n  github_repo: {}\n  credentials: {{}}\n",
            team_path.parent().unwrap().display(),
            team_name,
            team_path.display(),
            profile_name,
            github_repo,
        );
        std::fs::write(config_path, content).unwrap();
    }

    #[tokio::test]
    async fn sync_team_not_found_returns_404_or_error() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.yml");
        std::fs::write(
            &config_path,
            "workzone: /tmp\nteams: []\n",
        )
        .unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/teams/nonexistent/sync")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::INTERNAL_SERVER_ERROR);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert!(json["error"].as_str().unwrap().contains("not found"));
    }

    #[tokio::test]
    async fn sync_team_no_members_returns_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let team_dir = tmp.path().join("my-team");
        let team_repo = team_dir.join("team");
        std::fs::create_dir_all(&team_repo).unwrap();

        // Create a minimal botminter.yml with required fields
        let manifest = r#"
name: test-profile
display_name: "Test Profile"
version: "1.0.0"
schema_version: "1"
description: Test
default_coding_agent: claude-code
coding_agents:
  claude-code:
    name: claude-code
    display_name: "Claude Code"
    context_file: "CLAUDE.md"
    agent_dir: ".claude"
    binary: claude
statuses: []
labels: []
views: []
roles: []
projects: []
"#;
        std::fs::write(team_repo.join("botminter.yml"), manifest).unwrap();

        let config_path = tmp.path().join("config.yml");
        write_config(&config_path, "my-team", &team_dir, "test-profile", "org/repo");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/teams/my-team/sync")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        let status = resp.status();
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX).await.unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body).unwrap();
        assert_eq!(
            status,
            StatusCode::OK,
            "Expected 200 OK but got {}: {}",
            status,
            json
        );
        assert_eq!(json["ok"], true);
        assert!(json["message"].as_str().unwrap().contains("Sync complete"));
    }
}
