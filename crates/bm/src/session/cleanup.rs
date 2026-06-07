use std::path::PathBuf;

use anyhow::Result;
use chrono::{DateTime, Duration, Utc};

use super::registry::SessionRegistry;
use super::types::{FinalizationResult, GitState, RepoGitState, SessionId, SessionState, SessionType};

/// Structured summary of a retained session for operator inspection.
pub struct SessionInspection {
    pub session_id: SessionId,
    pub member_name: String,
    pub session_type: SessionType,
    pub current_state: SessionState,
    pub workspace_path: Option<PathBuf>,
    pub created_at: DateTime<Utc>,
    pub finalization_result: Option<FinalizationResult>,
    pub git_state: Option<GitState>,
}

/// Report of a single session cleanup operation.
#[derive(Debug)]
pub struct CleanupReport {
    pub session_id: SessionId,
    pub workspace_removed: bool,
    pub registry_removed: bool,
}

/// Filter for bulk cleanup of retained sessions.
pub enum CleanupFilter {
    AllRetained,
    ByMember(String),
    OlderThan(Duration),
}

/// Compute the git state of all repos within a session workspace.
pub fn compute_git_state(workspace_path: &std::path::Path) -> Option<GitState> {
    let projects_dir = workspace_path.join("projects");
    let entries = std::fs::read_dir(&projects_dir).ok()?;

    let mut repos = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() || !path.join(".git").exists() {
            continue;
        }

        let repo_name = entry.file_name().to_string_lossy().to_string();

        let current_branch = std::process::Command::new("git")
            .args(["rev-parse", "--abbrev-ref", "HEAD"])
            .current_dir(&path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string());

        let uncommitted_files = std::process::Command::new("git")
            .args(["status", "--porcelain"])
            .current_dir(&path)
            .output()
            .ok()
            .filter(|o| o.status.success())
            .map(|o| {
                String::from_utf8_lossy(&o.stdout)
                    .lines()
                    .filter(|l| !l.is_empty())
                    .map(|l| l.to_string())
                    .collect()
            })
            .unwrap_or_default();

        let has_remote = std::process::Command::new("git")
            .args(["remote"])
            .current_dir(&path)
            .output()
            .ok()
            .map(|o| !String::from_utf8_lossy(&o.stdout).trim().is_empty())
            .unwrap_or(false);

        let unpushed_branches = if has_remote {
            std::process::Command::new("git")
                .args(["log", "--branches", "--not", "--remotes", "--oneline"])
                .current_dir(&path)
                .output()
                .ok()
                .filter(|o| o.status.success())
                .map(|o| {
                    String::from_utf8_lossy(&o.stdout)
                        .lines()
                        .filter(|l| !l.is_empty())
                        .map(|l| l.to_string())
                        .collect()
                })
                .unwrap_or_default()
        } else {
            vec![]
        };

        repos.push(RepoGitState {
            repo_name,
            current_branch,
            uncommitted_files,
            unpushed_branches,
        });
    }

    if repos.is_empty() {
        None
    } else {
        Some(GitState { repos })
    }
}

/// Inspect a retained session, returning a structured summary.
pub fn inspect_session(
    registry: &SessionRegistry,
    session_id: &SessionId,
) -> Result<SessionInspection> {
    let record = registry
        .get(session_id)
        .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))?;

    let git_state = record
        .workspace_path
        .as_ref()
        .and_then(|p| compute_git_state(p));

    Ok(SessionInspection {
        session_id: record.session_id.clone(),
        member_name: record.member_name.clone(),
        session_type: record.session_type.clone(),
        current_state: record.current_state.clone(),
        workspace_path: record.workspace_path.clone(),
        created_at: record.created_at,
        finalization_result: record.finalization_result.clone(),
        git_state,
    })
}

/// Clean up a single retained session: remove workspace directory and remove from registry.
/// Only sessions in terminal or Retained states can be cleaned up.
pub fn cleanup_session(
    registry: &mut SessionRegistry,
    session_id: &SessionId,
) -> Result<CleanupReport> {
    let record = registry
        .get(session_id)
        .ok_or_else(|| anyhow::anyhow!("Session {} not found", session_id))?
        .clone();

    match record.current_state {
        SessionState::Completed
        | SessionState::Failed
        | SessionState::Killed
        | SessionState::Retained => {}
        ref state => {
            anyhow::bail!(
                "Session {} is in state {} and cannot be cleaned up. \
                 Only Completed, Failed, Killed, or Retained sessions can be cleaned up.",
                session_id,
                state
            );
        }
    }

    let workspace_removed = record
        .workspace_path
        .as_ref()
        .is_some_and(|p| p.is_dir() && std::fs::remove_dir_all(p).is_ok());

    registry.remove(session_id)?;

    Ok(CleanupReport {
        session_id: record.session_id,
        workspace_removed,
        registry_removed: true,
    })
}

/// Bulk cleanup of retained sessions matching a filter. Returns one report per cleaned session.
pub fn bulk_cleanup(
    registry: &mut SessionRegistry,
    filter: CleanupFilter,
) -> Result<Vec<CleanupReport>> {
    let ids: Vec<SessionId> = registry
        .list()
        .into_iter()
        .filter(|r| r.current_state == SessionState::Retained)
        .filter(|r| match &filter {
            CleanupFilter::AllRetained => true,
            CleanupFilter::ByMember(name) => r.member_name == *name,
            CleanupFilter::OlderThan(duration) => {
                Utc::now() - r.created_at > *duration
            }
        })
        .map(|r| r.session_id.clone())
        .collect();

    let mut reports = Vec::with_capacity(ids.len());
    for id in &ids {
        reports.push(cleanup_session(registry, id)?);
    }
    Ok(reports)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{SessionRecord, SessionState, SessionType};

    fn make_retained_record(member: &str, session_type: SessionType) -> SessionRecord {
        SessionRecord {
            session_id: SessionId::new(),
            member_name: member.to_string(),
            session_type,
            current_state: SessionState::Creating,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/workspace-test")),
            finalization_result: None,
                finalization_agent_pid: None,
        }
    }

    fn register_retained(registry: &mut SessionRegistry, record: SessionRecord) -> SessionId {
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry.update_state(&id, SessionState::Active).unwrap();
        registry
            .update_state(&id, SessionState::Completed)
            .unwrap();
        registry
            .update_state(&id, SessionState::Retained)
            .unwrap();
        id
    }

    fn new_registry() -> SessionRegistry {
        let tmp = tempfile::tempdir().unwrap();
        SessionRegistry::new(tmp.path().join("registry.json"))
    }

    // --- AC-18 (partial): Session Inspection ---

    #[test]
    fn inspect_retained_session_returns_workspace_path() {
        let mut registry = new_registry();
        let record = make_retained_record("alice", SessionType::Loop);
        let id = register_retained(&mut registry, record);

        let inspection = inspect_session(&registry, &id).unwrap();

        assert_eq!(
            inspection.workspace_path,
            Some(PathBuf::from("/tmp/workspace-test")),
            "inspection must include the session's workspace path"
        );
    }

    #[test]
    fn inspect_retained_session_returns_member_and_type() {
        let mut registry = new_registry();
        let record = make_retained_record("bob", SessionType::Brain);
        let id = register_retained(&mut registry, record);

        let inspection = inspect_session(&registry, &id).unwrap();

        assert_eq!(
            inspection.member_name, "bob",
            "inspection must include the correct member name"
        );
        assert_eq!(
            inspection.session_type,
            SessionType::Brain,
            "inspection must include the correct session type"
        );
        assert_eq!(
            inspection.current_state,
            SessionState::Retained,
            "inspection must reflect the current Retained state"
        );
    }

    #[test]
    fn inspect_session_returns_correct_session_id() {
        let mut registry = new_registry();
        let record = make_retained_record("carol", SessionType::Interactive);
        let id = register_retained(&mut registry, record);

        let inspection = inspect_session(&registry, &id).unwrap();

        assert_eq!(
            inspection.session_id, id,
            "inspection must return the queried session's ID, not a stub"
        );
    }

    #[test]
    fn inspect_nonexistent_session_returns_error() {
        let registry = new_registry();
        let phantom_id = SessionId::from_raw("nonexist");

        let result = inspect_session(&registry, &phantom_id);

        assert!(
            result.is_err(),
            "inspecting a nonexistent session must return an error"
        );
    }

    // --- Individual Cleanup ---

    #[test]
    fn cleanup_session_removes_from_registry() {
        let mut registry = new_registry();
        let record = make_retained_record("alice", SessionType::Loop);
        let id = register_retained(&mut registry, record);

        cleanup_session(&mut registry, &id).unwrap();

        assert!(
            registry.get(&id).is_none(),
            "session must be removed from registry after cleanup"
        );
    }

    #[test]
    fn cleanup_session_reports_registry_removed() {
        let mut registry = new_registry();
        let record = make_retained_record("bob", SessionType::Interactive);
        let id = register_retained(&mut registry, record);

        let report = cleanup_session(&mut registry, &id).unwrap();

        assert!(
            report.registry_removed,
            "cleanup report must indicate registry entry was removed"
        );
        assert_eq!(
            report.session_id, id,
            "cleanup report must reference the correct session ID"
        );
    }

    #[test]
    fn cleanup_nonexistent_session_returns_error() {
        let mut registry = new_registry();
        let phantom_id = SessionId::from_raw("nonexist");

        let result = cleanup_session(&mut registry, &phantom_id);

        assert!(
            result.is_err(),
            "cleaning up a nonexistent session must return an error"
        );
    }

    #[test]
    fn cleanup_active_session_rejected() {
        let mut registry = new_registry();
        let record = make_retained_record("alice", SessionType::Loop);
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry.update_state(&id, SessionState::Active).unwrap();

        let result = cleanup_session(&mut registry, &id);

        assert!(
            result.is_err(),
            "cleaning up an Active session must be rejected"
        );
        let msg = result.unwrap_err().to_string();
        assert!(
            msg.contains("Active") && msg.contains("cannot be cleaned up"),
            "error must describe the state guard violation, got: {msg}"
        );
    }

    #[test]
    fn cleanup_creating_session_rejected() {
        let mut registry = new_registry();
        let record = make_retained_record("bob", SessionType::Interactive);
        let id = record.session_id.clone();
        registry.register(record).unwrap();

        let result = cleanup_session(&mut registry, &id);

        assert!(
            result.is_err(),
            "cleaning up a Creating session must be rejected"
        );
    }

    #[test]
    fn cleanup_completed_session_allowed() {
        let mut registry = new_registry();
        let record = make_retained_record("carol", SessionType::Brain);
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry.update_state(&id, SessionState::Active).unwrap();
        registry.update_state(&id, SessionState::Completed).unwrap();

        let result = cleanup_session(&mut registry, &id);

        assert!(
            result.is_ok(),
            "cleaning up a Completed session must be allowed"
        );
    }

    // --- Bulk Cleanup ---

    #[test]
    fn bulk_cleanup_by_member_returns_only_matching() {
        let mut registry = new_registry();
        let r1 = make_retained_record("alice", SessionType::Loop);
        let r2 = make_retained_record("bob", SessionType::Interactive);
        let r3 = make_retained_record("alice", SessionType::Brain);
        let id1 = register_retained(&mut registry, r1);
        let _id2 = register_retained(&mut registry, r2);
        let id3 = register_retained(&mut registry, r3);

        let reports =
            bulk_cleanup(&mut registry, CleanupFilter::ByMember("alice".to_string())).unwrap();

        assert_eq!(
            reports.len(),
            2,
            "bulk cleanup by member must return exactly the matching sessions"
        );
        let cleaned_ids: Vec<_> = reports.iter().map(|r| r.session_id.as_str().to_string()).collect();
        assert!(
            cleaned_ids.contains(&id1.as_str().to_string()),
            "alice's first session must be cleaned"
        );
        assert!(
            cleaned_ids.contains(&id3.as_str().to_string()),
            "alice's second session must be cleaned"
        );
    }

    #[test]
    fn bulk_cleanup_by_age_returns_only_old() {
        let mut registry = new_registry();

        let mut old_record = make_retained_record("alice", SessionType::Loop);
        old_record.created_at = Utc::now() - chrono::Duration::hours(48);
        let old_id = register_retained(&mut registry, old_record);

        let fresh_record = make_retained_record("bob", SessionType::Interactive);
        let _fresh_id = register_retained(&mut registry, fresh_record);

        let reports = bulk_cleanup(
            &mut registry,
            CleanupFilter::OlderThan(chrono::Duration::hours(24)),
        )
        .unwrap();

        assert_eq!(
            reports.len(),
            1,
            "bulk cleanup by age must return only sessions older than the threshold"
        );
        assert_eq!(
            reports[0].session_id, old_id,
            "only the old session must be cleaned"
        );
    }

    #[test]
    fn bulk_cleanup_all_retained_returns_all() {
        let mut registry = new_registry();
        let r1 = make_retained_record("alice", SessionType::Loop);
        let r2 = make_retained_record("bob", SessionType::Interactive);
        register_retained(&mut registry, r1);
        register_retained(&mut registry, r2);

        let reports = bulk_cleanup(&mut registry, CleanupFilter::AllRetained).unwrap();

        assert_eq!(
            reports.len(),
            2,
            "bulk cleanup AllRetained must return all retained sessions"
        );
    }

    // --- Cleanup Independence from GC ---

    #[test]
    fn cleanup_does_not_conflict_with_concurrent_cleanup() {
        let mut registry = new_registry();
        let r1 = make_retained_record("alice", SessionType::Loop);
        let r2 = make_retained_record("bob", SessionType::Brain);
        let id1 = register_retained(&mut registry, r1);
        let id2 = register_retained(&mut registry, r2);

        let report1 = cleanup_session(&mut registry, &id1);
        let report2 = cleanup_session(&mut registry, &id2);

        assert!(
            report1.is_ok(),
            "first cleanup must succeed independently"
        );
        assert!(
            report2.is_ok(),
            "second cleanup must succeed independently — no conflict with first"
        );
        assert!(
            registry.get(&id1).is_none(),
            "first session must be removed after cleanup"
        );
        assert!(
            registry.get(&id2).is_none(),
            "second session must be removed after cleanup"
        );
    }

    // --- CT-89-06: AC-18 Fix — FinalizationResult + SessionInspection ---

    #[test]
    fn inspect_session_populates_finalization_from_record() {
        use crate::session::types::{
            CommittedRepo, FinalizationExitStatus, FinalizationResult as TypesFinalizationResult,
        };

        let mut registry = new_registry();
        let mut record = make_retained_record("alice", SessionType::Loop);
        record.finalization_result = Some(TypesFinalizationResult {
            exit_status: FinalizationExitStatus::Completed,
            committed_repos: vec![CommittedRepo {
                repo_name: "botminter".to_string(),
                branch: "feature/story-88".to_string(),
            }],
            pushed_branches: vec!["feature/story-88".to_string()],
            recovery_branches: vec![],
            github_issue_urls: vec![],
        });
        let id = register_retained(&mut registry, record);

        let inspection = inspect_session(&registry, &id).unwrap();

        assert!(
            inspection.finalization_result.is_some(),
            "inspect_session must copy finalization_result from SessionRecord, got None"
        );
        let fin = inspection.finalization_result.unwrap();
        assert_eq!(
            fin.exit_status,
            FinalizationExitStatus::Completed,
            "finalization exit_status must match the record"
        );
        assert_eq!(
            fin.committed_repos.len(),
            1,
            "finalization committed_repos must be copied from record"
        );
    }

    #[test]
    fn inspect_session_computes_git_state_for_workspace_with_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let ws = tmp.path().join("workspace");
        let projects = ws.join("projects");
        let repo = projects.join("myrepo");
        std::fs::create_dir_all(&repo).unwrap();

        std::process::Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.email", "test@test.com"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "user.name", "Test"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["config", "commit.gpgsign", "false"])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::fs::write(repo.join("README.md"), "hello").unwrap();
        std::process::Command::new("git")
            .args(["add", "."])
            .current_dir(&repo)
            .output()
            .unwrap();
        std::process::Command::new("git")
            .args(["commit", "-m", "init"])
            .current_dir(&repo)
            .output()
            .unwrap();

        let mut registry = new_registry();
        let mut record = make_retained_record("bob", SessionType::Interactive);
        record.workspace_path = Some(ws.clone());
        let id = register_retained(&mut registry, record);

        let inspection = inspect_session(&registry, &id).unwrap();

        assert!(
            inspection.git_state.is_some(),
            "inspect_session must compute git_state for a workspace with repos, got None"
        );
        let state = inspection.git_state.unwrap();
        assert!(
            !state.repos.is_empty(),
            "git_state must contain at least one repo entry"
        );
        assert_eq!(
            state.repos[0].repo_name, "myrepo",
            "git_state repo entry must have the correct name"
        );
    }
}
