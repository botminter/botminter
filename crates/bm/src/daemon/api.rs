use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use super::run::DaemonState;
use crate::formation;
use crate::state;

// ── Request types ────────────────────────────────────────────────────

/// Request body for `POST /api/members/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopMembersRequest {
    /// If set, stop only this member. If None, stop all members.
    pub member: Option<String>,
    /// Force-kill members instead of graceful shutdown.
    #[serde(default)]
    pub force: bool,
}

// ── Response types ───────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberErrorInfo {
    pub name: String,
    pub error: String,
}

/// Response for `POST /api/members/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopMembersResponse {
    pub ok: bool,
    pub stopped: Vec<MemberStoppedInfo>,
    pub errors: Vec<MemberErrorInfo>,
    #[serde(default)]
    pub no_members_running: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberStoppedInfo {
    pub name: String,
    pub already_exited: bool,
    pub forced: bool,
}

/// Response for `GET /api/members`.
#[derive(Debug, Serialize, Deserialize)]
pub struct MembersStatusResponse {
    pub members: Vec<MemberStatusInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberStatusInfo {
    pub name: String,
    pub status: String,
    pub pid: Option<u32>,
    pub workspace: Option<String>,
    pub brain_mode: bool,
    pub started_at: Option<String>,
}

/// Response for `GET /api/health`.
#[derive(Debug, Serialize, Deserialize)]
pub struct HealthResponse {
    pub ok: bool,
    pub version: String,
    pub team: String,
    pub daemon_mode: String,
    pub member_count: usize,
    pub uptime_secs: Option<u64>,
}

/// Error response body.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    ok: bool,
    error: String,
}

// ── Handlers ─────────────────────────────────────────────────────────

/// POST /api/members/stop — stops team members.
pub(super) async fn stop_members_handler(
    State(state): State<DaemonState>,
    Json(req): Json<StopMembersRequest>,
) -> impl IntoResponse {
    tracing::info!(
        filter = req.member.as_deref().unwrap_or("all"),
        force = req.force,
        "API: stop members"
    );

    let cfg = Arc::clone(&state.config);
    let team_entry = Arc::clone(&state.team_entry);

    let result = tokio::task::spawn_blocking(move || {
        formation::stop_local_members(
            &team_entry,
            &cfg,
            req.member.as_deref(),
            req.force,
        )
    })
    .await;

    match result {
        Ok(Ok(stop_result)) => {
            let has_errors = !stop_result.errors.is_empty();
            let resp = StopMembersResponse {
                ok: !has_errors,
                no_members_running: stop_result.no_members_running,
                stopped: stop_result
                    .stopped
                    .into_iter()
                    .map(|m| MemberStoppedInfo {
                        name: m.name,
                        already_exited: m.already_exited,
                        forced: m.forced,
                    })
                    .collect(),
                errors: stop_result
                    .errors
                    .into_iter()
                    .map(|m| MemberErrorInfo {
                        name: m.name,
                        error: m.error,
                    })
                    .collect(),
            };
            (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())).into_response()
        }
        Ok(Err(e)) => {
            tracing::error!(error = %e, "API stop failed");
            let resp = ErrorResponse {
                ok: false,
                error: e.to_string(),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            tracing::error!(error = %e, "API stop panicked");
            let resp = ErrorResponse {
                ok: false,
                error: "internal error".to_string(),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
    }
}

/// GET /api/members — returns member status.
pub(super) async fn list_members_handler(
    State(state): State<DaemonState>,
) -> impl IntoResponse {
    let team_name = state.team_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        let runtime_state = state::load()?;
        let team_prefix = format!("{}/", team_name);

        let members: Vec<MemberStatusInfo> = runtime_state
            .members
            .iter()
            .filter(|(key, _)| key.starts_with(&team_prefix))
            .map(|(key, rt)| {
                let name = key
                    .strip_prefix(&team_prefix)
                    .unwrap_or(key)
                    .to_string();
                let alive = state::is_alive(rt.pid);
                MemberStatusInfo {
                    name,
                    status: if alive {
                        if rt.brain_mode {
                            "brain".to_string()
                        } else {
                            "running".to_string()
                        }
                    } else {
                        "crashed".to_string()
                    },
                    pid: Some(rt.pid),
                    workspace: Some(rt.workspace.to_string_lossy().to_string()),
                    brain_mode: rt.brain_mode,
                    started_at: Some(rt.started_at.clone()),
                }
            })
            .collect();

        Ok::<_, anyhow::Error>(members)
    })
    .await;

    match result {
        Ok(Ok(members)) => {
            let resp = MembersStatusResponse { members };
            (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())).into_response()
        }
        Ok(Err(e)) => {
            let resp = ErrorResponse {
                ok: false,
                error: e.to_string(),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            let resp = ErrorResponse {
                ok: false,
                error: format!("internal error: {}", e),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
    }
}

/// GET /api/health — enhanced health check with daemon metadata.
pub(super) async fn health_check_handler(
    State(state): State<DaemonState>,
) -> impl IntoResponse {
    let team_name = state.team_name.clone();
    let mode = state.mode.clone();
    let started_at = state.started_at;

    let result = tokio::task::spawn_blocking(move || {
        let runtime_state = state::load().unwrap_or_default();
        let team_prefix = format!("{}/", team_name);
        let member_count = runtime_state
            .members
            .keys()
            .filter(|k| k.starts_with(&team_prefix))
            .count();
        (member_count, team_name)
    })
    .await;

    let (member_count, team) = match result {
        Ok(r) => r,
        Err(_) => (0, state.team_name.clone()),
    };

    let uptime_secs = started_at.map(|t| {
        let elapsed = std::time::Instant::now().duration_since(t);
        elapsed.as_secs()
    });

    let resp = HealthResponse {
        ok: !state.shutdown.load(Ordering::SeqCst),
        version: env!("CARGO_PKG_VERSION").to_string(),
        team,
        daemon_mode: mode,
        member_count,
        uptime_secs,
    };

    (StatusCode::OK, Json(serde_json::to_value(resp).unwrap()))
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn stop_request_deserialize_defaults() {
        let json = r#"{}"#;
        let req: StopMembersRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.member, None);
        assert!(!req.force);
    }

    #[test]
    fn stop_request_deserialize_with_force() {
        let json = r#"{"member": "alice", "force": true}"#;
        let req: StopMembersRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.member, Some("alice".to_string()));
        assert!(req.force);
    }

    #[test]
    fn stop_response_serialize() {
        let resp = StopMembersResponse {
            ok: true,
            stopped: vec![MemberStoppedInfo {
                name: "alice".to_string(),
                already_exited: false,
                forced: true,
            }],
            errors: vec![],
            no_members_running: false,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["stopped"][0]["name"], "alice");
        assert!(json["stopped"][0]["forced"].as_bool().unwrap());
    }

    #[test]
    fn members_status_response_serialize() {
        let resp = MembersStatusResponse {
            members: vec![
                MemberStatusInfo {
                    name: "alice".to_string(),
                    status: "running".to_string(),
                    pid: Some(1234),
                    workspace: Some("/tmp/ws/alice".to_string()),
                    brain_mode: false,
                    started_at: Some("2026-03-24T10:00:00Z".to_string()),
                },
                MemberStatusInfo {
                    name: "bob".to_string(),
                    status: "brain".to_string(),
                    pid: Some(5678),
                    workspace: Some("/tmp/ws/bob".to_string()),
                    brain_mode: true,
                    started_at: Some("2026-03-24T10:05:00Z".to_string()),
                },
            ],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["members"][0]["name"], "alice");
        assert_eq!(json["members"][0]["status"], "running");
        assert_eq!(json["members"][1]["brain_mode"], true);
    }

    #[test]
    fn health_response_serialize() {
        let resp = HealthResponse {
            ok: true,
            version: "0.1.0".to_string(),
            team: "my-team".to_string(),
            daemon_mode: "poll".to_string(),
            member_count: 3,
            uptime_secs: Some(120),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["version"], "0.1.0");
        assert_eq!(json["team"], "my-team");
        assert_eq!(json["daemon_mode"], "poll");
        assert_eq!(json["member_count"], 3);
        assert_eq!(json["uptime_secs"], 120);
    }

    #[test]
    fn error_response_serialize() {
        let resp = ErrorResponse {
            ok: false,
            error: "something went wrong".to_string(),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["error"], "something went wrong");
    }

    #[test]
    fn members_status_with_no_members() {
        let resp = MembersStatusResponse {
            members: vec![],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert!(json["members"].as_array().unwrap().is_empty());
    }
}
