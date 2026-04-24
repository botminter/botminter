use std::sync::atomic::Ordering;
use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::Json;
use serde::{Deserialize, Serialize};

use anyhow::Context;

use super::log::daemon_log;
use super::run::DaemonState;
use crate::formation::{self, CredentialDomain};
use crate::git::app_auth;
use crate::git::manifest_flow::credential_keys;
use crate::state;

// ── Request types ────────────────────────────────────────────────────

/// Request body for `POST /api/members/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartMembersRequest {
    /// If set, start only this member. If None, start all members.
    pub member: Option<String>,
}

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

/// Response for `POST /api/members/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartMembersResponse {
    pub ok: bool,
    pub launched: Vec<MemberLaunchedInfo>,
    pub skipped: Vec<MemberSkippedInfo>,
    pub errors: Vec<MemberErrorInfo>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberLaunchedInfo {
    pub name: String,
    pub pid: u32,
    pub brain_mode: bool,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct MemberSkippedInfo {
    pub name: String,
    pub pid: u32,
}

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

/// Request body for `POST /api/loops/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartLoopRequest {
    /// The prompt to pass to `ralph run -p`.
    pub prompt: String,
    /// If set, run the loop in this member's workspace. Defaults to the first member.
    pub member: Option<String>,
}

/// Response for `POST /api/loops/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartLoopResponse {
    pub ok: bool,
    pub loop_id: Option<String>,
    pub pid: Option<u32>,
    pub error: Option<String>,
}

/// Error response body.
#[derive(Debug, Serialize)]
struct ErrorResponse {
    ok: bool,
    error: String,
}

// ── Handlers ─────────────────────────────────────────────────────────

/// POST /api/members/start — launches team members.
pub(super) async fn start_members_handler(
    State(state): State<DaemonState>,
    Json(req): Json<StartMembersRequest>,
) -> impl IntoResponse {
    let paths = Arc::clone(&state.paths);

    daemon_log(
        &paths,
        "INFO",
        &format!(
            "API: start members (filter: {:?})",
            req.member.as_deref().unwrap_or("all")
        ),
    );

    let cfg = Arc::clone(&state.config);
    let team_entry = Arc::clone(&state.team_entry);

    let result = tokio::task::spawn_blocking(move || {
        let team_repo = team_entry.path.join("team");

        formation::start_local_members(
            &team_entry,
            &cfg,
            &team_repo,
            req.member.as_deref(),
            false,
            None,
        )
    })
    .await;

    match result {
        Ok(Ok(start_result)) => {
            // After successful launch, cache App credentials for launched members
            // and spawn refresh loops (on-demand — Req 8).
            let team_name_for_cache = state.team_name.clone();
            let app_creds = Arc::clone(&state.app_credentials);
            let shutdown_for_refresh = Arc::clone(&state.shutdown);
            let paths_for_refresh = Arc::clone(&state.paths);

            for member in &start_result.launched {
                if let Ok(cached) = cache_app_credentials(
                    &team_name_for_cache,
                    &member.name,
                    &member.pid,
                ) {
                    let member_name = member.name.clone();
                    app_creds
                        .lock()
                        .unwrap()
                        .insert(member_name.clone(), cached.clone());

                    daemon_log(
                        &paths,
                        "INFO",
                        &format!("Cached App credentials for member '{}'", member_name),
                    );

                    // Spawn background refresh task (50-minute interval — Req 11)
                    let refresh_creds = cached;
                    let refresh_shutdown = Arc::clone(&shutdown_for_refresh);
                    let refresh_paths = Arc::clone(&paths_for_refresh);
                    let refresh_team = team_name_for_cache.clone();
                    tokio::spawn(async move {
                        run_token_refresh_loop(
                            refresh_creds,
                            &refresh_team,
                            &refresh_paths,
                            &refresh_shutdown,
                        )
                        .await;
                    });
                }
            }

            let has_errors = !start_result.errors.is_empty();
            let resp = StartMembersResponse {
                ok: !has_errors,
                launched: start_result
                    .launched
                    .into_iter()
                    .map(|m| MemberLaunchedInfo {
                        name: m.name,
                        pid: m.pid,
                        brain_mode: m.brain_mode,
                    })
                    .collect(),
                skipped: start_result
                    .skipped
                    .into_iter()
                    .map(|m| MemberSkippedInfo {
                        name: m.name,
                        pid: m.pid,
                    })
                    .collect(),
                errors: start_result
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
            daemon_log(&paths, "ERROR", &format!("API start failed: {}", e));
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
            daemon_log(&paths, "ERROR", &format!("API start panicked: {}", e));
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

/// Reads App credentials from keyring for a launched member and returns
/// cached credentials for the refresh loop. Returns Err if the member
/// has no App credentials (not an error — just means legacy auth).
fn cache_app_credentials(
    team_name: &str,
    member_name: &str,
    _pid: &u32,
) -> anyhow::Result<formation::AppCredentialsCached> {
    let formation = formation::local::create_local_formation(team_name)?;
    let store = formation.credential_store(CredentialDomain::GitHubApp {
        team_name: team_name.to_string(),
        member_name: member_name.to_string(),
    })?;

    let client_id = store
        .retrieve(&credential_keys::client_id(member_name))?
        .ok_or_else(|| anyhow::anyhow!("No App client_id for member"))?;
    let private_key = store
        .retrieve(&credential_keys::private_key(member_name))?
        .ok_or_else(|| anyhow::anyhow!("No App private_key for member"))?;
    let installation_id_str = store
        .retrieve(&credential_keys::installation_id(member_name))?
        .ok_or_else(|| anyhow::anyhow!("No App installation_id for member"))?;
    let installation_id: u64 = installation_id_str
        .parse()
        .context("Invalid installation ID")?;

    // Resolve workspace from state.json
    let runtime_state = state::load()?;
    let state_key = format!("{}/{}", team_name, member_name);
    let workspace = runtime_state
        .members
        .get(&state_key)
        .map(|rt| rt.workspace.clone())
        .ok_or_else(|| anyhow::anyhow!("Member not in state.json"))?;

    Ok(formation::AppCredentialsCached {
        member_name: member_name.to_string(),
        client_id,
        private_key,
        installation_id,
        workspace,
    })
}

/// Background task: refreshes installation tokens every 50 minutes.
/// On failure, retries with exponential backoff. The existing token
/// remains valid until its 1-hour expiry (Req 11).
async fn run_token_refresh_loop(
    creds: formation::AppCredentialsCached,
    team_name: &str,
    paths: &super::config::DaemonPaths,
    shutdown: &std::sync::atomic::AtomicBool,
) {
    use super::log::daemon_log;

    // 50 minutes = 3000 seconds
    const REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(50 * 60);
    const MAX_BACKOFF: std::time::Duration = std::time::Duration::from_secs(300); // 5 min

    let mut backoff = std::time::Duration::from_secs(10);

    loop {
        tokio::time::sleep(REFRESH_INTERVAL).await;

        if shutdown.load(Ordering::SeqCst) {
            daemon_log(
                paths,
                "INFO",
                &format!("Token refresh loop stopping for member '{}'", creds.member_name),
            );
            break;
        }

        // Check if the member process is still alive
        let state_key = format!("{}/{}", team_name, creds.member_name);
        if !is_member_alive(&state_key) {
            daemon_log(
                paths,
                "INFO",
                &format!(
                    "Member '{}' no longer running, stopping refresh loop",
                    creds.member_name
                ),
            );
            break;
        }

        // Attempt token refresh with inner retry loop on failure.
        // Tokens expire after 60 minutes. We refresh at 50 minutes, leaving
        // a 10-minute window for retries before the token becomes invalid.
        loop {
            let refresh_result = attempt_token_refresh(&creds, team_name).await;

            match refresh_result {
                Ok(Ok(())) => {
                    daemon_log(
                        paths,
                        "INFO",
                        &format!("Refreshed token for member '{}'", creds.member_name),
                    );
                    backoff = std::time::Duration::from_secs(10);
                    break; // Success — resume outer 50-min cycle
                }
                Ok(Err(e)) => {
                    daemon_log(
                        paths,
                        "ERROR",
                        &format!(
                            "Token refresh failed for '{}': {}. Retrying in {}s. Existing token valid until expiry.",
                            creds.member_name,
                            e,
                            backoff.as_secs()
                        ),
                    );
                    tokio::time::sleep(backoff).await;
                    backoff = std::cmp::min(backoff * 2, MAX_BACKOFF);

                    // Before retrying, check shutdown and member liveness
                    if shutdown.load(Ordering::SeqCst) || !is_member_alive(&state_key) {
                        daemon_log(
                            paths,
                            "INFO",
                            &format!(
                                "Stopping retry loop for '{}' (shutdown or member exited)",
                                creds.member_name
                            ),
                        );
                        return; // Exit the entire refresh function
                    }
                }
                Err(e) => {
                    daemon_log(
                        paths,
                        "ERROR",
                        &format!("Token refresh task panicked for '{}': {}", creds.member_name, e),
                    );
                    return; // Unrecoverable — exit
                }
            }
        }
    }
}

/// Check if a member process is still alive based on state.json.
fn is_member_alive(state_key: &str) -> bool {
    match state::load() {
        Ok(s) => s
            .members
            .get(state_key)
            .map(|rt| state::is_alive(rt.pid))
            .unwrap_or(false),
        Err(_) => false,
    }
}

/// Attempt a single token refresh: JWT generation + exchange + atomic write.
async fn attempt_token_refresh(
    creds: &formation::AppCredentialsCached,
    team_name: &str,
) -> Result<anyhow::Result<()>, tokio::task::JoinError> {
    tokio::task::spawn_blocking({
        let creds = creds.clone();
        let team_name = team_name.to_string();
        move || -> anyhow::Result<()> {
            let jwt = app_auth::generate_jwt(&creds.client_id, &creds.private_key)?;
            let inst_token =
                app_auth::exchange_for_installation_token(&jwt, creds.installation_id)?;

            let formation = formation::local::create_local_formation(&team_name)?;
            formation.refresh_token(&creds.member_name, &creds.workspace, &inst_token.token)?;

            Ok(())
        }
    })
    .await
}

/// POST /api/members/stop — stops team members.
pub(super) async fn stop_members_handler(
    State(state): State<DaemonState>,
    Json(req): Json<StopMembersRequest>,
) -> impl IntoResponse {
    let paths = Arc::clone(&state.paths);

    daemon_log(
        &paths,
        "INFO",
        &format!(
            "API: stop members (filter: {:?}, force: {})",
            req.member.as_deref().unwrap_or("all"),
            req.force
        ),
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
            daemon_log(&paths, "ERROR", &format!("API stop failed: {}", e));
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
            daemon_log(&paths, "ERROR", &format!("API stop panicked: {}", e));
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

/// POST /api/loops/start — spawns a Ralph loop in a member's workspace.
pub(super) async fn start_loop_handler(
    State(state): State<DaemonState>,
    Json(req): Json<StartLoopRequest>,
) -> impl IntoResponse {
    let paths = Arc::clone(&state.paths);

    daemon_log(
        &paths,
        "INFO",
        &format!(
            "API: start loop (member: {:?}, prompt length: {})",
            req.member.as_deref().unwrap_or("default"),
            req.prompt.len()
        ),
    );

    let cfg = Arc::clone(&state.config);
    let team_entry = Arc::clone(&state.team_entry);
    let team_name = state.team_name.clone();

    let result = tokio::task::spawn_blocking(move || {
        start_loop_blocking(&team_name, &cfg, &team_entry, &req)
    })
    .await;

    match result {
        Ok(Ok(resp)) => {
            (StatusCode::OK, Json(serde_json::to_value(resp).unwrap())).into_response()
        }
        Ok(Err(e)) => {
            daemon_log(&paths, "ERROR", &format!("API start loop failed: {}", e));
            let resp = StartLoopResponse {
                ok: false,
                loop_id: None,
                pid: None,
                error: Some(e.to_string()),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
        Err(e) => {
            daemon_log(&paths, "ERROR", &format!("API start loop panicked: {}", e));
            let resp = StartLoopResponse {
                ok: false,
                loop_id: None,
                pid: None,
                error: Some("internal error".to_string()),
            };
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::to_value(resp).unwrap()),
            )
                .into_response()
        }
    }
}

/// Blocking implementation for loop spawning.
fn start_loop_blocking(
    team_name: &str,
    cfg: &crate::config::BotminterConfig,
    team_entry: &crate::config::TeamEntry,
    req: &StartLoopRequest,
) -> anyhow::Result<StartLoopResponse> {
    use crate::workspace;

    let team_repo = team_entry.path.join("team");
    let members_dir = team_repo.join("members");

    // Resolve which member's workspace to use
    let member_name = if let Some(ref name) = req.member {
        name.clone()
    } else {
        // Default to the first member found
        let dirs = workspace::list_member_dirs(&members_dir)?;
        dirs.into_iter()
            .next()
            .ok_or_else(|| anyhow::anyhow!("No members found in team"))?
    };

    let team_ws_base = cfg.workzone.join(team_name);
    let ws = workspace::find_workspace(&team_ws_base, &member_name)
        .ok_or_else(|| anyhow::anyhow!("No workspace found for member '{}'", member_name))?;

    // Write prompt to a temp file in the workspace
    let prompt_file = ws.join(".ralph-loop-prompt.md");
    std::fs::write(&prompt_file, &req.prompt)
        .with_context(|| format!("Failed to write loop prompt to {}", prompt_file.display()))?;

    // Resolve App credentials for the member (same path as member start)
    let local_formation = crate::formation::local::create_local_formation(team_name)?;
    let app_cred_store = local_formation.credential_store(
        crate::formation::CredentialDomain::GitHubApp {
            team_name: team_name.to_string(),
            member_name: String::new(),
        },
    )?;
    let gh_config_dir = match crate::formation::start_members::resolve_app_credentials_and_deliver(
        app_cred_store.as_ref(),
        local_formation.as_ref(),
        &member_name,
        &ws,
    ) {
        Ok(dir) => dir,
        Err(e) => {
            tracing::warn!(member = %member_name, error = %e, "App credential setup failed for loop, falling back to ambient auth");
            None
        }
    };

    // Spawn ralph run with the prompt
    let mut cmd = std::process::Command::new("ralph");
    cmd.args(["run", "-p"])
        .arg(&prompt_file)
        .current_dir(&ws)
        .env_remove("CLAUDECODE")
        .stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    if let Some(config_dir) = gh_config_dir {
        cmd.env("GH_CONFIG_DIR", config_dir);
        cmd.env_remove("GH_TOKEN");
        cmd.env_remove("GITHUB_TOKEN");
    }

    let child = cmd.spawn().with_context(|| {
        format!("Failed to spawn ralph loop in {}", ws.display())
    })?;

    let pid = child.id();
    crate::formation::reap_child(child);

    Ok(StartLoopResponse {
        ok: true,
        loop_id: Some(format!("loop-{}", pid)),
        pid: Some(pid),
        error: None,
    })
}

// ── Tests ────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn start_request_deserialize_with_member() {
        let json = r#"{"member": "superman"}"#;
        let req: StartMembersRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.member, Some("superman".to_string()));
    }

    #[test]
    fn start_request_deserialize_without_member() {
        let json = r#"{}"#;
        let req: StartMembersRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.member, None);
    }

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
    fn start_response_serialize() {
        let resp = StartMembersResponse {
            ok: true,
            launched: vec![MemberLaunchedInfo {
                name: "alice".to_string(),
                pid: 1234,
                brain_mode: false,
            }],
            skipped: vec![MemberSkippedInfo {
                name: "bob".to_string(),
                pid: 5678,
            }],
            errors: vec![],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["launched"][0]["name"], "alice");
        assert_eq!(json["launched"][0]["pid"], 1234);
        assert_eq!(json["skipped"][0]["name"], "bob");
        assert!(json["errors"].as_array().unwrap().is_empty());
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

    #[test]
    fn start_response_with_errors() {
        let resp = StartMembersResponse {
            ok: false,
            launched: vec![],
            skipped: vec![],
            errors: vec![MemberErrorInfo {
                name: "charlie".to_string(),
                error: "no workspace found".to_string(),
            }],
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], false);
        assert_eq!(json["errors"][0]["name"], "charlie");
        assert_eq!(json["errors"][0]["error"], "no workspace found");
    }

    #[test]
    fn start_loop_request_deserialize_with_member() {
        let json = r#"{"prompt": "Implement #1: fix bug", "member": "superman"}"#;
        let req: StartLoopRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Implement #1: fix bug");
        assert_eq!(req.member, Some("superman".to_string()));
    }

    #[test]
    fn start_loop_request_deserialize_without_member() {
        let json = r#"{"prompt": "Implement #2: add feature"}"#;
        let req: StartLoopRequest = serde_json::from_str(json).unwrap();
        assert_eq!(req.prompt, "Implement #2: add feature");
        assert_eq!(req.member, None);
    }

    #[test]
    fn start_loop_response_serialize_success() {
        let resp = StartLoopResponse {
            ok: true,
            loop_id: Some("loop-1234".to_string()),
            pid: Some(1234),
            error: None,
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], true);
        assert_eq!(json["loop_id"], "loop-1234");
        assert_eq!(json["pid"], 1234);
        assert!(json["error"].is_null());
    }

    #[test]
    fn start_loop_response_serialize_error() {
        let resp = StartLoopResponse {
            ok: false,
            loop_id: None,
            pid: None,
            error: Some("no workspace found".to_string()),
        };
        let json = serde_json::to_value(&resp).unwrap();
        assert_eq!(json["ok"], false);
        assert!(json["loop_id"].is_null());
        assert!(json["pid"].is_null());
        assert_eq!(json["error"], "no workspace found");
    }

    #[test]
    fn start_loop_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "loop_id": "loop-5678",
            "pid": 5678,
            "error": null
        });
        let resp: StartLoopResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.loop_id, Some("loop-5678".to_string()));
        assert_eq!(resp.pid, Some(5678));
        assert!(resp.error.is_none());
    }

    #[test]
    fn start_loop_request_serializes_for_client() {
        let req = StartLoopRequest {
            prompt: "Fix the tests".to_string(),
            member: Some("alice".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["prompt"], "Fix the tests");
        assert_eq!(parsed["member"], "alice");
    }
}
