use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::Serialize;

use super::state::WebState;
use crate::config;

/// API response for a single team entry.
/// Deliberately omits credentials — never expose tokens over HTTP.
#[derive(Serialize)]
pub struct TeamResponse {
    pub name: String,
    pub profile: String,
    pub github_repo: String,
    pub path: String,
}

/// GET /api/teams — returns the list of registered teams.
pub async fn list_teams(State(state): State<WebState>) -> impl IntoResponse {
    match config::load_from(&state.config_path) {
        Ok(cfg) => {
            let teams: Vec<TeamResponse> = cfg
                .teams
                .iter()
                .map(|t| TeamResponse {
                    name: t.name.clone(),
                    profile: t.profile.clone(),
                    github_repo: t.github_repo.clone(),
                    path: t.path.display().to_string(),
                })
                .collect();
            (StatusCode::OK, Json(serde_json::json!(teams))).into_response()
        }
        Err(e) => {
            let msg = e.to_string();
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": msg })),
            )
                .into_response()
        }
    }
}

#[cfg(test)]
mod tests {
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::*;
    use crate::web::web_router;

    /// Helper: build a test app with a config file at the given path.
    fn test_app(config_path: std::path::PathBuf) -> axum::Router {
        let state = WebState {
            config_path: std::sync::Arc::new(config_path),
            sessions_state: None,
        };
        web_router(state)
    }

    #[tokio::test]
    async fn list_teams_returns_registered_teams() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");

        let cfg = config::BotminterConfig {
            workzone: tmp.path().join("workspaces"),
            default_team: Some("alpha".to_string()),
            teams: vec![
                config::TeamEntry {
                    name: "alpha".to_string(),
                    path: tmp.path().join("alpha"),
                    profile: "agentic-sdlc-minimal".to_string(),
                    github_repo: "org/alpha-team".to_string(),
                    credentials: config::Credentials::default(),
                    coding_agent: None,
                    project_number: None,
                    bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                    vm: None,
                },
                config::TeamEntry {
                    name: "beta".to_string(),
                    path: tmp.path().join("beta"),
                    profile: "scrum".to_string(),
                    github_repo: "org/beta-team".to_string(),
                    credentials: config::Credentials {
                        telegram_bot_token: None,
                        webhook_secret: None,
                    },
                    coding_agent: None,
                    project_number: Some(42),
                    bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                    vm: None,
                },
            ],
            vms: Vec::new(),
            keyring_collection: None,
        };
        config::save_to(&config_path, &cfg).unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(teams.len(), 2);
        assert_eq!(teams[0]["name"], "alpha");
        assert_eq!(teams[0]["profile"], "agentic-sdlc-minimal");
        assert_eq!(teams[0]["github_repo"], "org/alpha-team");
        assert!(teams[0]["path"].is_string());
        assert_eq!(teams[1]["name"], "beta");
        assert_eq!(teams[1]["profile"], "scrum");

        // Credentials must NOT leak into the response
        for team in &teams {
            assert!(
                team.get("credentials").is_none(),
                "Credentials must not appear in API response"
            );
            assert!(
                team.get("gh_token").is_none(),
                "gh_token must not appear in API response"
            );
        }
    }

    #[tokio::test]
    async fn list_teams_empty_when_no_teams() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");

        let cfg = config::BotminterConfig {
            workzone: tmp.path().join("workspaces"),
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
                    .uri("/api/teams")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), StatusCode::OK);

        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let teams: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(teams.is_empty(), "Should return empty array");
    }

    #[tokio::test]
    async fn list_teams_returns_500_on_missing_config() {
        let tmp = tempfile::tempdir().unwrap();
        // Point to a config file that does not exist
        let config_path = tmp.path().join("nonexistent").join("config.yml");

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams")
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
        assert!(
            err["error"].is_string(),
            "Error response should have 'error' field"
        );
    }

    #[tokio::test]
    async fn list_teams_returns_500_on_corrupt_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");
        std::fs::create_dir_all(config_path.parent().unwrap()).unwrap();
        std::fs::write(&config_path, "not: [valid: yaml: {{{}}}").unwrap();

        let app = test_app(config_path);
        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams")
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
        assert!(
            err["error"].is_string(),
            "Error response should have 'error' field"
        );
    }
}
