use std::collections::HashSet;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use libc;

use crate::bridge;
use crate::session::manager::WorkspaceOps;
use crate::workspace::{HydrationWorkspaceConfig, HydrationWorkspaceOps};

/// Bridge credentials resolved at daemon startup, used to inject env vars when launching ralph.
pub struct BridgeContext {
    /// Bridge type name, e.g. "tuwunel", "rocketchat", "telegram".
    pub bridge_type_name: String,
    /// Path to bridge-state.json, read fresh on each launch to get current service_url.
    pub bstate_path: PathBuf,
    /// Per-member token store (reads from system keyring or env override).
    pub credential_store: bridge::LocalCredentialStore,
}

impl BridgeContext {
    /// Read the current service URL from bridge-state.json.
    pub fn service_url(&self) -> Option<String> {
        bridge::load_state(&self.bstate_path).ok()?.service_url
    }

    /// Resolve this member's bridge access token from the credential store.
    pub fn member_token(&self, member_name: &str) -> Option<String> {
        bridge::resolve_credential_from_store(member_name, &self.credential_store)
            .ok()
            .flatten()
    }

    /// Resolve this member's Matrix user_id from the bridge state.
    pub fn member_user_id(&self, member_name: &str) -> Option<String> {
        bridge::load_state(&self.bstate_path)
            .ok()?
            .identities
            .get(member_name)
            .map(|id| id.user_id.clone())
    }

    /// Resolve the admin Matrix user_id from the bridge state.
    pub fn admin_user_id(&self) -> Option<String> {
        bridge::load_state(&self.bstate_path).ok()?.admin_user_id
    }
}

use axum::extract::{Path, Query, State};
use axum::http::StatusCode;
use axum::routing::{delete, get, post};
use axum::{Json, Router};
use serde::{Deserialize, Serialize};

use crate::session::cleanup;
use crate::session::finalization::{deactivation as fin_deactivation, subagent as fin_subagent};
use crate::session::history::{self, ExitStatus};
use crate::session::registry::SessionRegistry;
use crate::session::retention::{self, ProcessChecker};
use crate::session::stop::{self, StopMode, StopOptions};
use crate::session::types::{
    FinalizationExitStatus, FinalizationResult, SessionId, SessionRecord, SessionState, SessionType,
};
use crate::session::work_item_lock::WorkItemLock;

struct SessionsInner {
    registry: SessionRegistry,
    work_item_lock: WorkItemLock,
}

/// Shared state for session management API handlers.
#[derive(Clone)]
pub struct SessionsApiState {
    inner: Arc<Mutex<SessionsInner>>,
    /// Workspace ops for hydrating ephemeral session workspaces.
    /// None in test mode or when workspace hydration is not configured.
    /// Stored outside the Mutex so blocking I/O does not hold the lock.
    workspace_ops: Option<Arc<HydrationWorkspaceOps>>,
    /// Bridge credentials resolved at startup for env injection into ralph processes.
    /// None when no bridge is configured.
    bridge_context: Option<Arc<BridgeContext>>,
}

impl SessionsApiState {
    pub fn new(registry_path: PathBuf) -> Self {
        Self {
            inner: Arc::new(Mutex::new(SessionsInner {
                registry: SessionRegistry::new(registry_path),
                work_item_lock: WorkItemLock::new(),
            })),
            workspace_ops: None,
            bridge_context: None,
        }
    }

    /// Production constructor that wires workspace hydration and optional bridge credentials.
    pub fn new_with_workspace_ops(
        registry_path: PathBuf,
        config: HydrationWorkspaceConfig,
        bridge_context: Option<BridgeContext>,
    ) -> Self {
        // Load persisted sessions from disk so daemon restart recovery can see prior sessions.
        let registry = SessionRegistry::load(registry_path.clone())
            .unwrap_or_else(|_| SessionRegistry::new(registry_path));
        Self {
            inner: Arc::new(Mutex::new(SessionsInner {
                registry,
                work_item_lock: WorkItemLock::new(),
            })),
            workspace_ops: Some(Arc::new(HydrationWorkspaceOps::new(config))),
            bridge_context: bridge_context.map(Arc::new),
        }
    }

    pub fn recover_stale_sessions(&self) -> Vec<SessionId> {
        let mut inner = self.inner.lock().unwrap();
        let checker = retention::LiveProcessChecker;
        retention::recover_stale_sessions(&mut inner.registry, &checker)
    }

    pub fn run_retention_cycle(&self) -> Vec<SessionId> {
        let mut inner = self.inner.lock().unwrap();
        let policy = retention::RetentionPolicy::default();
        let disk_usage = retention::FsDiskUsage;
        retention::run_cycle(&mut inner.registry, &policy, &disk_usage)
    }

    /// Send SIGTERM to all autonomous (non-Interactive) active sessions.
    /// Used during daemon shutdown before waiting for exit.
    pub fn stop_autonomous_sessions_gracefully(&self) {
        use crate::session::stop::{stop_sessions, StopMode, StopOptions};
        let options = StopOptions {
            mode: StopMode::AutonomousOnly,
            force: false,
        };
        let mut inner = self.inner.lock().unwrap();
        stop_sessions(&mut inner.registry, &options);
    }

    /// Send SIGKILL to all autonomous (non-Interactive) sessions still alive.
    /// Used as a last resort during daemon shutdown after SIGTERM + wait.
    pub fn force_stop_autonomous_sessions(&self) {
        use crate::session::stop::{stop_sessions, StopMode, StopOptions};
        let options = StopOptions {
            mode: StopMode::AutonomousOnly,
            force: true,
        };
        let mut inner = self.inner.lock().unwrap();
        stop_sessions(&mut inner.registry, &options);
    }

    /// Returns true if any autonomous session has a live agent process.
    pub fn has_alive_autonomous_sessions(&self) -> bool {
        use crate::session::types::SessionState;
        let checker = retention::LiveProcessChecker;
        let inner = self.inner.lock().unwrap();
        inner.registry.list().into_iter().any(|r| {
            matches!(
                r.current_state,
                SessionState::Active | SessionState::Finalizing
            ) && r.session_type != SessionType::Interactive
                && r.agent_pid.is_some_and(|pid| checker.is_pid_alive(pid))
        })
    }

    /// Starts a Loop session synchronously — called by the daemon poll/webhook handler.
    ///
    /// Replicates the `start_session_handler` flow without async.
    /// Returns the new `SessionId` on success, or an error string.
    ///
    /// Skips launch when the member already has an active Loop session with a live PID.
    pub fn start_loop_session_blocking(
        &self,
        member_name: &str,
    ) -> Result<SessionId, String> {
        tracing::debug!(member = %member_name, "Loop session blocking start");
        // Dedup: skip if member already has a live autonomous session.
        {
            let inner = self.inner.lock().unwrap();
            let checker = retention::LiveProcessChecker;
            let already_running = inner.registry.list().into_iter().any(|r| {
                r.member_name == member_name
                    && matches!(
                        r.current_state,
                        SessionState::Active | SessionState::Finalizing
                    )
                    && r.session_type != SessionType::Interactive
                    && r.agent_pid.is_some_and(|pid| checker.is_pid_alive(pid))
            });
            if already_running {
                tracing::debug!(member = %member_name, "Dedup: already has live autonomous session");
                return Err(format!(
                    "member {} already has a live autonomous session",
                    member_name
                ));
            }
        }

        let session_id = SessionId::new();

        // Step 1: Hydrate workspace synchronously (no Mutex held).
        let workspace_path: Option<std::path::PathBuf> =
            if let Some(ref ops) = self.workspace_ops {
                match ops.hydrate_workspace(&session_id, member_name) {
                    Ok(path) => {
                        tracing::debug!(session_id = %session_id, path = %path.display(), "Workspace hydrated (blocking)");
                        Some(path)
                    }
                    Err(e) => {
                        tracing::warn!(session_id = %session_id, member = %member_name, error = %e, "Workspace hydration failed (blocking)");
                        return Err(format!("workspace hydration failed: {e}"));
                    }
                }
            } else {
                None
            };

        // Step 2: Register session and transition to Active (under Mutex).
        {
            let mut inner = self.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: member_name.to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: workspace_path.clone(),
                finalization_result: None,
                finalization_agent_pid: None,
            };
            inner.registry.register(record).map_err(|e| e.to_string())?;
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .map_err(|e| e.to_string())?;
        }

        // Step 3: Launch ralph (synchronous, no Mutex held).
        let agent_pid: Option<u32> = if let Some(ref ws) = workspace_path {
            let bridge_type_name =
                self.bridge_context.as_ref().map(|bc| bc.bridge_type_name.clone());
            let service_url = self.bridge_context.as_ref().and_then(|bc| bc.service_url());
            let member_token = self
                .bridge_context
                .as_ref()
                .and_then(|bc| bc.member_token(member_name));
            let gh_config_dir = self
                .workspace_ops
                .as_ref()
                .and_then(|ops| ops.gh_config_dir_for_member(member_name));

            crate::formation::launch_ralph(
                ws,
                member_token.as_deref(),
                bridge_type_name.as_deref(),
                service_url.as_deref(),
                gh_config_dir.as_deref(),
            )
            .ok()
        } else {
            None
        };

        // Step 4: Persist agent PID (under Mutex).
        if let Some(pid) = agent_pid {
            tracing::info!(session_id = %session_id, pid = pid, "Agent launched (blocking)");
            let mut inner = self.inner.lock().unwrap();
            let _ = inner.registry.set_agent_pid(&session_id, pid);
        } else {
            tracing::warn!(session_id = %session_id, member = %member_name, "Agent launch returned no PID (blocking)");
        }

        Ok(session_id)
    }
}

// ── Request types ───────────────────────────────────────────────────────

/// Request body for `POST /api/sessions/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartSessionRequest {
    pub member_name: String,
    pub session_type: String,
    pub work_item_id: Option<String>,
}

// ── Response types ──────────────────────────────────────────────────────

/// Response for `POST /api/sessions/start`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StartSessionResponse {
    pub ok: bool,
    pub session_id: Option<String>,
    /// Absolute path to the hydrated ephemeral workspace on disk.
    /// None when workspace hydration is not configured (test mode).
    #[serde(default)]
    pub workspace_path: Option<String>,
    pub error: Option<String>,
}

/// A single session's summary info for list responses.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionInfo {
    pub session_id: String,
    pub member_name: String,
    pub session_type: String,
    pub current_state: String,
    pub started_at: String,
    #[serde(default)]
    pub state_transitioned_at: Option<String>,
    #[serde(default)]
    pub concurrent_count: Option<u32>,
}

/// Response for `GET /api/sessions`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionListResponse {
    pub sessions: Vec<SessionInfo>,
}

/// Response for `GET /api/sessions/:id`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionDetailResponse {
    pub ok: bool,
    pub session: Option<SessionInfo>,
    pub error: Option<String>,
}

/// Optional request body for `POST /api/sessions/:id/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopSessionRequest {
    #[serde(default)]
    pub force: bool,
}

/// Query parameters for `GET /api/sessions/history`.
#[derive(Debug, Deserialize)]
pub struct SessionHistoryQueryParams {
    pub member: Option<String>,
    pub since: Option<String>,
}

/// Response for `POST /api/sessions/:id/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopSessionResponse {
    pub ok: bool,
    pub error: Option<String>,
}

/// Request body for `POST /api/sessions/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopBulkRequest {
    pub mode: String,
    pub member: Option<String>,
    #[serde(default)]
    pub force: bool,
}

/// Response for `POST /api/sessions/stop`.
#[derive(Debug, Serialize, Deserialize)]
pub struct StopBulkResponse {
    pub ok: bool,
    pub deactivated: usize,
    pub killed: usize,
    pub skipped_interactive: usize,
    pub errors: Vec<String>,
    pub error: Option<String>,
}

/// Response for `POST /api/sessions/:id/finalize`.
#[derive(Debug, Serialize, Deserialize)]
pub struct RetriggerResponse {
    pub ok: bool,
    pub error: Option<String>,
}

/// A single session's history entry for the history endpoint.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionHistoryInfo {
    pub session_id: String,
    pub member_name: String,
    pub session_type: String,
    pub start_time: String,
    pub end_time: String,
    pub exit_normal: bool,
    /// Finalization outcome: "completed", "failed", "skipped", "pending", or "n/a".
    #[serde(default)]
    pub finalization_status: String,
}

/// Response for `GET /api/sessions/history`.
#[derive(Debug, Serialize, Deserialize)]
pub struct SessionHistoryResponse {
    pub sessions: Vec<SessionHistoryInfo>,
}

/// Response for `GET /api/sessions/:id/inspect`.
#[derive(Debug, Serialize, Deserialize)]
pub struct InspectSessionResponse {
    pub ok: bool,
    pub session_id: Option<String>,
    pub member_name: Option<String>,
    pub session_type: Option<String>,
    pub current_state: Option<String>,
    pub workspace_path: Option<String>,
    pub finalization_results: Option<serde_json::Value>,
    pub git_state: Option<serde_json::Value>,
    pub error: Option<String>,
}

/// Response for `DELETE /api/sessions/:id`.
#[derive(Debug, Serialize, Deserialize)]
pub struct CleanupSessionResponse {
    pub ok: bool,
    pub session_id: Option<String>,
    pub workspace_removed: bool,
    pub registry_removed: bool,
    pub error: Option<String>,
}

/// Request body for `POST /api/sessions/cleanup`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkCleanupRequest {
    pub filter: String,
    pub value: Option<String>,
}

/// Per-session report in bulk cleanup response.
#[derive(Debug, Serialize, Deserialize)]
pub struct CleanupReportInfo {
    pub session_id: String,
    pub workspace_removed: bool,
    pub registry_removed: bool,
}

/// Response for `POST /api/sessions/cleanup`.
#[derive(Debug, Serialize, Deserialize)]
pub struct BulkCleanupResponse {
    pub ok: bool,
    pub cleaned: u32,
    pub reports: Vec<CleanupReportInfo>,
    pub error: Option<String>,
}

/// Session summary for the web console operator API (`GET /api/teams/:team/sessions`).
#[derive(Debug, Serialize)]
pub struct ConsoleSessionSummary {
    pub session_id: String,
    pub member_name: String,
    pub state: String,
    pub session_type: String,
    pub created_at: String,
    pub finalization_status: String,
}

// ── Work-item lock ───────────────────────────────────────────────────────

#[derive(Debug, Serialize, Deserialize)]
pub struct AcquireLockRequest {
    pub work_item_id: String,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct AcquireLockResponse {
    pub acquired: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub holder: Option<String>,
}

#[derive(Debug, Serialize, Deserialize)]
pub struct ReleaseLockResponse {
    pub released: bool,
}

// ── Helpers ────────────────────────────────────────────────────────────

fn record_to_info(r: &SessionRecord) -> SessionInfo {
    SessionInfo {
        session_id: r.session_id.to_string(),
        member_name: r.member_name.clone(),
        session_type: r.session_type.to_string(),
        current_state: r.current_state.to_string(),
        started_at: r.created_at.to_rfc3339(),
        state_transitioned_at: Some(r.state_transitioned_at.to_rfc3339()),
        concurrent_count: None,
    }
}

fn finalization_status_str(status: Option<&crate::session::types::FinalizationExitStatus>) -> &'static str {
    use crate::session::types::FinalizationExitStatus;
    match status {
        Some(FinalizationExitStatus::Completed | FinalizationExitStatus::CompletedDegraded) => "completed",
        Some(FinalizationExitStatus::Failed) => "failed",
        Some(FinalizationExitStatus::Skipped) => "skipped",
        None => "n/a",
    }
}

// ── Handlers ────────────────────────────────────────────────────────────

/// GET /api/sessions/history — lists terminal sessions as history.
pub async fn list_session_history_handler(
    State(state): State<SessionsApiState>,
    Query(params): Query<SessionHistoryQueryParams>,
) -> (StatusCode, Json<SessionHistoryResponse>) {
    let inner = state.inner.lock().unwrap();
    let refs = inner.registry.list();

    let fin_map: std::collections::HashMap<String, Option<crate::session::types::FinalizationExitStatus>> = refs
        .iter()
        .map(|r| {
            (
                r.session_id.to_string(),
                r.finalization_result.as_ref().map(|f| f.exit_status.clone()),
            )
        })
        .collect();

    let since = params.since.as_deref().and_then(|s| {
        chrono::DateTime::parse_from_rfc3339(s)
            .ok()
            .map(|dt| dt.with_timezone(&chrono::Utc))
    });
    let query = history::SessionHistoryQuery {
        member: params.member,
        since,
    };

    let history_entries = history::query_history(&refs, &query);
    tracing::debug!(count = history_entries.len(), "Session history queried");
    let sessions = history_entries
        .into_iter()
        .map(|e| {
            let finalization_status =
                finalization_status_str(fin_map.get(&e.session_id).and_then(|f| f.as_ref()))
                    .to_string();
            SessionHistoryInfo {
                session_id: e.session_id,
                member_name: e.member,
                session_type: e.session_type,
                start_time: e.start_time.to_rfc3339(),
                end_time: e.end_time.to_rfc3339(),
                exit_normal: e.exit_status == ExitStatus::Normal,
                finalization_status,
            }
        })
        .collect();

    (StatusCode::OK, Json(SessionHistoryResponse { sessions }))
}

/// POST /api/sessions/start — creates a new ephemeral session with workspace hydration.
///
/// Flow:
/// 1. Acquire work_item_lock (under Mutex) — prevents duplicate work-item sessions.
/// 2. Hydrate workspace (blocking git I/O, no Mutex held) — creates ephemeral worktrees.
/// 3. Register session record with workspace_path (under Mutex) → Active.
/// 4. Launch agent process (blocking, no Mutex held) — ralph for Loop, brain-run for Brain.
/// 5. Persist agent PID (under Mutex).
pub async fn start_session_handler(
    State(state): State<SessionsApiState>,
    Json(req): Json<StartSessionRequest>,
) -> (StatusCode, Json<StartSessionResponse>) {
    let session_type: SessionType = match req.session_type.parse() {
        Ok(t) => t,
        Err(e) => {
            return (
                StatusCode::BAD_REQUEST,
                Json(StartSessionResponse {
                    ok: false,
                    session_id: None,
                    workspace_path: None,
                    error: Some(e.to_string()),
                }),
            );
        }
    };

    let session_id = SessionId::new();
    tracing::info!(
        session_id = %session_id,
        member = %req.member_name,
        session_type = %req.session_type,
        work_item = req.work_item_id.as_deref().unwrap_or("none"),
        "Session create requested"
    );

    // Step 1: Acquire work_item_lock (under Mutex) then release the lock before I/O.
    if let Some(ref work_item_id) = req.work_item_id {
        let inner = state.inner.lock().unwrap();
        if let Err(e) = inner.work_item_lock.acquire(work_item_id, &session_id) {
            return (
                StatusCode::CONFLICT,
                Json(StartSessionResponse {
                    ok: false,
                    session_id: None,
                    workspace_path: None,
                    error: Some(e.to_string()),
                }),
            );
        }
    }
    // Mutex released. workspace hydration runs without holding the lock.

    // Step 2: Hydrate workspace (blocking git I/O — worktree provisioning + config assembly).
    let workspace_path: Option<PathBuf> = if let Some(ref ops) = state.workspace_ops {
        let ops_ref = Arc::clone(ops);
        let sid = session_id.clone();
        let member = req.member_name.clone();
        let result = tokio::task::spawn_blocking(move || {
            ops_ref.hydrate_workspace(&sid, &member)
        })
        .await;

        match result {
            Ok(Ok(path)) => {
                tracing::debug!(session_id = %session_id, path = %path.display(), "Workspace hydrated");
                Some(path)
            }
            Ok(Err(e)) => {
                tracing::warn!(session_id = %session_id, error = %e, "Workspace hydration failed");
                if let Some(ref work_item_id) = req.work_item_id {
                    let inner = state.inner.lock().unwrap();
                    inner.work_item_lock.release(work_item_id, &session_id);
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(StartSessionResponse {
                        ok: false,
                        session_id: None,
                        workspace_path: None,
                        error: Some(format!("Workspace hydration failed: {e}")),
                    }),
                );
            }
            Err(e) => {
                tracing::warn!(session_id = %session_id, error = %e, "Workspace hydration task panicked");
                if let Some(ref work_item_id) = req.work_item_id {
                    let inner = state.inner.lock().unwrap();
                    inner.work_item_lock.release(work_item_id, &session_id);
                }
                return (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    Json(StartSessionResponse {
                        ok: false,
                        session_id: None,
                        workspace_path: None,
                        error: Some(format!("Workspace hydration task panicked: {e}")),
                    }),
                );
            }
        }
    } else {
        None
    };

    // Auto-detect brain mode from the assembled session workspace.
    // ConfigAssembler copies brain-prompt.md from team_repo/members/<member>/brain-prompt.md
    // (if it exists) during hydration — same path as PROMPT.md and CLAUDE.md.
    let session_type = if session_type == SessionType::Loop {
        if let Some(ref ws_path) = workspace_path {
            if crate::formation::is_brain_member(ws_path) {
                SessionType::Brain
            } else {
                session_type
            }
        } else {
            session_type
        }
    } else {
        session_type
    };
    tracing::debug!(session_id = %session_id, resolved_type = %session_type, "Session type resolved");

    // Step 3: Register session with workspace_path and transition to Active (under Mutex).
    {
        let mut inner = state.inner.lock().unwrap();
        let now = chrono::Utc::now();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: req.member_name.clone(),
            session_type: session_type.clone(),
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: workspace_path.clone(),
            finalization_result: None,
            finalization_agent_pid: None,
        };

        if let Err(e) = inner.registry.register(record) {
            if let Some(ref work_item_id) = req.work_item_id {
                inner.work_item_lock.release(work_item_id, &session_id);
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StartSessionResponse {
                    ok: false,
                    session_id: None,
                    workspace_path: None,
                    error: Some(e.to_string()),
                }),
            );
        }

        if let Err(e) = inner
            .registry
            .update_state(&session_id, SessionState::Active)
        {
            if let Some(ref work_item_id) = req.work_item_id {
                inner.work_item_lock.release(work_item_id, &session_id);
            }
            return (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(StartSessionResponse {
                    ok: false,
                    session_id: None,
                    workspace_path: None,
                    error: Some(e.to_string()),
                }),
            );
        }
    }
    tracing::debug!(session_id = %session_id, "Session registered and active");
    // Mutex released. Agent launch runs without holding the lock.

    // Step 4: Launch agent process (blocking, no Mutex held).
    // Loop → ralph run -p PROMPT.md in workspace
    // Brain → bm brain-run in workspace
    // Interactive → no agent (user connects via bm chat)
    //
    // Resolve bridge credentials once, outside the closure.
    let bridge_type_name: Option<String>;
    let service_url: Option<String>;
    let member_token: Option<String>;
    let brain_user_id: Option<String>;
    let brain_operator_user_id: Option<String>;
    if let Some(ref bc) = state.bridge_context {
        bridge_type_name = Some(bc.bridge_type_name.clone());
        service_url = bc.service_url();
        member_token = bc.member_token(&req.member_name);
        brain_user_id = bc.member_user_id(&req.member_name);
        brain_operator_user_id = bc.admin_user_id();
    } else {
        bridge_type_name = None;
        service_url = None;
        member_token = None;
        brain_user_id = None;
        brain_operator_user_id = None;
    }

    let gh_config_dir: Option<PathBuf> = state
        .workspace_ops
        .as_ref()
        .and_then(|ops| ops.gh_config_dir_for_member(&req.member_name));

    let member_state_dir: Option<PathBuf> = state
        .workspace_ops
        .as_ref()
        .map(|ops| ops.member_state_dir_for_member(&req.member_name));

    // Gather info needed to render brain-prompt.md into the session workspace.
    // surface_brain_prompt() reads the template from team_repo/brain/system-prompt.md
    // and writes the rendered result to session_workspace/brain-prompt.md, which
    // brain_run reads at startup. Without this, the brain crashes immediately.
    let brain_render_info: Option<(PathBuf, crate::brain::BrainPromptVars)> =
        if session_type == SessionType::Brain {
            state.workspace_ops.as_ref().and_then(|ops| {
                let team_repo = ops.team_repo_path();
                let team_name = team_repo
                    .parent()
                    .and_then(|p| p.file_name())
                    .and_then(|n| n.to_str())
                    .unwrap_or("team")
                    .to_string();
                let role = crate::brain::read_member_role(&team_repo, &req.member_name)
                    .unwrap_or_else(|| "engineer".to_string());
                let member_name = crate::brain::read_member_name(&team_repo, &req.member_name);
                // Parse org/repo from team_repo_url (https://github.com/org/repo.git)
                let url = ops.team_repo_url().to_string();
                let gh_str = url
                    .trim_start_matches("https://github.com/")
                    .trim_end_matches(".git");
                crate::brain::parse_github_repo(gh_str).map(|(org, repo)| {
                    let vars = crate::brain::BrainPromptVars {
                        member_name,
                        team_name,
                        role,
                        gh_org: org.to_string(),
                        gh_repo: repo.to_string(),
                    };
                    (team_repo, vars)
                })
            })
        } else {
            None
        };

    let agent_pid: Option<u32> = if let Some(ref ws) = workspace_path {
        let ws_owned = ws.clone();
        let st = session_type.clone();

        tokio::task::spawn_blocking(move || -> Option<u32> {
            match st {
                SessionType::Loop => {
                    crate::formation::launch_ralph(
                        &ws_owned,
                        member_token.as_deref(),
                        bridge_type_name.as_deref(),
                        service_url.as_deref(),
                        gh_config_dir.as_deref(),
                    ).ok()
                }
                SessionType::Brain => {
                    // Render brain-prompt.md from team repo template into session workspace.
                    if let Some((ref team_repo, ref vars)) = brain_render_info {
                        if let Err(e) = crate::brain::surface_brain_prompt(team_repo, &ws_owned, vars) {
                            tracing::warn!(
                                member = vars.member_name.as_str(),
                                error = %e,
                                "brain-prompt.md render failed — brain process will crash at startup"
                            );
                        }
                    }
                    let system_prompt = ws_owned.join("brain-prompt.md");
                    let brain_cfg = crate::formation::BrainLaunchConfig {
                        workspace: &ws_owned,
                        system_prompt_path: &system_prompt,
                        member_token: member_token.as_deref(),
                        bridge_type: bridge_type_name.as_deref(),
                        service_url: service_url.as_deref(),
                        room_id: None,
                        user_id: brain_user_id.as_deref(),
                        operator_user_id: brain_operator_user_id.as_deref(),
                        team_repo: None,
                        gh_config_dir: gh_config_dir.as_deref(),
                        member_state_dir: member_state_dir.as_deref(),
                    };
                    crate::formation::launch_brain(&brain_cfg).ok()
                }
                SessionType::Interactive => None,
            }
        })
        .await
        .ok()
        .flatten()
    } else {
        None
    };

    // Step 5: Persist agent PID (under Mutex).
    if let Some(pid) = agent_pid {
        tracing::info!(session_id = %session_id, pid = pid, session_type = %session_type, "Agent launched");
        let mut inner = state.inner.lock().unwrap();
        let _ = inner.registry.set_agent_pid(&session_id, pid);
    } else if session_type != SessionType::Interactive {
        tracing::warn!(session_id = %session_id, session_type = %session_type, "Agent launch returned no PID");
    }

    let workspace_path_str = workspace_path.map(|p| p.display().to_string());

    (
        StatusCode::OK,
        Json(StartSessionResponse {
            ok: true,
            session_id: Some(session_id.to_string()),
            workspace_path: workspace_path_str,
            error: None,
        }),
    )
}

/// GET /api/sessions — lists active sessions.
pub async fn list_sessions_handler(
    State(state): State<SessionsApiState>,
) -> (StatusCode, Json<SessionListResponse>) {
    let mut inner = state.inner.lock().unwrap();

    // Detect agent processes that crashed since the last check.
    // Only Active sessions — Finalizing sessions have an intentionally dead PID (SIGTERM was
    // sent during graceful stop). The spawn_deactivation_watcher owns Finalizing → Completed/Failed.
    let checker = retention::LiveProcessChecker;
    let crashed_ids: Vec<_> = inner
        .registry
        .list()
        .into_iter()
        .filter(|r| {
            r.current_state == SessionState::Active
                && r.agent_pid.is_some_and(|pid| !checker.is_pid_alive(pid))
        })
        .map(|r| r.session_id.clone())
        .collect();
    if !crashed_ids.is_empty() {
        tracing::debug!(count = crashed_ids.len(), ids = ?crashed_ids, "Crash-detected sessions marked Failed");
    }
    for id in &crashed_ids {
        let _ = inner.registry.update_state(id, SessionState::Failed);
    }

    let refs = inner.registry.list();
    let sessions = refs
        .iter()
        .map(|r| {
            let count = history::compute_concurrent_count(&refs, &r.member_name);
            let mut info = record_to_info(r);
            info.concurrent_count = Some(count);
            info
        })
        .collect();

    (StatusCode::OK, Json(SessionListResponse { sessions }))
}

/// POST /api/sessions/:id/stop — stops a specific session via the stop module.
///
/// Accepts an optional JSON body with `{ "force": true }` to force-kill
/// instead of graceful deactivation.
/// Spawn a background tokio task that waits for a gracefully-stopped agent to exit,
/// inspects workspace dirty state, and triggers the finalization subagent if needed.
///
/// Called after graceful stop transitions a session to `Finalizing`. The watcher
/// transitions the session to `Completed` (clean or finalization exit 0) or
/// `Failed` (finalization exit non-zero or spawn failure).
fn spawn_deactivation_watcher(
    session_id: SessionId,
    workspace_path: Option<PathBuf>,
    agent_pid: Option<u32>,
    arc_inner: Arc<Mutex<SessionsInner>>,
    workspace_ops: Option<Arc<HydrationWorkspaceOps>>,
) {
    use std::time::Duration;
    tracing::debug!(session_id = session_id.as_str(), agent_pid = ?agent_pid, "Deactivation watcher spawned");
    tokio::spawn(async move {
        // Wait up to 10s for the agent process to exit after SIGTERM, then SIGKILL.
        // A 60s grace period left zero margin for the 120s finalization subagent within
        // D22's 180s window; 10s + SIGKILL gives ~168s for finalization to complete.
        if let Some(pid) = agent_pid {
            let graceful_deadline = tokio::time::Instant::now() + Duration::from_secs(10);
            loop {
                let alive = unsafe { libc::kill(pid as i32, 0) } == 0;
                if !alive {
                    break;
                }
                if tokio::time::Instant::now() >= graceful_deadline {
                    tracing::warn!(
                        session_id = session_id.as_str(),
                        "agent PID {} did not exit within 10s after SIGTERM; sending SIGKILL",
                        pid
                    );
                    // SAFETY: valid PID, SIGKILL is a valid signal. The reap_child thread
                    // from launch_brain handles the wait() call to prevent a zombie.
                    unsafe { libc::kill(pid as i32, libc::SIGKILL); }
                    // Wait briefly for the kernel to deliver SIGKILL before inspecting state.
                    for _ in 0..10u32 {
                        tokio::time::sleep(Duration::from_millis(200)).await;
                        if unsafe { libc::kill(pid as i32, 0) } != 0 {
                            break;
                        }
                    }
                    break;
                }
                tokio::time::sleep(Duration::from_millis(500)).await;
            }
        }

        // Re-read the session state. A concurrent force-stop may have moved it to Retained
        // while we were waiting for the agent to exit. Only proceed if still Finalizing.
        let still_finalizing = {
            if let Ok(guard) = arc_inner.lock() {
                guard
                    .registry
                    .get(&session_id)
                    .map(|r| r.current_state == SessionState::Finalizing)
                    .unwrap_or(false)
            } else {
                false
            }
        };

        if !still_finalizing {
            tracing::debug!(session_id = session_id.as_str(), "No longer Finalizing — skipping deactivation");
            return;
        }

        let Some(ws) = workspace_path else {
            // No workspace — transition to Completed only if still Finalizing (force-stop
            // sends SIGKILL before updating state, so the process can die while the registry
            // still shows Finalizing; the state update to Retained arrives moments later).
            if let Ok(mut guard) = arc_inner.lock() {
                if guard
                    .registry
                    .get(&session_id)
                    .map(|r| r.current_state == SessionState::Finalizing)
                    .unwrap_or(false)
                {
                    let _ = guard.registry.update_state(&session_id, SessionState::Completed);
                    let _ = guard.registry.set_finalization_result(
                        &session_id,
                        FinalizationResult::for_state(FinalizationExitStatus::Skipped),
                    );
                }
            }
            return;
        };

        // Inspect dirty state to decide whether to run the finalization subagent.
        let dirty_state = if let Some(ref ops) = workspace_ops {
            ops.inspect_dirty_state(&ws).unwrap_or_default()
        } else {
            vec![]
        };

        if fin_deactivation::has_committable_files(&ws, &dirty_state) {
            // Fast path: attempt a direct `git push origin --all` for repos that only have
            // committed-but-unpushed branches (the common D22 case). This avoids spawning the
            // LLM finalization agent when a plain git push is sufficient, keeping finalization
            // well inside D22's 180s polling window regardless of Vertex AI availability.
            let ws_push = ws.clone();
            let dirty_push = dirty_state.clone();
            let push_result = tokio::task::spawn_blocking(move || {
                fin_deactivation::push_unpushed_repos(&ws_push, &dirty_push)
            })
            .await;

            // Re-inspect after push to see if uncommitted files still require the LLM agent.
            // Do NOT re-check unpushed_branches: bare-clone worktrees have no refs/remotes/,
            // so inspect_unpushed always reports all branches as "not in remotes" even after
            // a successful push. Checking only uncommitted files avoids a false-positive that
            // would otherwise force every session through the 160 s LLM timeout.
            let still_committable = match push_result {
                Ok(Ok(())) => {
                    let fresh = if let Some(ref ops) = workspace_ops {
                        ops.inspect_dirty_state(&ws).unwrap_or_default()
                    } else {
                        vec![]
                    };
                    fin_deactivation::has_uncommitted_committable_files(&ws, &fresh)
                }
                Ok(Err(ref e)) => {
                    tracing::info!(
                        session_id = session_id.as_str(),
                        "direct git push failed ({}); falling through to LLM finalization agent",
                        e
                    );
                    true
                }
                Err(ref join_err) => {
                    tracing::warn!(
                        session_id = session_id.as_str(),
                        "push_unpushed_repos task panicked: {join_err}; falling through to LLM agent"
                    );
                    true
                }
            };

            if !still_committable {
                // Fast path handled all dirty state — transition directly to Completed.
                tracing::info!(
                    session_id = session_id.as_str(),
                    "direct git push succeeded; transitioning to Completed without LLM agent"
                );
                if let Ok(mut guard) = arc_inner.lock() {
                    if guard
                        .registry
                        .get(&session_id)
                        .map(|r| r.current_state == SessionState::Finalizing)
                        .unwrap_or(false)
                    {
                        let _ = guard.registry.update_state(&session_id, SessionState::Completed);
                        let _ = guard.registry.set_finalization_result(
                            &session_id,
                            FinalizationResult::for_state(FinalizationExitStatus::Completed),
                        );
                    }
                }
            } else {
                // Direct push insufficient (uncommitted files, push failed, etc.) — spawn LLM agent.
                match fin_deactivation::retrigger_finalization(&session_id, &ws) {
                    Ok(child) => {
                        // Record the finalization agent PID so force-stop can kill it.
                        // Without this, force-stop sends SIGKILL to the stale brain PID,
                        // leaving abandoned finalization agents running until their timeout.
                        let fin_pid = child.id();
                        if let Ok(mut guard) = arc_inner.lock() {
                            let _ = guard
                                .registry
                                .set_finalization_agent_pid(&session_id, fin_pid);
                        }
                        let timeout = Duration::from_secs(fin_subagent::FINALIZATION_TIMEOUT_SECS);
                        let sid = session_id.clone();
                        fin_subagent::wait_and_transition(
                            child,
                            sid.clone(),
                            timeout,
                            move |new_state| {
                                if let Ok(mut guard) = arc_inner.lock() {
                                    // Conditional: don't overwrite Retained if force-stop raced.
                                    if guard
                                        .registry
                                        .get(&sid)
                                        .map(|r| r.current_state == SessionState::Finalizing)
                                        .unwrap_or(false)
                                    {
                                        let exit_status = match new_state {
                                            SessionState::Completed => FinalizationExitStatus::Completed,
                                            _ => FinalizationExitStatus::Failed,
                                        };
                                        let _ = guard.registry.update_state(&sid, new_state);
                                        let _ = guard.registry.set_finalization_result(
                                            &sid,
                                            FinalizationResult::for_state(exit_status),
                                        );
                                    }
                                }
                            },
                        )
                        .await;
                    }
                    Err(e) => {
                        tracing::error!(
                            session_id = session_id.as_str(),
                            "failed to spawn finalization subagent during deactivation: {}",
                            e
                        );
                        if let Ok(mut guard) = arc_inner.lock() {
                            if guard
                                .registry
                                .get(&session_id)
                                .map(|r| r.current_state == SessionState::Finalizing)
                                .unwrap_or(false)
                            {
                                let _ = guard
                                    .registry
                                    .update_state(&session_id, SessionState::Failed);
                                let _ = guard.registry.set_finalization_result(
                                    &session_id,
                                    FinalizationResult::for_state(FinalizationExitStatus::Failed),
                                );
                            }
                        }
                    }
                }
            }
        } else {
            tracing::debug!(session_id = session_id.as_str(), "Workspace clean — skipping finalization");
            // Clean workspace — transition to Completed only if still Finalizing.
            if let Ok(mut guard) = arc_inner.lock() {
                if guard
                    .registry
                    .get(&session_id)
                    .map(|r| r.current_state == SessionState::Finalizing)
                    .unwrap_or(false)
                {
                    let _ = guard.registry.update_state(&session_id, SessionState::Completed);
                    let _ = guard.registry.set_finalization_result(
                        &session_id,
                        FinalizationResult::for_state(FinalizationExitStatus::Skipped),
                    );
                }
            }
        }
    });
}

/// Collect sessions that just transitioned to Finalizing during a stop call.
fn collect_newly_finalizing(
    registry: &SessionRegistry,
    pre_finalizing: &HashSet<SessionId>,
) -> Vec<(SessionId, Option<PathBuf>, Option<u32>)> {
    registry
        .list()
        .iter()
        .filter(|r| {
            r.current_state == SessionState::Finalizing
                && !pre_finalizing.contains(&r.session_id)
        })
        .map(|r| (r.session_id.clone(), r.workspace_path.clone(), r.agent_pid))
        .collect()
}

/// Spawn deactivation watchers for sessions that just entered Finalizing.
fn spawn_deactivation_watchers(
    newly_finalizing: Vec<(SessionId, Option<PathBuf>, Option<u32>)>,
    state: &SessionsApiState,
) {
    tracing::debug!(
        newly_finalizing = newly_finalizing.len(),
        "spawning deactivation watchers"
    );
    for (session_id, workspace_path, agent_pid) in newly_finalizing {
        spawn_deactivation_watcher(
            session_id,
            workspace_path,
            agent_pid,
            Arc::clone(&state.inner),
            state.workspace_ops.clone(),
        );
    }
}

pub async fn stop_session_handler(
    State(state): State<SessionsApiState>,
    Path(session_id_str): Path<String>,
    body: Option<Json<StopSessionRequest>>,
) -> (StatusCode, Json<StopSessionResponse>) {
    let mut inner = state.inner.lock().unwrap();
    let session_id = SessionId::from_raw(&session_id_str);

    if inner.registry.get(&session_id).is_none() {
        tracing::debug!(session_id = %session_id_str, "Stop requested for unknown session");
        return (
            StatusCode::NOT_FOUND,
            Json(StopSessionResponse {
                ok: false,
                error: Some(format!("Session {} not found", session_id_str)),
            }),
        );
    }

    let force = body.is_some_and(|b| b.force);
    tracing::info!(session_id = %session_id_str, force = force, "Session stop requested");
    let options = StopOptions {
        mode: StopMode::SpecificSession(session_id.clone()),
        force,
    };

    // Snapshot sessions already in Finalizing so we don't double-watch them.
    let pre_finalizing: HashSet<SessionId> = inner
        .registry
        .list()
        .iter()
        .filter(|r| r.current_state == SessionState::Finalizing)
        .map(|r| r.session_id.clone())
        .collect();

    let summary = stop::stop_sessions(&mut inner.registry, &options);

    if !summary.errors.is_empty() {
        return (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(StopSessionResponse {
                ok: false,
                error: Some(summary.errors.join("; ")),
            }),
        );
    }

    let newly_finalizing = collect_newly_finalizing(&inner.registry, &pre_finalizing);

    inner.work_item_lock.release_all(&session_id);
    tracing::info!(session_id = %session_id_str, "Session stopped");

    // Spawn deactivation watchers after releasing the mutex.
    drop(inner);

    spawn_deactivation_watchers(newly_finalizing, &state);

    (
        StatusCode::OK,
        Json(StopSessionResponse {
            ok: true,
            error: None,
        }),
    )
}

/// POST /api/sessions/stop — bulk stop by member or autonomous mode.
pub async fn stop_bulk_handler(
    State(state): State<SessionsApiState>,
    Json(req): Json<StopBulkRequest>,
) -> (StatusCode, Json<StopBulkResponse>) {
    let mut inner = state.inner.lock().unwrap();
    tracing::info!(mode = %req.mode, member = req.member.as_deref().unwrap_or("all"), force = req.force, "Bulk stop requested");

    let mode = match req.mode.as_str() {
        "member" => match req.member {
            Some(name) => StopMode::AllForMember(name),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(StopBulkResponse {
                        ok: false,
                        deactivated: 0,
                        killed: 0,
                        skipped_interactive: 0,
                        errors: vec![],
                        error: Some("member mode requires a 'member' field".to_string()),
                    }),
                )
            }
        },
        "autonomous" => StopMode::AutonomousOnly,
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(StopBulkResponse {
                    ok: false,
                    deactivated: 0,
                    killed: 0,
                    skipped_interactive: 0,
                    errors: vec![],
                    error: Some(format!("unknown mode: {other}")),
                }),
            )
        }
    };

    let options = StopOptions {
        mode,
        force: req.force,
    };

    // Snapshot sessions already in Finalizing so we don't double-watch them.
    let pre_finalizing: HashSet<SessionId> = inner
        .registry
        .list()
        .iter()
        .filter(|r| r.current_state == SessionState::Finalizing)
        .map(|r| r.session_id.clone())
        .collect();

    let summary = stop::stop_sessions(&mut inner.registry, &options);

    let newly_finalizing = collect_newly_finalizing(&inner.registry, &pre_finalizing);

    let stopped_ids: Vec<SessionId> = inner
        .registry
        .list()
        .iter()
        .filter(|r| {
            matches!(
                r.current_state,
                SessionState::Finalizing | SessionState::Killed
            )
        })
        .map(|r| r.session_id.clone())
        .collect();

    for id in &stopped_ids {
        inner.work_item_lock.release_all(id);
    }

    // Spawn deactivation watchers after releasing the mutex.
    drop(inner);

    spawn_deactivation_watchers(newly_finalizing, &state);

    (
        StatusCode::OK,
        Json(StopBulkResponse {
            ok: true,
            deactivated: summary.deactivated,
            killed: summary.killed,
            skipped_interactive: summary.skipped_interactive,
            errors: summary.errors,
            error: None,
        }),
    )
}

/// POST /api/sessions/:id/finalize — re-trigger finalization on a retained session.
pub async fn retrigger_finalization_handler(
    State(state): State<SessionsApiState>,
    Path(session_id_str): Path<String>,
) -> (StatusCode, Json<RetriggerResponse>) {
    let session_id = SessionId::from_raw(&session_id_str);
    tracing::info!(session_id = %session_id_str, "Finalization retrigger requested");

    // Acquire lock only long enough to transition state and get the child handle.
    let retrigger_result = {
        let mut inner = state.inner.lock().unwrap();
        stop::retrigger_session_finalization(&mut inner.registry, &session_id)
    };

    match retrigger_result {
        Ok((_, child)) => {
            if let Some(child) = child {
                let arc_inner = Arc::clone(&state.inner);
                let sid = session_id.clone();
                let timeout =
                    std::time::Duration::from_secs(fin_subagent::FINALIZATION_TIMEOUT_SECS);
                tokio::spawn(fin_subagent::wait_and_transition(
                    child,
                    sid.clone(),
                    timeout,
                    move |new_state| {
                        if let Ok(mut guard) = arc_inner.lock() {
                            if let Err(e) = guard.registry.update_state(&sid, new_state) {
                                tracing::error!(
                                    session_id = sid.as_str(),
                                    "failed to update session state after finalization: {}",
                                    e
                                );
                            }
                        }
                    },
                ));
            }
            (StatusCode::OK, Json(RetriggerResponse { ok: true, error: None }))
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(RetriggerResponse {
                ok: false,
                error: Some(e.to_string()),
            }),
        ),
    }
}

/// GET /api/sessions/:id — returns full session details.
pub async fn session_detail_handler(
    State(state): State<SessionsApiState>,
    Path(session_id_str): Path<String>,
) -> (StatusCode, Json<SessionDetailResponse>) {
    let inner = state.inner.lock().unwrap();
    let session_id = SessionId::from_raw(&session_id_str);
    tracing::debug!(session_id = %session_id_str, "Session detail requested");

    match inner.registry.get(&session_id) {
        Some(r) => (
            StatusCode::OK,
            Json(SessionDetailResponse {
                ok: true,
                session: Some(record_to_info(r)),
                error: None,
            }),
        ),
        None => (
            StatusCode::NOT_FOUND,
            Json(SessionDetailResponse {
                ok: false,
                session: None,
                error: Some(format!("Session {} not found", session_id_str)),
            }),
        ),
    }
}

/// GET /api/sessions/:id/inspect — returns detailed inspection with finalization and git state.
pub async fn inspect_session_handler(
    State(state): State<SessionsApiState>,
    Path(session_id_str): Path<String>,
) -> (StatusCode, Json<InspectSessionResponse>) {
    let session_id = SessionId::from_raw(&session_id_str);
    tracing::debug!(session_id = %session_id_str, "Session inspect requested");

    // Clone data out of the Mutex before doing blocking I/O (git commands).
    let record_snapshot = {
        let inner = state.inner.lock().unwrap();
        inner.registry.get(&session_id).cloned()
    };

    match record_snapshot {
        Some(record) => {
            let finalization_results = record
                .finalization_result
                .as_ref()
                .and_then(|f| serde_json::to_value(f).ok());

            let git_state = record
                .workspace_path
                .as_ref()
                .and_then(|p| cleanup::compute_git_state(p))
                .and_then(|g| serde_json::to_value(g).ok());

            (
                StatusCode::OK,
                Json(InspectSessionResponse {
                    ok: true,
                    session_id: Some(session_id_str),
                    member_name: Some(record.member_name.clone()),
                    session_type: Some(record.session_type.to_string()),
                    current_state: Some(record.current_state.to_string()),
                    workspace_path: record
                        .workspace_path
                        .as_ref()
                        .map(|p| p.display().to_string()),
                    finalization_results,
                    git_state,
                    error: None,
                }),
            )
        }
        None => {
            let error = format!("Session {session_id_str} not found");
            (
                StatusCode::NOT_FOUND,
                Json(InspectSessionResponse {
                    ok: false,
                    session_id: Some(session_id_str),
                    member_name: None,
                    session_type: None,
                    current_state: None,
                    workspace_path: None,
                    finalization_results: None,
                    git_state: None,
                    error: Some(error),
                }),
            )
        }
    }
}

/// DELETE /api/sessions/:id — cleans up a single retained session.
pub async fn cleanup_session_handler(
    State(state): State<SessionsApiState>,
    Path(session_id_str): Path<String>,
) -> (StatusCode, Json<CleanupSessionResponse>) {
    let mut inner = state.inner.lock().unwrap();
    let session_id = SessionId::from_raw(&session_id_str);
    tracing::info!(session_id = %session_id_str, "Session cleanup requested");

    match cleanup::cleanup_session(&mut inner.registry, &session_id) {
        Ok(report) => {
            tracing::info!(
                session_id = %session_id_str,
                workspace_removed = report.workspace_removed,
                registry_removed = report.registry_removed,
                "Session cleaned up"
            );
            (
                StatusCode::OK,
                Json(CleanupSessionResponse {
                    ok: true,
                    session_id: Some(session_id_str),
                    workspace_removed: report.workspace_removed,
                    registry_removed: report.registry_removed,
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::NOT_FOUND,
            Json(CleanupSessionResponse {
                ok: false,
                session_id: Some(session_id_str),
                workspace_removed: false,
                registry_removed: false,
                error: Some(e.to_string()),
            }),
        ),
    }
}

/// POST /api/sessions/cleanup — bulk cleanup of retained sessions.
pub async fn bulk_cleanup_handler(
    State(state): State<SessionsApiState>,
    Json(req): Json<BulkCleanupRequest>,
) -> (StatusCode, Json<BulkCleanupResponse>) {
    let mut inner = state.inner.lock().unwrap();
    tracing::info!(filter = %req.filter, value = req.value.as_deref().unwrap_or("none"), "Bulk cleanup requested");

    let filter = match req.filter.as_str() {
        "all" => cleanup::CleanupFilter::AllRetained,
        "member" => match req.value {
            Some(name) => cleanup::CleanupFilter::ByMember(name),
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BulkCleanupResponse {
                        ok: false,
                        cleaned: 0,
                        reports: vec![],
                        error: Some("member filter requires a value".to_string()),
                    }),
                )
            }
        },
        "older_than" => match req.value.as_deref() {
            Some(secs_str) => match secs_str.parse::<i64>() {
                Ok(secs) => cleanup::CleanupFilter::OlderThan(chrono::Duration::seconds(secs)),
                Err(_) => {
                    return (
                        StatusCode::BAD_REQUEST,
                        Json(BulkCleanupResponse {
                            ok: false,
                            cleaned: 0,
                            reports: vec![],
                            error: Some(format!("older_than value must be an integer number of seconds, got: {secs_str}")),
                        }),
                    )
                }
            },
            None => {
                return (
                    StatusCode::BAD_REQUEST,
                    Json(BulkCleanupResponse {
                        ok: false,
                        cleaned: 0,
                        reports: vec![],
                        error: Some("older_than filter requires a value (seconds)".to_string()),
                    }),
                )
            }
        },
        other => {
            return (
                StatusCode::BAD_REQUEST,
                Json(BulkCleanupResponse {
                    ok: false,
                    cleaned: 0,
                    reports: vec![],
                    error: Some(format!("unknown filter: {other}")),
                }),
            )
        }
    };

    match cleanup::bulk_cleanup(&mut inner.registry, filter) {
        Ok(reports) => {
            let cleaned = reports.len() as u32;
            tracing::info!(cleaned = cleaned, "Bulk cleanup complete");
            let report_infos = reports
                .into_iter()
                .map(|r| CleanupReportInfo {
                    session_id: r.session_id.to_string(),
                    workspace_removed: r.workspace_removed,
                    registry_removed: r.registry_removed,
                })
                .collect();

            (
                StatusCode::OK,
                Json(BulkCleanupResponse {
                    ok: true,
                    cleaned,
                    reports: report_infos,
                    error: None,
                }),
            )
        }
        Err(e) => (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(BulkCleanupResponse {
                ok: false,
                cleaned: 0,
                reports: vec![],
                error: Some(e.to_string()),
            }),
        ),
    }
}

/// POST /api/sessions/{id}/locks — acquire a work-item lock for this session.
pub async fn acquire_lock_handler(
    State(state): State<SessionsApiState>,
    Path(session_id): Path<String>,
    Json(body): Json<AcquireLockRequest>,
) -> (StatusCode, Json<AcquireLockResponse>) {
    let id = SessionId::from_raw(&session_id);
    let inner = state.inner.lock().unwrap();
    match inner.work_item_lock.acquire(&body.work_item_id, &id) {
        Ok(()) => {
            tracing::debug!(session=%session_id, work_item=%body.work_item_id, "work-item lock acquired");
            (StatusCode::OK, Json(AcquireLockResponse { acquired: true, holder: None }))
        }
        Err(_) => {
            let holder = inner.work_item_lock.holder_of(&body.work_item_id)
                .map(|h| h.to_string());
            tracing::debug!(session=%session_id, work_item=%body.work_item_id, holder=?holder, "work-item lock contended");
            (StatusCode::OK, Json(AcquireLockResponse { acquired: false, holder }))
        }
    }
}

/// DELETE /api/sessions/{id}/locks/{work_item_id} — release a work-item lock.
pub async fn release_lock_handler(
    State(state): State<SessionsApiState>,
    Path((session_id, work_item_id)): Path<(String, String)>,
) -> (StatusCode, Json<ReleaseLockResponse>) {
    let id = SessionId::from_raw(&session_id);
    let inner = state.inner.lock().unwrap();
    inner.work_item_lock.release(&work_item_id, &id);
    tracing::debug!(session=%session_id, work_item=%work_item_id, "work-item lock released");
    (StatusCode::OK, Json(ReleaseLockResponse { released: true }))
}

/// Build the sessions API router fragment (merged into the daemon router in green phase).
pub fn sessions_router(state: SessionsApiState) -> Router {
    Router::new()
        .route("/api/sessions/start", post(start_session_handler))
        .route(
            "/api/sessions/history",
            get(list_session_history_handler),
        )
        .route(
            "/api/sessions/cleanup",
            post(bulk_cleanup_handler),
        )
        .route("/api/sessions/stop", post(stop_bulk_handler))
        .route("/api/sessions", get(list_sessions_handler))
        .route(
            "/api/sessions/{id}/inspect",
            get(inspect_session_handler),
        )
        .route(
            "/api/sessions/{id}/finalize",
            post(retrigger_finalization_handler),
        )
        .route("/api/sessions/{id}/stop", post(stop_session_handler))
        .route("/api/sessions/{id}/locks", post(acquire_lock_handler))
        .route(
            "/api/sessions/{id}/locks/{work_item_id}",
            delete(release_lock_handler),
        )
        .route("/api/sessions/{id}", get(session_detail_handler).delete(cleanup_session_handler))
        .with_state(state)
}

// ── Credential Refresh Loop ──────────────────────────────────────────────────

/// Refreshes GitHub App credentials for a single team member.
/// Abstracted for test injection — production impl delegates to [`CredentialRelay`].
pub(crate) trait CredentialRefreshable: Send + Sync {
    fn ensure_credentials(&self, member_name: &str) -> anyhow::Result<()>;
}

impl CredentialRefreshable for SessionsApiState {
    fn ensure_credentials(&self, member_name: &str) -> anyhow::Result<()> {
        match &self.workspace_ops {
            Some(ops) => ops.ensure_credentials(member_name),
            None => Ok(()),
        }
    }
}

impl SessionsApiState {
    /// Return unique member names that have at least one session in the Active state.
    pub(crate) fn active_member_names(&self) -> Vec<String> {
        let inner = self.inner.lock().unwrap();
        let mut seen = std::collections::HashSet::new();
        inner
            .registry
            .list()
            .into_iter()
            .filter(|r| r.current_state == SessionState::Active)
            .filter_map(|r| {
                if seen.insert(r.member_name.clone()) {
                    Some(r.member_name.clone())
                } else {
                    None
                }
            })
            .collect()
    }

    /// Refresh credentials for all members with Active sessions.
    ///
    /// Non-fatal: each member is attempted regardless of whether earlier members fail.
    /// Returns `(member_name, error_message)` pairs for any member that fails.
    pub(crate) fn refresh_active_session_credentials(
        &self,
        refresher: &dyn CredentialRefreshable,
    ) -> Vec<(String, String)> {
        self.active_member_names()
            .into_iter()
            .filter_map(|member| {
                refresher
                    .ensure_credentials(&member)
                    .err()
                    .map(|e| (member, e.to_string()))
            })
            .collect()
    }
}

/// Background loop: refreshes credentials for active-session members every `interval`.
/// Stops when `shutdown` is set to `true`.
pub(crate) async fn run_credential_refresh_loop(
    sessions_state: SessionsApiState,
    refresher: std::sync::Arc<dyn CredentialRefreshable>,
    interval: std::time::Duration,
    shutdown: std::sync::Arc<std::sync::atomic::AtomicBool>,
) {
    loop {
        sessions_state.refresh_active_session_credentials(refresher.as_ref());
        if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
        tokio::time::sleep(interval).await;
        if shutdown.load(std::sync::atomic::Ordering::SeqCst) {
            break;
        }
    }
}

// ── Console view ────────────────────────────────────────────────────────

impl SessionsApiState {
    /// Returns all session records for the web console operator view.
    pub fn list_for_console(&self) -> Vec<ConsoleSessionSummary> {
        use crate::session::types::FinalizationExitStatus;
        let inner = self.inner.lock().unwrap();
        inner
            .registry
            .list()
            .into_iter()
            .map(|r| {
                let finalization_status = match r.finalization_result.as_ref().map(|f| &f.exit_status) {
                    Some(
                        FinalizationExitStatus::Completed | FinalizationExitStatus::CompletedDegraded,
                    ) => "completed",
                    Some(FinalizationExitStatus::Failed) => "failed",
                    Some(FinalizationExitStatus::Skipped) => "skipped",
                    None => "n/a",
                };
                ConsoleSessionSummary {
                    session_id: r.session_id.to_string(),
                    member_name: r.member_name.clone(),
                    state: r.current_state.to_string(),
                    session_type: r.session_type.to_string(),
                    created_at: r.created_at.to_rfc3339(),
                    finalization_status: finalization_status.to_string(),
                }
            })
            .collect()
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use axum::http::Request;
    use tower::ServiceExt;

    fn test_state() -> (SessionsApiState, tempfile::TempDir) {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        {
            let mut inner = state.inner.lock().unwrap();
            let session_id = SessionId::from_raw("test-session-id");
            let now = chrono::Utc::now();
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "test-member".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: Some(PathBuf::from("/tmp/ws")),
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
        }
        (state, tmp)
    }

    fn test_router() -> (Router, tempfile::TempDir) {
        let (state, tmp) = test_state();
        (sessions_router(state), tmp)
    }

    // AC-1: Session Status Display — list endpoint returns session metadata

    #[tokio::test]
    async fn post_sessions_start_creates_session_and_returns_id() {
        let (app, _tmp) = test_router();
        let body = serde_json::json!({
            "member_name": "alice",
            "session_type": "Interactive"
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/start")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(
            json["session_id"].is_string(),
            "response must include a session_id string"
        );
    }

    #[tokio::test]
    async fn get_sessions_lists_active_sessions() {
        let (app, _tmp) = test_router();
        let request = Request::builder()
            .uri("/api/sessions")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(
            json["sessions"].is_array(),
            "response must include a sessions array"
        );
    }

    #[tokio::test]
    async fn post_sessions_stop_deactivates_session() {
        let (app, _tmp) = test_router();
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/test-session-id/stop")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert_eq!(json["ok"], true);
    }

    #[tokio::test]
    async fn get_session_detail_returns_full_info() {
        let (app, _tmp) = test_router();
        let request = Request::builder()
            .uri("/api/sessions/test-session-id")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(response.status(), StatusCode::OK);
        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: serde_json::Value = serde_json::from_slice(&body_bytes).unwrap();
        assert!(
            json["session"].is_object(),
            "response must include a session object"
        );
    }

    #[tokio::test]
    async fn history_handler_returns_terminal_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();

            // Active session — should NOT appear in history
            let active_id = SessionId::from_raw("active-session");
            let active = SessionRecord {
                session_id: active_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(active).unwrap();
            inner
                .registry
                .update_state(&active_id, SessionState::Active)
                .unwrap();

            // Completed session — SHOULD appear with exit_normal: true
            let completed_id = SessionId::from_raw("completed-session");
            let completed = SessionRecord {
                session_id: completed_id.clone(),
                member_name: "bob".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(completed).unwrap();
            inner
                .registry
                .update_state(&completed_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&completed_id, SessionState::Completed)
                .unwrap();

            // Failed session — SHOULD appear with exit_normal: false
            let failed_id = SessionId::from_raw("failed-session");
            let failed = SessionRecord {
                session_id: failed_id.clone(),
                member_name: "carol".to_string(),
                session_type: SessionType::Brain,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(failed).unwrap();
            inner
                .registry
                .update_state(&failed_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&failed_id, SessionState::Failed)
                .unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/history")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: SessionHistoryResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert_eq!(
            json.sessions.len(),
            2,
            "should return only terminal sessions (Completed + Failed), not Active"
        );

        let completed = json
            .sessions
            .iter()
            .find(|s| s.session_id == "completed-session");
        assert!(
            completed.is_some(),
            "Completed session must be in history"
        );
        assert!(
            completed.unwrap().exit_normal,
            "Completed session must have exit_normal: true"
        );

        let failed = json
            .sessions
            .iter()
            .find(|s| s.session_id == "failed-session");
        assert!(failed.is_some(), "Failed session must be in history");
        assert!(
            !failed.unwrap().exit_normal,
            "Failed session must have exit_normal: false"
        );
    }

    #[tokio::test]
    async fn get_session_detail_not_found() {
        let (app, _tmp) = test_router();
        let request = Request::builder()
            .uri("/api/sessions/nonexistent-id")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();

        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "unknown session ID must return 404"
        );
    }

    // --- CT-89-06: AC-18 Fix — Inspect Endpoint ---

    #[tokio::test]
    async fn inspect_handler_returns_session_metadata() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("inspect-test-session");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: Some(PathBuf::from("/tmp/ws")),
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Completed)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Retained)
                .unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/inspect-test-session/inspect")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: InspectSessionResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok, "inspect response must be ok");
        assert_eq!(
            json.member_name,
            Some("alice".to_string()),
            "inspect response must include member_name from the session record"
        );
        assert_eq!(
            json.current_state,
            Some("Retained".to_string()),
            "inspect response must include current_state from the session record"
        );
    }

    #[tokio::test]
    async fn inspect_handler_returns_finalization_when_present() {
        use crate::session::types::{
            CommittedRepo, FinalizationExitStatus,
            FinalizationResult as TypesFinalizationResult,
        };

        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("finalization-session");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "bob".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: Some(TypesFinalizationResult {
                    exit_status: FinalizationExitStatus::CompletedDegraded,
                    committed_repos: vec![CommittedRepo {
                        repo_name: "myproject".to_string(),
                        branch: "main".to_string(),
                    }],
                    pushed_branches: vec!["main".to_string()],
                    recovery_branches: vec!["recovery/abc/main".to_string()],
                    github_issue_urls: vec![],
                }),
                finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Completed)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Retained)
                .unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/finalization-session/inspect")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: InspectSessionResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(
            json.finalization_results.is_some(),
            "inspect must include finalization_results when the session has finalization data"
        );
        let fin = json.finalization_results.unwrap();
        assert_eq!(
            fin["exit_status"], "CompletedDegraded",
            "finalization exit_status must match the record"
        );
        assert!(
            fin["committed_repos"].is_array(),
            "finalization committed_repos must be an array"
        );
    }

    // --- CT-89-06: Cleanup Endpoint ---

    #[tokio::test]
    async fn cleanup_handler_removes_session() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("cleanup-target");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Completed)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Retained)
                .unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .method("DELETE")
            .uri("/api/sessions/cleanup-target")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: CleanupSessionResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok, "cleanup response must be ok");
        assert!(
            json.registry_removed,
            "cleanup must report registry_removed: true after removing the session"
        );
    }

    #[tokio::test]
    async fn bulk_cleanup_handler_cleans_retained_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();

            for name in &["s1", "s2"] {
                let session_id = SessionId::from_raw(*name);
                let record = SessionRecord {
                    session_id: session_id.clone(),
                    member_name: "alice".to_string(),
                    session_type: SessionType::Loop,
                    current_state: SessionState::Creating,
                    created_at: now,
                    state_transitioned_at: now,
                    agent_pid: None,
                    workspace_path: None,
                    finalization_result: None,
            finalization_agent_pid: None,
                };
                inner.registry.register(record).unwrap();
                inner
                    .registry
                    .update_state(&session_id, SessionState::Active)
                    .unwrap();
                inner
                    .registry
                    .update_state(&session_id, SessionState::Completed)
                    .unwrap();
                inner
                    .registry
                    .update_state(&session_id, SessionState::Retained)
                    .unwrap();
            }
        }

        let app = sessions_router(state);
        let body = serde_json::json!({
            "filter": "all",
            "value": null
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/cleanup")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: BulkCleanupResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok, "bulk cleanup response must be ok");
        assert_eq!(
            json.cleaned, 2,
            "bulk cleanup must report 2 sessions cleaned"
        );
        assert_eq!(
            json.reports.len(),
            2,
            "bulk cleanup must include 2 per-session reports"
        );
    }

    // --- CT-88-03: Graceful stop transitions to Finalizing ---

    #[tokio::test]
    async fn graceful_stop_transitions_to_finalizing() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("stop-finalize-test");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
        }

        let app = sessions_router(state.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/stop-finalize-test/stop")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let inner = state.inner.lock().unwrap();
        let session_id = SessionId::from_raw("stop-finalize-test");
        let record = inner.registry.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Finalizing,
            "graceful stop must transition to Finalizing (awaiting finalization)"
        );
    }

    // --- CT-88-03: Force stop transitions to Killed ---

    #[tokio::test]
    async fn force_stop_transitions_to_killed() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("force-stop-test");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
        }

        let app = sessions_router(state.clone());
        let body = serde_json::json!({ "force": true });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/force-stop-test/stop")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let inner = state.inner.lock().unwrap();
        let session_id = SessionId::from_raw("force-stop-test");
        let record = inner.registry.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Killed,
            "force stop must transition to Killed"
        );
    }

    // --- CT-154-01-fix: stop_session_handler must spawn deactivation watcher ---

    #[tokio::test]
    async fn stop_session_handler_spawns_deactivation_watcher() {
        // Session with no workspace and no agent_pid: the watcher (when spawned) transitions
        // immediately to Completed. This exercises the missing watcher call in stop_session_handler.
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("watcher-test-session");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
                finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner.registry.update_state(&session_id, SessionState::Active).unwrap();
        }

        let app = sessions_router(state.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/watcher-test-session/stop")
            .body(Body::empty())
            .unwrap();
        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        // Give the deactivation watcher time to run and transition Finalizing → Completed.
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;

        // The watcher (no pid, no workspace) must drive the session to Completed.
        // Bug: stop_session_handler does not spawn the watcher, so session stays Finalizing.
        let inner = state.inner.lock().unwrap();
        let session_id = SessionId::from_raw("watcher-test-session");
        let record = inner.registry.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Completed,
            "stop_session_handler must spawn a deactivation watcher — session must transition from Finalizing to Completed, not stay stuck in Finalizing"
        );
    }

    // --- CT-88-03: Bulk stop by member ---

    #[tokio::test]
    async fn bulk_stop_member_deactivates_all_member_sessions() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();

            for name in &["member-s1", "member-s2"] {
                let session_id = SessionId::from_raw(*name);
                let record = SessionRecord {
                    session_id: session_id.clone(),
                    member_name: "alice".to_string(),
                    session_type: SessionType::Loop,
                    current_state: SessionState::Creating,
                    created_at: now,
                    state_transitioned_at: now,
                    agent_pid: None,
                    workspace_path: None,
                    finalization_result: None,
            finalization_agent_pid: None,
                };
                inner.registry.register(record).unwrap();
                inner
                    .registry
                    .update_state(&session_id, SessionState::Active)
                    .unwrap();
            }
        }

        let app = sessions_router(state.clone());
        let body = serde_json::json!({
            "mode": "member",
            "member": "alice"
        });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/stop")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: StopBulkResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok);
        assert_eq!(
            json.deactivated, 2,
            "bulk member stop must deactivate all member sessions"
        );
    }

    // --- CT-88-03: Bulk autonomous stop skips interactive ---

    #[tokio::test]
    async fn bulk_autonomous_stop_skips_interactive() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();

            let loop_id = SessionId::from_raw("auto-loop");
            let loop_record = SessionRecord {
                session_id: loop_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(loop_record).unwrap();
            inner
                .registry
                .update_state(&loop_id, SessionState::Active)
                .unwrap();

            let interactive_id = SessionId::from_raw("auto-interactive");
            let interactive_record = SessionRecord {
                session_id: interactive_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(interactive_record).unwrap();
            inner
                .registry
                .update_state(&interactive_id, SessionState::Active)
                .unwrap();
        }

        let app = sessions_router(state.clone());
        let body = serde_json::json!({ "mode": "autonomous" });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/stop")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: StopBulkResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok);
        assert_eq!(json.deactivated, 1, "only the loop session should be deactivated");
        assert_eq!(
            json.skipped_interactive, 1,
            "interactive session must be reported as skipped"
        );

        let inner = state.inner.lock().unwrap();
        let interactive_id = SessionId::from_raw("auto-interactive");
        let record = inner.registry.get(&interactive_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Active,
            "interactive session must remain Active"
        );
    }

    // --- CT-88-03: Retrigger finalization endpoint ---

    #[tokio::test]
    async fn retrigger_finalization_transitions_retained_to_finalizing() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("retrigger-test");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Killed)
                .unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Retained)
                .unwrap();
        }

        let app = sessions_router(state.clone());
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/retrigger-test/finalize")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let body_bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024)
            .await
            .unwrap();
        let json: RetriggerResponse = serde_json::from_slice(&body_bytes).unwrap();

        assert!(json.ok, "retrigger must succeed on Retained session");

        let inner = state.inner.lock().unwrap();
        let session_id = SessionId::from_raw("retrigger-test");
        let record = inner.registry.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Finalizing,
            "retrigger must transition Retained → Finalizing"
        );
    }

    // --- CT-87-03: Cleanup rejects Active sessions ---

    #[tokio::test]
    async fn cleanup_handler_rejects_active_session() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("active-cleanup-test");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Interactive,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
            finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner
                .registry
                .update_state(&session_id, SessionState::Active)
                .unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .method("DELETE")
            .uri("/api/sessions/active-cleanup-test")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::NOT_FOUND,
            "cleanup of an Active session must be rejected"
        );
    }

    // --- CT-89-01 QE re-entry: AC-10 fix ---

    #[test]
    fn record_to_info_includes_state_transitioned_at() {
        let now = chrono::Utc::now();
        let record = SessionRecord {
            session_id: SessionId::from_raw("abc123"),
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Active,
            created_at: now - chrono::Duration::hours(2),
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        let info = record_to_info(&record);
        assert!(
            info.state_transitioned_at.is_some(),
            "SessionInfo must include state_transitioned_at from SessionRecord"
        );
    }

    // --- CT-154-04: bm-agent lock acquire/release — daemon endpoints ---

    fn make_active_session(state: &SessionsApiState, session_id: &str) {
        let mut inner = state.inner.lock().unwrap();
        let now = chrono::Utc::now();
        let id = SessionId::from_raw(session_id);
        let record = SessionRecord {
            session_id: id.clone(),
            member_name: "test-member".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        inner.registry.register(record).unwrap();
        inner.registry.update_state(&id, SessionState::Active).unwrap();
    }

    #[tokio::test]
    async fn lock_acquire_unclaimed_returns_acquired_true() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_active_session(&state, "sess-acq-1");

        let app = sessions_router(state);
        let body = serde_json::json!({ "work_item_id": "ISSUE-42" });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/sess-acq-1/locks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "POST /locks must return 200"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: AcquireLockResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(json.acquired, "unclaimed lock must return acquired: true");
        assert!(json.holder.is_none(), "no holder when acquired");
    }

    #[tokio::test]
    async fn lock_acquire_contended_returns_acquired_false_with_holder() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_active_session(&state, "sess-holder");
        make_active_session(&state, "sess-requester");

        // Pre-acquire lock with the holder session
        {
            let inner = state.inner.lock().unwrap();
            inner.work_item_lock.acquire("ISSUE-42", &SessionId::from_raw("sess-holder")).unwrap();
        }

        let app = sessions_router(state);
        let body = serde_json::json!({ "work_item_id": "ISSUE-42" });
        let request = Request::builder()
            .method("POST")
            .uri("/api/sessions/sess-requester/locks")
            .header("content-type", "application/json")
            .body(Body::from(serde_json::to_string(&body).unwrap()))
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "contention must return HTTP 200 (not 4xx)"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: AcquireLockResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(!json.acquired, "contended lock must return acquired: false");
        assert_eq!(
            json.holder.as_deref(),
            Some("sess-holder"),
            "holder field must identify the current lock owner"
        );
    }

    #[tokio::test]
    async fn lock_release_held_returns_released_true() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_active_session(&state, "sess-rel-1");

        {
            let inner = state.inner.lock().unwrap();
            inner.work_item_lock.acquire("ISSUE-42", &SessionId::from_raw("sess-rel-1")).unwrap();
        }

        let app = sessions_router(state);
        let request = Request::builder()
            .method("DELETE")
            .uri("/api/sessions/sess-rel-1/locks/ISSUE-42")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "DELETE /locks/:work_item_id must return 200"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: ReleaseLockResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(json.released, "releasing a held lock must return released: true");
    }

    #[tokio::test]
    async fn lock_release_unheld_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_active_session(&state, "sess-rel-2");

        let app = sessions_router(state);
        let request = Request::builder()
            .method("DELETE")
            .uri("/api/sessions/sess-rel-2/locks/ISSUE-99")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(
            response.status(),
            StatusCode::OK,
            "releasing an unheld lock must return 200 (idempotent)"
        );
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: ReleaseLockResponse = serde_json::from_slice(&bytes).unwrap();
        assert!(json.released, "idempotent release must return released: true");
    }

    // ── CT-154-05: bm session list — finalization_status field ───────────

    fn make_terminal_session_with_finalization(
        state: &SessionsApiState,
        session_id: &str,
        final_state: SessionState,
        fin_result: Option<crate::session::types::FinalizationResult>,
    ) {
        let mut inner = state.inner.lock().unwrap();
        let now = chrono::Utc::now();
        let id = SessionId::from_raw(session_id);
        let record = SessionRecord {
            session_id: id.clone(),
            member_name: "test-member".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: None,
            finalization_result: fin_result,
            finalization_agent_pid: None,
        };
        inner.registry.register(record).unwrap();
        inner.registry.update_state(&id, SessionState::Active).unwrap();
        inner.registry.update_state(&id, SessionState::Finalizing).unwrap();
        inner.registry.update_state(&id, final_state).unwrap();
    }

    fn make_finalization_result(
        exit_status: crate::session::types::FinalizationExitStatus,
    ) -> crate::session::types::FinalizationResult {
        crate::session::types::FinalizationResult {
            exit_status,
            committed_repos: vec![],
            pushed_branches: vec![],
            recovery_branches: vec![],
            github_issue_urls: vec![],
        }
    }

    #[tokio::test]
    async fn history_finalization_status_completed_when_exit_status_is_completed() {
        use crate::session::types::FinalizationExitStatus;
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_terminal_session_with_finalization(
            &state,
            "sess-fin-completed",
            SessionState::Completed,
            Some(make_finalization_result(FinalizationExitStatus::Completed)),
        );

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/history")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: SessionHistoryResponse = serde_json::from_slice(&bytes).unwrap();

        let session = json.sessions.iter().find(|s| s.session_id == "sess-fin-completed")
            .expect("session must appear in history");
        assert_eq!(
            session.finalization_status, "completed",
            "Completed finalization exit status must map to 'completed', got '{}'",
            session.finalization_status
        );
    }

    #[tokio::test]
    async fn history_finalization_status_failed_when_exit_status_is_failed() {
        use crate::session::types::FinalizationExitStatus;
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_terminal_session_with_finalization(
            &state,
            "sess-fin-failed",
            SessionState::Failed,
            Some(make_finalization_result(FinalizationExitStatus::Failed)),
        );

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/history")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: SessionHistoryResponse = serde_json::from_slice(&bytes).unwrap();

        let session = json.sessions.iter().find(|s| s.session_id == "sess-fin-failed")
            .expect("session must appear in history");
        assert_eq!(
            session.finalization_status, "failed",
            "Failed finalization exit status must map to 'failed', got '{}'",
            session.finalization_status
        );
    }

    #[tokio::test]
    async fn history_finalization_status_skipped_when_exit_status_is_skipped() {
        use crate::session::types::FinalizationExitStatus;
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_terminal_session_with_finalization(
            &state,
            "sess-fin-skipped",
            SessionState::Completed,
            Some(make_finalization_result(FinalizationExitStatus::Skipped)),
        );

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/history")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: SessionHistoryResponse = serde_json::from_slice(&bytes).unwrap();

        let session = json.sessions.iter().find(|s| s.session_id == "sess-fin-skipped")
            .expect("session must appear in history");
        assert_eq!(
            session.finalization_status, "skipped",
            "Skipped finalization exit status must map to 'skipped', got '{}'",
            session.finalization_status
        );
    }

    #[tokio::test]
    async fn history_finalization_status_na_when_no_finalization_result() {
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_terminal_session_with_finalization(
            &state,
            "sess-fin-na",
            SessionState::Completed,
            None, // no finalization_result — daemon was killed before finalization ran
        );

        let app = sessions_router(state);
        let request = Request::builder()
            .uri("/api/sessions/history")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);
        let bytes = axum::body::to_bytes(response.into_body(), 1024 * 1024).await.unwrap();
        let json: SessionHistoryResponse = serde_json::from_slice(&bytes).unwrap();

        let session = json.sessions.iter().find(|s| s.session_id == "sess-fin-na")
            .expect("session must appear in history");
        assert_eq!(
            session.finalization_status, "n/a",
            "Session with no finalization_result must have 'n/a' status, got '{}'",
            session.finalization_status
        );
    }


    fn make_active_session_for_member(state: &SessionsApiState, session_id: &str, member: &str) {
        let mut inner = state.inner.lock().unwrap();
        let now = chrono::Utc::now();
        let id = SessionId::from_raw(session_id);
        let record = SessionRecord {
            session_id: id.clone(),
            member_name: member.to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        inner.registry.register(record).unwrap();
        inner.registry.update_state(&id, SessionState::Active).unwrap();
    }

    #[test]
    fn credential_refresh_covers_all_active_members() {
        use std::sync::Mutex;

        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        make_active_session_for_member(&state, "sess-cr-a", "alice");
        make_active_session_for_member(&state, "sess-cr-b", "bob");

        // carol: create as Active then transition to Completed (terminal)
        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let carol_id = SessionId::from_raw("sess-cr-c");
            let record = SessionRecord {
                session_id: carol_id.clone(),
                member_name: "carol".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: None,
                workspace_path: None,
                finalization_result: None,
                finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner.registry.update_state(&carol_id, SessionState::Active).unwrap();
            inner.registry.update_state(&carol_id, SessionState::Completed).unwrap();
        }

        let refreshed: Arc<Mutex<Vec<String>>> = Arc::new(Mutex::new(vec![]));
        struct TrackingRefresher {
            log: Arc<Mutex<Vec<String>>>,
        }
        impl CredentialRefreshable for TrackingRefresher {
            fn ensure_credentials(&self, member_name: &str) -> anyhow::Result<()> {
                self.log.lock().unwrap().push(member_name.to_string());
                Ok(())
            }
        }

        state.refresh_active_session_credentials(&TrackingRefresher { log: refreshed.clone() });

        let called = refreshed.lock().unwrap();
        assert!(
            called.contains(&"alice".to_string()),
            "alice must be refreshed (Active session), got: {:?}",
            *called
        );
        assert!(
            called.contains(&"bob".to_string()),
            "bob must be refreshed (Active session), got: {:?}",
            *called
        );
        assert!(
            !called.contains(&"carol".to_string()),
            "carol must not be refreshed (Completed session)"
        );
    }

    #[test]
    fn credential_refresh_failure_is_non_fatal() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        make_active_session_for_member(&state, "sess-nf-a", "alice");
        make_active_session_for_member(&state, "sess-nf-b", "bob");

        let bob_refreshed = Arc::new(AtomicBool::new(false));
        struct SelectiveRefresher {
            bob_flag: Arc<AtomicBool>,
        }
        impl CredentialRefreshable for SelectiveRefresher {
            fn ensure_credentials(&self, member_name: &str) -> anyhow::Result<()> {
                if member_name == "alice" {
                    anyhow::bail!("simulated alice credential failure");
                }
                self.bob_flag.store(true, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }

        let failures = state.refresh_active_session_credentials(&SelectiveRefresher {
            bob_flag: bob_refreshed.clone(),
        });

        assert!(
            bob_refreshed.load(Ordering::SeqCst),
            "bob must still be refreshed even when alice fails"
        );
        assert_eq!(
            failures.len(),
            1,
            "exactly one failure (alice) must be reported, got: {:?}",
            failures
        );
        assert_eq!(
            failures[0].0, "alice",
            "alice must be identified as the failing member"
        );
    }

    #[tokio::test]
    async fn credential_refresh_loop_stops_on_shutdown() {
        use std::sync::atomic::{AtomicUsize, Ordering};
        use std::time::Duration;

        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));
        make_active_session_for_member(&state, "sess-sd-a", "alice");

        let call_count = Arc::new(AtomicUsize::new(0));
        struct CountingRefresher {
            count: Arc<AtomicUsize>,
        }
        impl CredentialRefreshable for CountingRefresher {
            fn ensure_credentials(&self, _member_name: &str) -> anyhow::Result<()> {
                self.count.fetch_add(1, std::sync::atomic::Ordering::SeqCst);
                Ok(())
            }
        }

        let shutdown = Arc::new(std::sync::atomic::AtomicBool::new(false));
        let shutdown_clone = shutdown.clone();
        let count_clone = call_count.clone();

        // Signal shutdown after a short delay so the loop gets at least one pass
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            shutdown_clone.store(true, std::sync::atomic::Ordering::SeqCst);
        });

        tokio::time::timeout(
            Duration::from_millis(500),
            run_credential_refresh_loop(
                state,
                Arc::new(CountingRefresher { count: count_clone }),
                Duration::from_millis(1),
                shutdown,
            ),
        )
        .await
        .expect("credential refresh loop must exit within 500ms when shutdown is signaled");

        assert!(
            call_count.load(Ordering::SeqCst) >= 1,
            "refresh loop must execute at least one pass before shutdown"
        );
    }

    // --- CT-154-15: BridgeContext resolves user IDs from bridge state ---

    #[test]
    fn bridge_context_member_user_id_reads_from_state() {
        use crate::bridge::{BridgeIdentity, BridgeState, LocalCredentialStore};

        let tmp = tempfile::tempdir().unwrap();
        let bstate_path = tmp.path().join("bridge-state.json");

        let mut state = BridgeState::default();
        state.identities.insert(
            "alice".to_string(),
            BridgeIdentity {
                username: "alice".to_string(),
                user_id: "@alice:matrix.example.com".to_string(),
                token: None,
                created_at: "2026-01-01T00:00:00Z".to_string(),
                is_operator: false,
            },
        );
        bridge::save_state(&bstate_path, &state).unwrap();

        let bc = BridgeContext {
            bridge_type_name: "tuwunel".to_string(),
            bstate_path: bstate_path.clone(),
            credential_store: LocalCredentialStore::new("team", "bridge", bstate_path),
        };

        assert_eq!(
            bc.member_user_id("alice"),
            Some("@alice:matrix.example.com".to_string())
        );
        assert_eq!(bc.member_user_id("nonexistent"), None);
    }

    #[test]
    fn bridge_context_admin_user_id_reads_from_state() {
        use crate::bridge::{BridgeState, LocalCredentialStore};

        let tmp = tempfile::tempdir().unwrap();
        let bstate_path = tmp.path().join("bridge-state.json");

        let mut state = BridgeState::default();
        state.admin_user_id = Some("@admin:matrix.example.com".to_string());
        bridge::save_state(&bstate_path, &state).unwrap();

        let bc = BridgeContext {
            bridge_type_name: "tuwunel".to_string(),
            bstate_path: bstate_path.clone(),
            credential_store: LocalCredentialStore::new("team", "bridge", bstate_path),
        };

        assert_eq!(
            bc.admin_user_id(),
            Some("@admin:matrix.example.com".to_string())
        );
    }

    #[test]
    fn bridge_context_user_id_missing_state_returns_none() {
        use crate::bridge::LocalCredentialStore;

        let tmp = tempfile::tempdir().unwrap();
        let bstate_path = tmp.path().join("nonexistent-bridge-state.json");

        let bc = BridgeContext {
            bridge_type_name: "tuwunel".to_string(),
            bstate_path: bstate_path.clone(),
            credential_store: LocalCredentialStore::new("team", "bridge", bstate_path),
        };

        // Missing state file: load_state returns default (empty) state → None
        assert_eq!(bc.member_user_id("alice"), None);
        assert_eq!(bc.admin_user_id(), None);
    }

    // --- CT-154-16: Finalizing sessions with dead PIDs must not be crash-detected ---

    #[tokio::test]
    async fn finalizing_session_with_dead_pid_stays_finalizing() {
        // A Finalizing session has an intentionally dead PID (SIGTERM was sent during graceful
        // stop). The spawn_deactivation_watcher owns the Finalizing → Completed/Failed
        // transition. Crash detection in list_sessions_handler must NOT race by moving
        // Finalizing → Failed when the agent PID is dead.
        let tmp = tempfile::tempdir().unwrap();
        let state = SessionsApiState::new(tmp.path().join("registry.json"));

        {
            let mut inner = state.inner.lock().unwrap();
            let now = chrono::Utc::now();
            let session_id = SessionId::from_raw("finalizing-dead-pid");
            let record = SessionRecord {
                session_id: session_id.clone(),
                member_name: "alice".to_string(),
                session_type: SessionType::Loop,
                current_state: SessionState::Creating,
                created_at: now,
                state_transitioned_at: now,
                agent_pid: Some(u32::MAX), // guaranteed dead PID
                workspace_path: None,
                finalization_result: None,
                finalization_agent_pid: None,
            };
            inner.registry.register(record).unwrap();
            inner.registry.update_state(&session_id, SessionState::Active).unwrap();
            inner.registry.update_state(&session_id, SessionState::Finalizing).unwrap();
        }

        let app = sessions_router(state.clone());
        let request = Request::builder()
            .uri("/api/sessions")
            .body(Body::empty())
            .unwrap();

        let response = app.oneshot(request).await.unwrap();
        assert_eq!(response.status(), StatusCode::OK);

        let inner = state.inner.lock().unwrap();
        let session_id = SessionId::from_raw("finalizing-dead-pid");
        let record = inner.registry.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Finalizing,
            "Finalizing session with dead PID must stay Finalizing — \
             spawn_deactivation_watcher owns the Finalizing -> Completed/Failed transition"
        );
    }
}
