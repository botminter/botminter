use axum::extract::{Path as AxumPath, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;

use super::state::WebState;

/// GET /api/teams/:team/sessions — returns session list for operator visibility.
///
/// Returns an empty array when the daemon has no sessions state (standalone mode).
pub async fn list_sessions(
    State(state): State<WebState>,
    AxumPath(_team_name): AxumPath<String>,
) -> impl IntoResponse {
    let summaries = match &state.sessions_state {
        Some(sessions) => sessions.list_for_console(),
        None => vec![],
    };
    (StatusCode::OK, Json(summaries)).into_response()
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    use super::super::state::WebState;
    use crate::daemon::sessions_api::{sessions_router, SessionsApiState};
    use crate::web::web_router;

    fn make_test_web_state(
        config_path: std::path::PathBuf,
        sessions: Option<SessionsApiState>,
    ) -> WebState {
        WebState {
            config_path: Arc::new(config_path),
            sessions_state: sessions,
        }
    }

    #[tokio::test]
    async fn list_sessions_returns_empty_when_no_sessions_state() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.yml");
        std::fs::write(&config_path, "workzone: /tmp\nteams: []\nvms: []\n").unwrap();

        let web_state = make_test_web_state(config_path, None);
        let app = web_router(web_state);

        let resp = app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/test-team/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let sessions: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();
        assert!(sessions.is_empty(), "must return [] when no sessions state");
    }

    #[tokio::test]
    async fn list_sessions_returns_summaries_with_required_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join("config.yml");
        std::fs::write(&config_path, "workzone: /tmp\nteams: []\nvms: []\n").unwrap();

        // Create sessions state — the clone shares the same Arc<Mutex<...>>.
        let sessions_state = SessionsApiState::new(tmp.path().join("registry.json"));
        let sessions_state_for_web = sessions_state.clone();

        // Use the sessions API router to create a session (no workspace, so no external deps).
        let sessions_app = sessions_router(sessions_state);
        let start_body = serde_json::json!({
            "member_name": "engineer-alice",
            "session_type": "Interactive"
        });
        let create_resp = sessions_app
            .oneshot(
                Request::builder()
                    .method("POST")
                    .uri("/api/sessions/start")
                    .header("content-type", "application/json")
                    .body(Body::from(serde_json::to_string(&start_body).unwrap()))
                    .unwrap(),
            )
            .await
            .unwrap();
        assert_eq!(
            create_resp.status(),
            axum::http::StatusCode::OK,
            "session creation must succeed"
        );

        // Now query the web console endpoint — reads from the same shared state.
        let web_state = make_test_web_state(config_path, Some(sessions_state_for_web));
        let web_app = web_router(web_state);
        let list_resp = web_app
            .oneshot(
                Request::builder()
                    .uri("/api/teams/test-team/sessions")
                    .body(Body::empty())
                    .unwrap(),
            )
            .await
            .unwrap();

        assert_eq!(list_resp.status(), axum::http::StatusCode::OK);
        let body = axum::body::to_bytes(list_resp.into_body(), usize::MAX)
            .await
            .unwrap();
        let sessions: Vec<serde_json::Value> = serde_json::from_slice(&body).unwrap();

        assert_eq!(sessions.len(), 1, "one session must appear");
        let s = &sessions[0];
        assert!(s["session_id"].is_string(), "session_id must be a string");
        assert_eq!(s["member_name"], "engineer-alice", "member_name must match");
        assert!(s["state"].is_string(), "state must be a string");
        assert_eq!(s["session_type"], "Interactive", "session_type must match");
        assert!(s["created_at"].is_string(), "created_at must be a string");
        assert_eq!(
            s["finalization_status"], "n/a",
            "finalization_status must be n/a for a new session"
        );
    }
}
