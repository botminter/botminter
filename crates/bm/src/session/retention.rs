use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::Result;
use chrono::{DateTime, Utc};

use super::cleanup::cleanup_session;
use super::registry::SessionRegistry;
use super::types::{FinalizationExitStatus, SessionId, SessionState, SessionType};

pub struct RetentionPolicy {
    pub loop_brain_duration: Duration,
    pub interactive_success_duration: Duration,
    pub interactive_failure_duration: Duration,
    pub disk_budget_bytes: Option<u64>,
    pub scan_interval: Duration,
}

impl Default for RetentionPolicy {
    fn default() -> Self {
        Self {
            loop_brain_duration: Duration::from_secs(24 * 3600),
            interactive_success_duration: Duration::ZERO,
            interactive_failure_duration: Duration::from_secs(24 * 3600),
            disk_budget_bytes: None,
            scan_interval: Duration::from_secs(300),
        }
    }
}

impl RetentionPolicy {
    pub fn duration_for(&self, session_type: &SessionType, was_failure: bool) -> Duration {
        match session_type {
            SessionType::Loop | SessionType::Brain => self.loop_brain_duration,
            SessionType::Interactive => {
                if was_failure {
                    self.interactive_failure_duration
                } else {
                    self.interactive_success_duration
                }
            }
        }
    }

    pub fn expired_sessions(&self, sessions: &[RetainedSessionInfo]) -> Vec<SessionId> {
        let now = Utc::now();
        sessions
            .iter()
            .filter(|s| {
                let max_age = self.duration_for(&s.session_type, s.was_failure);
                let elapsed = (now - s.retained_at).to_std().unwrap_or(Duration::ZERO);
                elapsed >= max_age
            })
            .map(|s| s.session_id.clone())
            .collect()
    }

    pub fn over_budget_sessions<D: DiskUsageProvider>(
        &self,
        sessions: &[RetainedSessionInfo],
        disk_provider: &D,
    ) -> Vec<SessionId> {
        let budget = match self.disk_budget_bytes {
            Some(b) => b,
            None => return vec![],
        };

        let mut measured: Vec<_> = sessions
            .iter()
            .filter_map(|s| {
                let path = s.workspace_path.as_ref()?;
                let size = disk_provider.workspace_disk_usage(path).unwrap_or(0);
                Some((&s.session_id, s.retained_at, size))
            })
            .collect();

        let total: u64 = measured.iter().map(|(_, _, sz)| sz).sum();
        if total <= budget {
            return vec![];
        }

        measured.sort_by_key(|(_, retained_at, _)| *retained_at);

        let mut to_evict = Vec::new();
        let mut remaining = total;
        for (id, _, size) in &measured {
            if remaining <= budget {
                break;
            }
            to_evict.push((*id).clone());
            remaining -= size;
        }

        to_evict
    }
}

pub struct RetainedSessionInfo {
    pub session_id: SessionId,
    pub session_type: SessionType,
    pub retained_at: DateTime<Utc>,
    pub workspace_path: Option<PathBuf>,
    pub was_failure: bool,
}

pub trait DiskUsageProvider: Send + Sync {
    fn workspace_disk_usage(&self, path: &Path) -> Result<u64>;
}

pub trait ProcessChecker: Send + Sync {
    fn is_pid_alive(&self, pid: u32) -> bool;
}

pub fn recover_stale_sessions<P: ProcessChecker>(
    registry: &mut SessionRegistry,
    process_checker: &P,
) -> Vec<SessionId> {
    let stale_ids: Vec<_> = registry
        .list()
        .into_iter()
        .filter(|r| {
            matches!(r.current_state, SessionState::Active | SessionState::Finalizing)
                && r.agent_pid
                    .is_none_or(|pid| !process_checker.is_pid_alive(pid))
        })
        .map(|r| r.session_id.clone())
        .collect();

    let mut recovered = Vec::new();
    for id in stale_ids {
        if registry.update_state(&id, SessionState::Failed).is_ok() {
            recovered.push(id);
        }
    }

    recovered
}

/// Check if a process with the given PID is alive using OS-level probing.
/// Delegates to `state::is_alive` which includes zombie detection via `/proc`.
pub fn is_pid_alive(pid: u32) -> bool {
    crate::state::is_alive(pid)
}

/// Concrete ProcessChecker that uses OS-level PID probing.
pub struct LiveProcessChecker;

impl ProcessChecker for LiveProcessChecker {
    fn is_pid_alive(&self, pid: u32) -> bool {
        is_pid_alive(pid)
    }
}

/// Concrete DiskUsageProvider that measures actual filesystem usage.
pub struct FsDiskUsage;

impl DiskUsageProvider for FsDiskUsage {
    fn workspace_disk_usage(&self, path: &Path) -> Result<u64> {
        fn dir_size(path: &Path) -> u64 {
            let mut total = 0;
            if let Ok(entries) = std::fs::read_dir(path) {
                for entry in entries.flatten() {
                    let p = entry.path();
                    if p.is_dir() {
                        total += dir_size(&p);
                    } else if let Ok(meta) = entry.metadata() {
                        total += meta.len();
                    }
                }
            }
            total
        }
        Ok(dir_size(path))
    }
}

/// Run one GC cycle: evaluate retention policy, enforce disk budget, clean up expired sessions.
pub fn run_cycle<D: DiskUsageProvider>(
    registry: &mut SessionRegistry,
    policy: &RetentionPolicy,
    disk_provider: &D,
) -> Vec<SessionId> {
    let retained: Vec<RetainedSessionInfo> = registry
        .list()
        .into_iter()
        .filter(|r| r.current_state == SessionState::Retained)
        .map(|r| RetainedSessionInfo {
            session_id: r.session_id.clone(),
            session_type: r.session_type.clone(),
            retained_at: r.state_transitioned_at,
            workspace_path: r.workspace_path.clone(),
            was_failure: r
                .finalization_result
                .as_ref()
                .is_some_and(|f| matches!(f.exit_status, FinalizationExitStatus::Failed)),
        })
        .collect();

    let mut to_clean = policy.expired_sessions(&retained);
    for id in policy.over_budget_sessions(&retained, disk_provider) {
        if !to_clean.contains(&id) {
            to_clean.push(id);
        }
    }

    let mut cleaned = Vec::new();
    for id in &to_clean {
        if cleanup_session(registry, id).is_ok() {
            cleaned.push(id.clone());
        }
    }
    cleaned
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{SessionRecord, SessionState, SessionType};
    use std::collections::{HashMap, HashSet};

    fn make_retained_info(
        session_type: SessionType,
        retained_hours_ago: i64,
        was_failure: bool,
    ) -> RetainedSessionInfo {
        RetainedSessionInfo {
            session_id: SessionId::new(),
            session_type,
            retained_at: Utc::now() - chrono::Duration::hours(retained_hours_ago),
            workspace_path: Some(PathBuf::from("/tmp/workspace")),
            was_failure,
        }
    }

    fn new_registry() -> SessionRegistry {
        let tmp = tempfile::tempdir().unwrap();
        SessionRegistry::new(tmp.path().join("registry.json"))
    }

    fn register_with_pid(
        registry: &mut SessionRegistry,
        target_state: SessionState,
        pid: u32,
    ) -> SessionId {
        let record = SessionRecord {
            session_id: SessionId::new(),
            member_name: "test".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: Some(pid),
            workspace_path: Some(PathBuf::from("/tmp/workspace")),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry
            .update_state(&id, SessionState::Active)
            .unwrap();
        if target_state == SessionState::Finalizing {
            registry
                .update_state(&id, SessionState::Finalizing)
                .unwrap();
        }
        id
    }

    struct FakeProcessChecker {
        alive_pids: HashSet<u32>,
    }

    impl ProcessChecker for FakeProcessChecker {
        fn is_pid_alive(&self, pid: u32) -> bool {
            self.alive_pids.contains(&pid)
        }
    }

    struct FakeDiskUsage {
        sizes: HashMap<PathBuf, u64>,
    }

    impl DiskUsageProvider for FakeDiskUsage {
        fn workspace_disk_usage(&self, path: &Path) -> Result<u64> {
            Ok(*self.sizes.get(path).unwrap_or(&0))
        }
    }

    // --- AC-20: Loop/Brain Retention ---

    #[test]
    fn retained_loop_past_duration_is_expired() {
        let session = make_retained_info(SessionType::Loop, 25, false);
        let policy = RetentionPolicy::default();

        let expired = policy.expired_sessions(&[session]);

        assert_eq!(
            expired.len(),
            1,
            "loop session retained for 25h (past 24h default) must be in expiry list"
        );
    }

    #[test]
    fn retained_brain_past_duration_is_expired() {
        let session = make_retained_info(SessionType::Brain, 25, false);
        let policy = RetentionPolicy::default();

        let expired = policy.expired_sessions(&[session]);

        assert_eq!(
            expired.len(),
            1,
            "brain session retained for 25h (past 24h default) must be in expiry list"
        );
    }

    #[test]
    fn retained_loop_within_duration_not_expired() {
        let session = make_retained_info(SessionType::Loop, 1, false);
        let policy = RetentionPolicy::default();

        let expired = policy.expired_sessions(&[session]);

        assert!(
            expired.is_empty(),
            "loop session retained for 1h (within 24h default) must not be expired"
        );
    }

    #[test]
    fn successful_interactive_immediately_expired() {
        let session = make_retained_info(SessionType::Interactive, 0, false);
        let policy = RetentionPolicy::default();

        let expired = policy.expired_sessions(&[session]);

        assert_eq!(
            expired.len(),
            1,
            "successful interactive session has zero retention — must be immediately expired"
        );
    }

    // --- AC-21: Disk Budget Enforcement ---

    #[test]
    fn disk_budget_exceeded_evicts_oldest_first() {
        let ws1 = PathBuf::from("/tmp/ws-oldest");
        let ws2 = PathBuf::from("/tmp/ws-middle");
        let ws3 = PathBuf::from("/tmp/ws-newest");

        let sessions = vec![
            RetainedSessionInfo {
                session_id: SessionId::from_raw("oldest"),
                session_type: SessionType::Loop,
                retained_at: Utc::now() - chrono::Duration::hours(48),
                workspace_path: Some(ws1.clone()),
                was_failure: false,
            },
            RetainedSessionInfo {
                session_id: SessionId::from_raw("middle"),
                session_type: SessionType::Loop,
                retained_at: Utc::now() - chrono::Duration::hours(24),
                workspace_path: Some(ws2.clone()),
                was_failure: false,
            },
            RetainedSessionInfo {
                session_id: SessionId::from_raw("newest"),
                session_type: SessionType::Loop,
                retained_at: Utc::now() - chrono::Duration::hours(1),
                workspace_path: Some(ws3.clone()),
                was_failure: false,
            },
        ];

        let mut sizes = HashMap::new();
        sizes.insert(ws1, 500_000_000);
        sizes.insert(ws2, 500_000_000);
        sizes.insert(ws3, 500_000_000);
        let disk_provider = FakeDiskUsage { sizes };

        let policy = RetentionPolicy {
            disk_budget_bytes: Some(1_000_000_000),
            ..RetentionPolicy::default()
        };

        let to_evict = policy.over_budget_sessions(&sessions, &disk_provider);

        assert!(
            !to_evict.is_empty(),
            "must evict sessions when total 1.5GB exceeds 1GB budget"
        );
        assert_eq!(
            to_evict[0].as_str(),
            "oldest",
            "oldest session must be evicted first"
        );
    }

    #[test]
    fn disk_budget_not_exceeded_evicts_nothing() {
        let session = RetainedSessionInfo {
            session_id: SessionId::from_raw("only"),
            session_type: SessionType::Loop,
            retained_at: Utc::now(),
            workspace_path: Some(PathBuf::from("/tmp/ws")),
            was_failure: false,
        };

        let mut sizes = HashMap::new();
        sizes.insert(PathBuf::from("/tmp/ws"), 100_000_000);
        let disk_provider = FakeDiskUsage { sizes };

        let policy = RetentionPolicy {
            disk_budget_bytes: Some(1_000_000_000),
            ..RetentionPolicy::default()
        };

        let to_evict = policy.over_budget_sessions(&[session], &disk_provider);

        assert!(
            to_evict.is_empty(),
            "must not evict when 100MB is within 1GB budget"
        );
    }

    // --- AC-25: Daemon Restart Recovery ---

    #[test]
    fn stale_active_session_transitions_to_failed() {
        let mut registry = new_registry();
        let id = register_with_pid(&mut registry, SessionState::Active, 99999);

        let checker = FakeProcessChecker {
            alive_pids: HashSet::new(),
        };

        let recovered = recover_stale_sessions(&mut registry, &checker);

        assert!(
            recovered.contains(&id),
            "Active session with dead PID must be in recovered list"
        );
        assert_eq!(
            registry.get(&id).unwrap().current_state,
            SessionState::Failed,
            "Active session with dead PID must be transitioned to Failed"
        );
    }

    #[test]
    fn stale_finalizing_session_transitions_to_failed() {
        let mut registry = new_registry();
        let id = register_with_pid(&mut registry, SessionState::Finalizing, 99998);

        let checker = FakeProcessChecker {
            alive_pids: HashSet::new(),
        };

        let recovered = recover_stale_sessions(&mut registry, &checker);

        assert!(
            recovered.contains(&id),
            "Finalizing session with dead PID must be in recovered list"
        );
        assert_eq!(
            registry.get(&id).unwrap().current_state,
            SessionState::Failed,
            "Finalizing session with dead PID must be transitioned to Failed"
        );
    }

    #[test]
    fn active_session_with_live_pid_not_recovered() {
        let mut registry = new_registry();
        let id = register_with_pid(&mut registry, SessionState::Active, 12345);

        let mut alive = HashSet::new();
        alive.insert(12345u32);
        let checker = FakeProcessChecker {
            alive_pids: alive,
        };

        let recovered = recover_stale_sessions(&mut registry, &checker);

        assert!(
            !recovered.contains(&id),
            "Active session with live PID must not be recovered"
        );
        assert_eq!(
            registry.get(&id).unwrap().current_state,
            SessionState::Active,
            "Active session with live PID must remain Active"
        );
    }

    // --- AC-26: Failed Interactive Retention ---

    #[test]
    fn failed_interactive_uses_loop_brain_duration() {
        let policy = RetentionPolicy::default();

        let duration = policy.duration_for(&SessionType::Interactive, true);

        assert_eq!(
            duration,
            Duration::from_secs(24 * 3600),
            "failed interactive session must use 24h retention (same as loop/brain), not zero"
        );
    }

    #[test]
    fn successful_interactive_uses_zero_duration() {
        let policy = RetentionPolicy::default();

        let duration = policy.duration_for(&SessionType::Interactive, false);

        assert_eq!(
            duration,
            Duration::ZERO,
            "successful interactive session must use zero retention (immediate cleanup)"
        );
    }

    #[test]
    fn loop_session_uses_loop_brain_duration() {
        let policy = RetentionPolicy::default();

        let duration = policy.duration_for(&SessionType::Loop, false);

        assert_eq!(
            duration,
            Duration::from_secs(24 * 3600),
            "loop session must use 24h retention duration"
        );
    }

    // --- CT-89-05: Daemon Startup Wiring — Recovery + Retention ---

    #[test]
    fn is_pid_alive_returns_false_for_nonexistent_pid() {
        assert!(
            !is_pid_alive(u32::MAX),
            "is_pid_alive must return false for a PID that does not exist"
        );
    }

    #[test]
    fn daemon_startup_recovery_marks_dead_pid_sessions_failed() {
        let mut registry = new_registry();
        let id = register_with_pid(&mut registry, SessionState::Active, u32::MAX);

        let checker = LiveProcessChecker;
        let recovered = recover_stale_sessions(&mut registry, &checker);

        assert!(
            recovered.contains(&id),
            "startup recovery with LiveProcessChecker must recover Active session with dead PID"
        );
        assert_eq!(
            registry.get(&id).unwrap().current_state,
            SessionState::Failed,
            "Active session with dead PID must be transitioned to Failed after startup recovery"
        );
    }

    #[test]
    fn run_cycle_removes_expired_retained_sessions() {
        let mut registry = new_registry();
        let record = SessionRecord {
            session_id: SessionId::new(),
            member_name: "test".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: None,
            workspace_path: Some(PathBuf::from("/tmp/workspace")),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry.update_state(&id, SessionState::Active).unwrap();
        registry
            .update_state(&id, SessionState::Completed)
            .unwrap();
        registry
            .update_state(&id, SessionState::Retained)
            .unwrap();

        let policy = RetentionPolicy {
            loop_brain_duration: Duration::ZERO,
            ..RetentionPolicy::default()
        };
        let disk = FakeDiskUsage {
            sizes: HashMap::new(),
        };

        let cleaned = run_cycle(&mut registry, &policy, &disk);

        assert!(
            !cleaned.is_empty(),
            "run_cycle must return IDs of expired retained sessions"
        );
        assert!(
            registry.get(&id).is_none(),
            "expired retained session must be removed from registry after run_cycle"
        );
    }

    #[test]
    fn run_cycle_enforces_disk_budget() {
        let mut registry = new_registry();
        let ws = PathBuf::from("/tmp/big-workspace");
        let record = SessionRecord {
            session_id: SessionId::new(),
            member_name: "test".to_string(),
            session_type: SessionType::Loop,
            current_state: SessionState::Creating,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: None,
            workspace_path: Some(ws.clone()),
            finalization_result: None,
                finalization_agent_pid: None,
        };
        let id = record.session_id.clone();
        registry.register(record).unwrap();
        registry.update_state(&id, SessionState::Active).unwrap();
        registry
            .update_state(&id, SessionState::Completed)
            .unwrap();
        registry
            .update_state(&id, SessionState::Retained)
            .unwrap();

        let policy = RetentionPolicy {
            loop_brain_duration: Duration::from_secs(999_999),
            disk_budget_bytes: Some(1_000_000_000),
            ..RetentionPolicy::default()
        };
        let mut sizes = HashMap::new();
        sizes.insert(ws, 5_000_000_000);
        let disk = FakeDiskUsage { sizes };

        let cleaned = run_cycle(&mut registry, &policy, &disk);

        assert!(
            !cleaned.is_empty(),
            "run_cycle must evict sessions when disk budget is exceeded"
        );
        assert!(
            registry.get(&id).is_none(),
            "over-budget session must be removed from registry after run_cycle"
        );
    }
}
