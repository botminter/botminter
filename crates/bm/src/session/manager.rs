use std::path::{Path, PathBuf};

use anyhow::Result;

use super::dirty_state::RepoDirtyState;
use super::finalization::deactivation::{self, DEFAULT_MAX_RETRIES};
use super::registry::SessionRegistry;
use super::types::{SessionId, SessionRecord, SessionState, SessionType};
use super::work_item_lock::WorkItemLock;

/// Abstracts workspace provisioning and inspection so the session manager
/// can be tested without real git operations.
pub trait WorkspaceOps: Send + Sync {
    fn hydrate_workspace(&self, session_id: &SessionId, member: &str) -> Result<PathBuf>;
    fn inspect_dirty_state(&self, workspace_path: &Path) -> Result<Vec<RepoDirtyState>>;
}

/// Result returned by `deactivate_session` — contains the updated session record
/// and the dirty state snapshot taken after push attempts.
pub struct DeactivationResult {
    pub session_record: SessionRecord,
    pub dirty_state: Vec<RepoDirtyState>,
}

/// Parameters for creating a new session.
#[derive(Debug)]
pub struct CreateSessionParams {
    pub member_name: String,
    pub session_type: SessionType,
    pub work_item_id: Option<String>,
}

/// Coordinates session lifecycle across the registry, workspace hydrator, and work-item lock.
pub struct SessionManager<W: WorkspaceOps> {
    pub(crate) registry: SessionRegistry,
    pub(crate) work_item_lock: WorkItemLock,
    pub(crate) workspace_ops: W,
}

impl<W: WorkspaceOps> SessionManager<W> {
    pub fn new(registry: SessionRegistry, work_item_lock: WorkItemLock, workspace_ops: W) -> Self {
        Self {
            registry,
            work_item_lock,
            workspace_ops,
        }
    }

    /// Create a new session: hydrate workspace, register in registry, transition to Active.
    /// If `work_item_id` is provided, acquire a lock on it.
    pub fn create_session(&mut self, params: CreateSessionParams) -> Result<SessionRecord> {
        let session_id = SessionId::new();

        if let Some(ref work_item_id) = params.work_item_id {
            self.work_item_lock
                .acquire(work_item_id, &session_id)?;
        }

        let workspace_path = match self
            .workspace_ops
            .hydrate_workspace(&session_id, &params.member_name)
        {
            Ok(path) => path,
            Err(e) => {
                if let Some(ref work_item_id) = params.work_item_id {
                    self.work_item_lock.release(work_item_id, &session_id);
                }
                return Err(e);
            }
        };

        let now = chrono::Utc::now();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: params.member_name,
            session_type: params.session_type,
            current_state: SessionState::Creating,
            created_at: now,
            state_transitioned_at: now,
            agent_pid: None,
            workspace_path: Some(workspace_path),
            finalization_result: None,
            finalization_agent_pid: None,
        };

        self.registry.register(record)?;
        self.registry
            .update_state(&session_id, SessionState::Active)?;

        Ok(self.registry.get(&session_id).unwrap().clone())
    }

    /// Return sessions in terminal states (Completed, Failed, Killed).
    pub fn list_terminal(&self) -> Vec<SessionRecord> {
        self.registry
            .list()
            .into_iter()
            .filter(|r| r.current_state.is_terminal())
            .cloned()
            .collect()
    }

    /// Deactivate a session: inspect dirty state, push unpushed repos with
    /// rebase-retry, re-inspect, transition state, and release locks.
    ///
    /// Push failures are non-fatal — the session still transitions to
    /// Completed and the dirty state is returned so the caller can decide
    /// whether to launch the finalization subagent.
    pub fn deactivate_session(&mut self, session_id: &SessionId) -> Result<DeactivationResult> {
        let record = self.registry.get(session_id)
            .ok_or_else(|| anyhow::anyhow!("session {session_id} not found"))?;
        let workspace_path = record.workspace_path.clone()
            .ok_or_else(|| anyhow::anyhow!("session {session_id} has no workspace path"))?;

        // Push unpushed repos with rebase-retry (non-fatal on failure).
        let initial_dirty = self.workspace_ops.inspect_dirty_state(&workspace_path)?;
        for repo in &initial_dirty {
            if repo.unpushed_branches.is_empty() {
                continue;
            }
            let repo_path = if repo.repo_name == "team" {
                workspace_path.join("team")
            } else {
                workspace_path.join("projects").join(&repo.repo_name)
            };
            let _ = deactivation::push_with_rebase_retry(&repo_path, DEFAULT_MAX_RETRIES);
        }

        // Re-inspect dirty state after push attempts.
        let dirty_state = self.workspace_ops.inspect_dirty_state(&workspace_path)?;

        // Release work-item locks held by this session.
        self.work_item_lock.release_all(session_id);

        // Transition to Completed — the caller (daemon deactivation watcher)
        // examines dirty_state to decide whether to launch finalization.
        self.registry.update_state(session_id, SessionState::Completed)?;

        let updated = self.registry.get(session_id).unwrap().clone();
        Ok(DeactivationResult {
            session_record: updated,
            dirty_state,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::SessionState;

    struct FakeWorkspaceOps {
        workspace_path: PathBuf,
        dirty_state: Vec<RepoDirtyState>,
    }

    impl WorkspaceOps for FakeWorkspaceOps {
        fn hydrate_workspace(
            &self,
            _session_id: &SessionId,
            _member: &str,
        ) -> Result<PathBuf> {
            Ok(self.workspace_path.clone())
        }

        fn inspect_dirty_state(
            &self,
            _workspace_path: &Path,
        ) -> Result<Vec<RepoDirtyState>> {
            Ok(self.dirty_state.clone())
        }
    }

    fn make_manager(dirty_state: Vec<RepoDirtyState>) -> SessionManager<FakeWorkspaceOps> {
        let tmp = tempfile::tempdir().unwrap();
        let registry = SessionRegistry::new(tmp.path().join("registry.json"));
        let lock = WorkItemLock::new();
        let ops = FakeWorkspaceOps {
            workspace_path: tmp.path().join("workspace"),
            dirty_state,
        };
        SessionManager::new(registry, lock, ops)
    }

    // AC-1: Session Status Display (session creation returns correct fields)

    #[test]
    fn create_session_returns_session_with_active_state() {
        let mut mgr = make_manager(vec![]);
        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            work_item_id: None,
        };

        let record = mgr.create_session(params).unwrap();

        assert_eq!(record.current_state, SessionState::Active);
        assert_eq!(record.member_name, "alice");
        assert_eq!(record.session_type, SessionType::Interactive);
        assert!(!record.session_id.as_str().is_empty());
    }

    // AC-3: Work-Item Lock integration with session lifecycle

    #[test]
    fn create_session_acquires_work_item_lock_if_provided() {
        let mut mgr = make_manager(vec![]);
        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Loop,
            work_item_id: Some("ISSUE-42".to_string()),
        };

        let record = mgr.create_session(params).unwrap();

        // A second session trying to acquire the same work item must fail
        let other_session = SessionId::new();
        let err = mgr
            .work_item_lock
            .acquire("ISSUE-42", &other_session)
            .expect_err("work item must be locked by the first session");
        let _ = err; // Just verify it errors
        let _ = record;
    }

    // AC-5: Dirty State Reported on Deactivation

    #[test]
    fn deactivate_session_transitions_to_completed_when_clean() {
        let mut mgr = make_manager(vec![]);

        // Manually set up an Active session
        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert_eq!(
            result.session_record.current_state,
            SessionState::Completed,
            "clean workspace must transition to Completed"
        );
        assert!(
            result.dirty_state.iter().all(|r| r.is_clean()),
            "no dirty state should be reported for a clean workspace"
        );
    }

    #[test]
    fn deactivate_session_reports_dirty_state() {
        let dirty = vec![RepoDirtyState {
            repo_name: "myproject".to_string(),
            uncommitted_files: vec!["dirty.txt".to_string()],
            unpushed_branches: vec![],
        }];
        let mut mgr = make_manager(dirty);

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "bob".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert!(
            result.dirty_state.iter().any(|r| !r.is_clean()),
            "dirty workspace must be reported in deactivation result"
        );
    }

    #[test]
    fn list_terminal_returns_only_terminal_state_sessions() {
        let mut mgr = make_manager(vec![]);

        // Active session — NOT terminal
        let active_id = SessionId::new();
        let active = SessionRecord {
            session_id: active_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(active).unwrap();
        mgr.registry
            .update_state(&active_id, SessionState::Active)
            .unwrap();

        // Completed session — terminal
        let completed_id = SessionId::new();
        let completed = SessionRecord {
            session_id: completed_id.clone(),
            member_name: "bob".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(completed).unwrap();
        mgr.registry
            .update_state(&completed_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&completed_id, SessionState::Completed)
            .unwrap();

        // Failed session — terminal
        let failed_id = SessionId::new();
        let failed = SessionRecord {
            session_id: failed_id.clone(),
            member_name: "carol".to_string(),
            session_type: SessionType::Brain,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(failed).unwrap();
        mgr.registry
            .update_state(&failed_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&failed_id, SessionState::Failed)
            .unwrap();

        // Killed session — terminal
        let killed_id = SessionId::new();
        let killed = SessionRecord {
            session_id: killed_id.clone(),
            member_name: "dan".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(killed).unwrap();
        mgr.registry
            .update_state(&killed_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&killed_id, SessionState::Killed)
            .unwrap();

        // Finalizing session — NOT terminal
        let finalizing_id = SessionId::new();
        let finalizing = SessionRecord {
            session_id: finalizing_id.clone(),
            member_name: "eve".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(finalizing).unwrap();
        mgr.registry
            .update_state(&finalizing_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&finalizing_id, SessionState::Finalizing)
            .unwrap();

        // Retained session — NOT terminal
        let retained_id = SessionId::new();
        let retained = SessionRecord {
            session_id: retained_id.clone(),
            member_name: "frank".to_string(),
            session_type: SessionType::Brain,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(retained).unwrap();
        mgr.registry
            .update_state(&retained_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&retained_id, SessionState::Completed)
            .unwrap();
        mgr.registry
            .update_state(&retained_id, SessionState::Retained)
            .unwrap();

        let terminals = mgr.list_terminal();

        assert_eq!(
            terminals.len(),
            3,
            "should return exactly 3 terminal sessions (Completed, Failed, Killed)"
        );

        let ids: Vec<String> = terminals.iter().map(|r| r.session_id.to_string()).collect();
        assert!(
            ids.contains(&completed_id.to_string()),
            "Completed session must be included"
        );
        assert!(
            ids.contains(&failed_id.to_string()),
            "Failed session must be included"
        );
        assert!(
            ids.contains(&killed_id.to_string()),
            "Killed session must be included"
        );
    }

    // ---
    // State transition invariants
    // ---

    #[test]
    fn retained_to_finalizing_is_valid_transition() {
        assert!(
            SessionState::Retained.can_transition_to(&SessionState::Finalizing),
            "Retained -> Finalizing must be valid to support re-trigger finalization"
        );
    }

    #[test]
    fn new_session_while_old_is_finalizing() {
        let mut mgr = make_manager(vec![]);

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws1")),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Finalizing)
            .unwrap();

        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            work_item_id: None,
        };
        let result = mgr.create_session(params);

        assert!(
            result.is_ok(),
            "new session must succeed when old session is Finalizing"
        );
    }

    #[test]
    fn deactivate_session_releases_work_item_locks() {
        let mut mgr = make_manager(vec![]);

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "carol".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();

        // Manually acquire a lock for this session
        mgr.work_item_lock
            .acquire("ISSUE-99", &session_id)
            .unwrap();

        mgr.deactivate_session(&session_id).unwrap();

        // After deactivation, the lock must be released
        let other = SessionId::new();
        mgr.work_item_lock
            .acquire("ISSUE-99", &other)
            .expect("work item lock must be released after session deactivation");
    }

    #[test]
    fn new_session_while_old_is_failed() {
        let mut mgr = make_manager(vec![]);

        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "alice".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/ws1")),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Failed)
            .unwrap();

        let params = CreateSessionParams {
            member_name: "alice".to_string(),
            session_type: SessionType::Interactive,
            work_item_id: None,
        };
        let result = mgr.create_session(params);

        assert!(
            result.is_ok(),
            "new session must succeed when old session is Failed"
        );
    }
}

#[cfg(test)]
mod session_push_integration_tests {
    use super::*;
    use crate::session::dirty_state;
    use std::fs;
    use std::process::Command;

    struct RealWorkspaceOps {
        workspace_path: PathBuf,
    }

    impl WorkspaceOps for RealWorkspaceOps {
        fn hydrate_workspace(&self, _session_id: &SessionId, _member: &str) -> Result<PathBuf> {
            Ok(self.workspace_path.clone())
        }

        fn inspect_dirty_state(&self, workspace_path: &Path) -> Result<Vec<RepoDirtyState>> {
            dirty_state::inspect_dirty_state(workspace_path)
        }
    }

    fn git(dir: &Path, args: &[&str]) {
        let output = Command::new("git")
            .args(args)
            .current_dir(dir)
            .output()
            .unwrap();
        assert!(
            output.status.success(),
            "git {} failed: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }

    fn setup_workspace_with_pushed_repo(tmp: &Path) -> (PathBuf, PathBuf) {
        let ws = tmp.join("workspace");
        let projects = ws.join("projects");
        let bare = tmp.join("origin.git");
        let repo = projects.join("myproject");

        Command::new("git")
            .args(["init", "--bare", "-b", "main", bare.to_str().unwrap()])
            .output()
            .unwrap();

        fs::create_dir_all(&projects).unwrap();
        Command::new("git")
            .args(["clone", bare.to_str().unwrap(), repo.to_str().unwrap()])
            .output()
            .unwrap();

        git(&repo, &["config", "user.email", "test@test.com"]);
        git(&repo, &["config", "user.name", "Test"]);
        git(&repo, &["config", "commit.gpgsign", "false"]);

        fs::write(repo.join("README.md"), "initial").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "initial"]);
        git(&repo, &["push", "-u", "origin", "main"]);

        (ws, bare)
    }

    fn advance_remote(tmp: &Path, bare: &Path) {
        let advancer = tmp.join("advancer");
        Command::new("git")
            .args(["clone", bare.to_str().unwrap(), advancer.to_str().unwrap()])
            .output()
            .unwrap();
        git(&advancer, &["config", "user.email", "adv@test.com"]);
        git(&advancer, &["config", "user.name", "Advancer"]);
        git(&advancer, &["config", "commit.gpgsign", "false"]);
        fs::write(advancer.join("remote.txt"), "remote content").unwrap();
        git(&advancer, &["add", "."]);
        git(&advancer, &["commit", "-m", "advance remote"]);
        git(&advancer, &["push", "origin", "main"]);
    }

    fn install_always_rejecting_hook(bare: &Path) {
        let hooks_dir = bare.join("hooks");
        fs::create_dir_all(&hooks_dir).unwrap();
        let hook = hooks_dir.join("pre-receive");
        fs::write(
            &hook,
            r#"#!/bin/bash
while read old new ref; do true; done
PARENT=$(git rev-parse refs/heads/main)
TREE=$(git rev-parse "$PARENT^{tree}")
NEW=$(echo "advance" | GIT_COMMITTER_NAME=hook GIT_COMMITTER_EMAIL=hook@test GIT_AUTHOR_NAME=hook GIT_AUTHOR_EMAIL=hook@test git commit-tree "$TREE" -p "$PARENT")
git update-ref refs/heads/main "$NEW"
echo "! [rejected] main -> main (non-fast-forward)" >&2
exit 1
"#,
        )
        .unwrap();
        Command::new("chmod")
            .args(["+x", hook.to_str().unwrap()])
            .output()
            .unwrap();
    }

    fn make_manager_with_real_ops(
        workspace_path: PathBuf,
    ) -> SessionManager<RealWorkspaceOps> {
        let tmp = tempfile::tempdir().unwrap();
        let registry = SessionRegistry::new(tmp.path().join("registry.json"));
        let lock = WorkItemLock::new();
        let ops = RealWorkspaceOps { workspace_path };
        SessionManager::new(registry, lock, ops)
    }

    fn setup_active_session(mgr: &mut SessionManager<RealWorkspaceOps>) -> SessionId {
        let session_id = SessionId::new();
        let record = SessionRecord {
            session_id: session_id.clone(),
            member_name: "test".to_string(),
            session_type: SessionType::Interactive,
            current_state: SessionState::Creating,
            created_at: chrono::Utc::now(),
            state_transitioned_at: chrono::Utc::now(),
            agent_pid: None,
            workspace_path: Some(mgr.workspace_ops.workspace_path.clone()),
            finalization_result: None,
            finalization_agent_pid: None,
        };
        mgr.registry.register(record).unwrap();
        mgr.registry
            .update_state(&session_id, SessionState::Active)
            .unwrap();
        session_id
    }

    #[test]
    fn push_succeeds_repo_no_longer_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, _bare) = setup_workspace_with_pushed_repo(tmp.path());
        let repo = ws.join("projects").join("myproject");

        fs::write(repo.join("new.txt"), "new content").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "unpushed"]);

        let mut mgr = make_manager_with_real_ops(ws);
        let session_id = setup_active_session(&mut mgr);

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert!(
            result
                .dirty_state
                .iter()
                .all(|r| r.unpushed_branches.is_empty()),
            "After deactivation, push should clear unpushed branches. Got: {:?}",
            result.dirty_state
        );
    }

    #[test]
    fn nff_rebase_retry_succeeds_repo_no_longer_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, bare) = setup_workspace_with_pushed_repo(tmp.path());
        let repo = ws.join("projects").join("myproject");

        advance_remote(tmp.path(), &bare);

        fs::write(repo.join("local.txt"), "local content").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "local"]);

        let mut mgr = make_manager_with_real_ops(ws);
        let session_id = setup_active_session(&mut mgr);

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert!(
            result
                .dirty_state
                .iter()
                .all(|r| r.unpushed_branches.is_empty()),
            "After rebase+retry, push should clear unpushed branches. Got: {:?}",
            result.dirty_state
        );
    }

    #[test]
    fn push_fails_max_retries_repo_stays_dirty() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, bare) = setup_workspace_with_pushed_repo(tmp.path());
        let repo = ws.join("projects").join("myproject");

        advance_remote(tmp.path(), &bare);
        install_always_rejecting_hook(&bare);

        fs::write(repo.join("local.txt"), "local content").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "local"]);

        let mut mgr = make_manager_with_real_ops(ws);
        let session_id = setup_active_session(&mut mgr);

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert!(
            result
                .dirty_state
                .iter()
                .any(|r| !r.unpushed_branches.is_empty()),
            "After push failure, repo should still have unpushed branches"
        );
    }

    #[test]
    fn uncommitted_only_repos_not_pushed() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, _bare) = setup_workspace_with_pushed_repo(tmp.path());
        let repo = ws.join("projects").join("myproject");

        fs::write(repo.join("dirty.txt"), "uncommitted content").unwrap();

        let mut mgr = make_manager_with_real_ops(ws);
        let session_id = setup_active_session(&mut mgr);

        let result = mgr.deactivate_session(&session_id).unwrap();

        assert!(
            result
                .dirty_state
                .iter()
                .any(|r| !r.uncommitted_files.is_empty()),
            "Uncommitted files should be reported"
        );
        assert!(
            result
                .dirty_state
                .iter()
                .all(|r| r.unpushed_branches.is_empty()),
            "No push should be attempted for uncommitted-only repos"
        );
    }

    #[test]
    fn push_failure_is_non_fatal() {
        let tmp = tempfile::tempdir().unwrap();
        let (ws, _bare) = setup_workspace_with_pushed_repo(tmp.path());
        let repo = ws.join("projects").join("myproject");

        git(&repo, &["remote", "set-url", "origin", "/nonexistent/path.git"]);

        fs::write(repo.join("new.txt"), "content").unwrap();
        git(&repo, &["add", "."]);
        git(&repo, &["commit", "-m", "unpushed"]);

        let mut mgr = make_manager_with_real_ops(ws);
        let session_id = setup_active_session(&mut mgr);

        let result = mgr.deactivate_session(&session_id);

        assert!(
            result.is_ok(),
            "deactivate_session must return Ok even when push fails"
        );
    }
}
