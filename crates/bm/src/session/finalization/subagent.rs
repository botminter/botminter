use std::path::Path;
use std::process::Command;

use anyhow::Result;
use libc;

use crate::session::types::{SessionId, SessionState};

/// Production timeout ceiling for finalization subagent watcher (160 seconds).
///
/// The finalization agent makes 2-3 LLM API turns (inspect repos, push branches).
/// With Vertex AI latency up to 36s/turn, 3 turns = 108s + 10s startup = 118s.
/// 160s gives 42s of margin, while still leaving 18s before D22's 180s polling window closes
/// (accounting for 10s SIGKILL grace + 2s inspect/spawn overhead).
pub const FINALIZATION_TIMEOUT_SECS: u64 = 160;

/// Wait for a spawned finalization child to exit, then call `on_state_change` with the result.
///
/// Transitions Finalizing -> Completed on exit 0, Finalizing -> Failed on non-zero exit or timeout.
/// The caller supplies `on_state_change` to update the session registry — this keeps the watcher
/// decoupled from the registry type used at the call site.
pub async fn wait_and_transition<F>(
    mut child: std::process::Child,
    session_id: SessionId,
    timeout: std::time::Duration,
    on_state_change: F,
) where
    F: FnOnce(SessionState) + Send + 'static,
{
    let pid = child.id();
    tracing::info!(
        session_id = session_id.as_str(),
        pid = pid,
        "finalization subagent watcher started"
    );

    let wait_handle = tokio::task::spawn_blocking(move || child.wait());

    let new_state = match tokio::time::timeout(timeout, wait_handle).await {
        Ok(Ok(Ok(status))) if status.success() => {
            tracing::info!(
                session_id = session_id.as_str(),
                "finalization subagent exited 0; transitioning to Completed"
            );
            SessionState::Completed
        }
        Ok(Ok(Ok(status))) => {
            tracing::warn!(
                session_id = session_id.as_str(),
                exit_code = ?status.code(),
                "finalization subagent exited non-zero; transitioning to Failed"
            );
            SessionState::Failed
        }
        Err(_elapsed) => {
            tracing::error!(
                session_id = session_id.as_str(),
                pid = pid,
                "finalization subagent timed out; killing process and transitioning to Failed"
            );
            // SAFETY: sending SIGKILL to a child PID we own
            unsafe {
                libc::kill(pid as libc::pid_t, libc::SIGKILL);
            }
            SessionState::Failed
        }
        Ok(Err(join_err)) => {
            tracing::error!(
                session_id = session_id.as_str(),
                "finalization subagent wait task panicked: {join_err}; transitioning to Failed"
            );
            SessionState::Failed
        }
        Ok(Ok(Err(io_err))) => {
            tracing::error!(
                session_id = session_id.as_str(),
                "finalization subagent wait returned IO error: {io_err}; transitioning to Failed"
            );
            SessionState::Failed
        }
    };

    on_state_change(new_state);
}

pub fn build_finalization_command(
    workspace_path: &Path,
    session_id: &SessionId,
) -> Command {
    let prompt = format!(
        "Finalize session {}: inspect all repos, commit and push relevant work, \
         handle push conflicts with D-10 recovery.",
        session_id.as_str()
    );

    let mut cmd = Command::new("claude");
    cmd.args([
        "--dangerously-skip-permissions",
        "--agent",
        "finalization",
        "-p",
        &prompt,
    ])
    .current_dir(workspace_path)
    .env("BM_SESSION_ID", session_id.as_str())
    .env_remove("CLAUDECODE");
    cmd
}

pub fn retrigger_finalization(
    workspace_path: &Path,
    session_id: &SessionId,
) -> Result<std::process::Child> {
    build_finalization_command(workspace_path, session_id)
        .spawn()
        .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};

    use crate::session::registry::SessionRegistry;
    use crate::session::types::{SessionRecord, SessionState, SessionType};

    fn make_state_updater(
        registry: Arc<Mutex<SessionRegistry>>,
        session_id: SessionId,
    ) -> impl FnOnce(SessionState) + Send + 'static {
        move |state| {
            if let Ok(mut reg) = registry.lock() {
                let _ = reg.update_state(&session_id, state);
            }
        }
    }

    fn finalization_command() -> Command {
        let workspace = PathBuf::from("/tmp/workspace");
        let session_id = SessionId::from_raw("abc12345");
        build_finalization_command(&workspace, &session_id)
    }

    #[test]
    fn launch_finalization_subagent_spawns_claude_binary() {
        let cmd = finalization_command();

        assert_eq!(
            cmd.get_program(),
            OsStr::new("claude"),
            "finalization command must use 'claude' as executable"
        );
    }

    #[test]
    fn launch_finalization_subagent_sets_workspace_as_cwd() {
        let cmd = finalization_command();

        assert_eq!(
            cmd.get_current_dir(),
            Some(Path::new("/tmp/workspace")),
            "finalization command must set current_dir to workspace_path"
        );
    }

    #[test]
    fn launch_finalization_subagent_passes_session_id_in_env() {
        let cmd = finalization_command();

        let bm_session_id = cmd
            .get_envs()
            .find(|(k, _)| *k == OsStr::new("BM_SESSION_ID"))
            .map(|(_, v)| v);

        assert_eq!(
            bm_session_id,
            Some(Some(OsStr::new("abc12345"))),
            "finalization command must set BM_SESSION_ID env var to session ID"
        );
    }

    #[test]
    fn finalization_agent_file_exists_in_agentic_sdlc_planning_profile() {
        let profiles = crate::profile::embedded::embedded_profiles();
        let path = "agentic-sdlc-planning/coding-agent/agents/finalization.md";

        let file = profiles.get_file(path);

        assert!(
            file.is_some(),
            "finalization.md must exist at profiles/{}", path
        );
    }

    #[test]
    fn retrigger_finalization_is_not_a_stub() {
        let workspace = PathBuf::from("/nonexistent/workspace-retrigger-test");
        let session_id = SessionId::from_raw("sess-retrigger-stub-check");

        let result = retrigger_finalization(&workspace, &session_id);

        assert!(
            result.is_err(),
            "retrigger_finalization with non-existent workspace must fail — a stub returning Ok is not a real implementation"
        );
    }

    #[test]
    fn retrigger_finalization_uses_workspace_as_cwd() {
        let workspace = PathBuf::from("/tmp/workspace-retrigger");
        let session_id = SessionId::from_raw("sess-retrigger-cwd");

        let cmd = build_finalization_command(&workspace, &session_id);

        assert_eq!(
            cmd.get_current_dir(),
            Some(workspace.as_path()),
            "finalization command must set current_dir to workspace_path for retrigger"
        );
    }

    // --- CT-154-01: child-process reaper ---

    fn make_finalizing_registry() -> (Arc<Mutex<SessionRegistry>>, SessionId) {
        let tmp = tempfile::tempdir().unwrap();
        let mut registry = SessionRegistry::new(tmp.path().join("registry.json"));
        let session_id = SessionId::from_raw("fin-reaper-sess");
        let now = chrono::Utc::now();
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
        registry.register(record).unwrap();
        registry.update_state(&session_id, SessionState::Active).unwrap();
        registry.update_state(&session_id, SessionState::Finalizing).unwrap();
        (Arc::new(Mutex::new(registry)), session_id)
    }

    #[tokio::test]
    async fn wait_and_transition_exits_zero_sets_completed() {
        let (registry, session_id) = make_finalizing_registry();

        let child = std::process::Command::new("true")
            .spawn()
            .expect("failed to spawn 'true'");

        wait_and_transition(
            child,
            session_id.clone(),
            std::time::Duration::from_secs(10),
            make_state_updater(registry.clone(), session_id.clone()),
        )
        .await;

        let reg = registry.lock().unwrap();
        let record = reg.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Completed,
            "finalization subagent exit 0 must transition Finalizing -> Completed"
        );
    }

    #[tokio::test]
    async fn wait_and_transition_exits_nonzero_sets_failed() {
        let (registry, session_id) = make_finalizing_registry();

        let child = std::process::Command::new("false")
            .spawn()
            .expect("failed to spawn 'false'");

        wait_and_transition(
            child,
            session_id.clone(),
            std::time::Duration::from_secs(10),
            make_state_updater(registry.clone(), session_id.clone()),
        )
        .await;

        let reg = registry.lock().unwrap();
        let record = reg.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Failed,
            "finalization subagent exit non-zero must transition Finalizing -> Failed"
        );
    }

    #[tokio::test]
    async fn wait_and_transition_timeout_transitions_to_failed() {
        let (registry, session_id) = make_finalizing_registry();

        let child = std::process::Command::new("sleep")
            .arg("9999")
            .spawn()
            .expect("failed to spawn 'sleep 9999'");

        // inject a very short timeout to trigger the timeout path without waiting 120s
        wait_and_transition(
            child,
            session_id.clone(),
            std::time::Duration::from_millis(200),
            make_state_updater(registry.clone(), session_id.clone()),
        )
        .await;

        let reg = registry.lock().unwrap();
        let record = reg.get(&session_id).unwrap();
        assert_eq!(
            record.current_state,
            SessionState::Failed,
            "finalization timeout must transition Finalizing -> Failed"
        );
    }

    #[test]
    fn production_finalization_timeout_is_160s() {
        assert_eq!(
            FINALIZATION_TIMEOUT_SECS,
            160,
            "production finalization timeout must be 160s — enough for 3 LLM turns at \
             36s each (Vertex AI worst case) plus startup and git push overhead, \
             while still fitting within D22's 180s polling window"
        );
    }
}
