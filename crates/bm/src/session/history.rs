use chrono::{DateTime, Utc};
use serde::Serialize;

use super::types::{SessionRecord, SessionState};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub enum ExitStatus {
    Normal,
    Abnormal,
}

impl std::fmt::Display for ExitStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExitStatus::Normal => write!(f, "normal"),
            ExitStatus::Abnormal => write!(f, "abnormal"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SessionHistoryEntry {
    pub session_id: String,
    pub member: String,
    pub session_type: String,
    pub start_time: DateTime<Utc>,
    pub end_time: DateTime<Utc>,
    pub exit_status: ExitStatus,
}

#[derive(Debug, Default)]
pub struct SessionHistoryQuery {
    pub member: Option<String>,
    pub since: Option<DateTime<Utc>>,
}

pub fn query_history(
    records: &[&SessionRecord],
    query: &SessionHistoryQuery,
) -> Vec<SessionHistoryEntry> {
    records
        .iter()
        .filter(|r| r.current_state.is_terminal())
        .filter(|r| {
            query
                .member
                .as_ref()
                .is_none_or(|m| r.member_name == *m)
        })
        .filter(|r| {
            query
                .since
                .is_none_or(|since| r.state_transitioned_at >= since)
        })
        .map(|r| {
            let exit_status = match r.current_state {
                SessionState::Completed => ExitStatus::Normal,
                _ => ExitStatus::Abnormal,
            };
            SessionHistoryEntry {
                session_id: r.session_id.to_string(),
                member: r.member_name.clone(),
                session_type: r.session_type.to_string(),
                start_time: r.created_at,
                end_time: r.state_transitioned_at,
                exit_status,
            }
        })
        .collect()
}

pub fn compute_concurrent_count(records: &[&SessionRecord], member: &str) -> u32 {
    records
        .iter()
        .filter(|r| r.member_name == member && r.current_state == SessionState::Active)
        .count() as u32
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{SessionId, SessionState, SessionType};

    fn make_record_with_state(
        member: &str,
        session_type: SessionType,
        state: SessionState,
    ) -> SessionRecord {
        SessionRecord {
            session_id: SessionId::new(),
            member_name: member.to_string(),
            session_type,
            current_state: state,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
                finalization_agent_pid: None,
        }
    }

    // AC-10: Concurrent session count per member

    #[test]
    fn concurrent_count_counts_active_sessions_for_member() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Active),
            make_record_with_state("alice", SessionType::Brain, SessionState::Active),
            make_record_with_state("alice", SessionType::Interactive, SessionState::Active),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let count = compute_concurrent_count(&refs, "alice");
        assert_eq!(count, 3, "alice has 3 active sessions");
    }

    #[test]
    fn concurrent_count_ignores_other_members() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Active),
            make_record_with_state("alice", SessionType::Brain, SessionState::Active),
            make_record_with_state("bob", SessionType::Loop, SessionState::Active),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let count = compute_concurrent_count(&refs, "alice");
        assert_eq!(
            count, 2,
            "alice has 2 active sessions, bob's should be excluded"
        );
    }

    #[test]
    fn concurrent_count_ignores_non_active_sessions() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Active),
            make_record_with_state("alice", SessionType::Brain, SessionState::Completed),
            make_record_with_state("alice", SessionType::Interactive, SessionState::Failed),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let count = compute_concurrent_count(&refs, "alice");
        assert_eq!(count, 1, "only 1 of alice's sessions is Active");
    }

    #[test]
    fn concurrent_count_returns_zero_for_no_sessions() {
        let records: Vec<SessionRecord> = vec![];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let count = compute_concurrent_count(&refs, "alice");
        assert_eq!(count, 0, "no sessions means 0 concurrent");
    }

    // AC-17: Session history queries

    #[test]
    fn history_returns_terminal_sessions() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Active),
            make_record_with_state("alice", SessionType::Brain, SessionState::Completed),
            make_record_with_state("bob", SessionType::Loop, SessionState::Failed),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let entries = query_history(&refs, &SessionHistoryQuery::default());
        assert_eq!(
            entries.len(),
            2,
            "should return 2 terminal sessions (Completed + Failed)"
        );
    }

    #[test]
    fn history_excludes_active_sessions() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Active),
            make_record_with_state("bob", SessionType::Brain, SessionState::Creating),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let entries = query_history(&refs, &SessionHistoryQuery::default());
        assert!(
            entries.is_empty(),
            "no terminal sessions should mean empty history"
        );
    }

    #[test]
    fn history_entry_shows_normal_exit_for_completed() {
        let records = [make_record_with_state(
            "alice",
            SessionType::Loop,
            SessionState::Completed,
        )];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let entries = query_history(&refs, &SessionHistoryQuery::default());
        assert_eq!(entries.len(), 1, "should return 1 completed session");
        assert_eq!(
            entries[0].exit_status,
            ExitStatus::Normal,
            "Completed sessions should have Normal exit"
        );
    }

    #[test]
    fn history_entry_shows_abnormal_exit_for_failed() {
        let records = [make_record_with_state(
            "alice",
            SessionType::Loop,
            SessionState::Failed,
        )];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let entries = query_history(&refs, &SessionHistoryQuery::default());
        assert_eq!(entries.len(), 1, "should return 1 failed session");
        assert_eq!(
            entries[0].exit_status,
            ExitStatus::Abnormal,
            "Failed sessions should have Abnormal exit"
        );
    }

    #[test]
    fn history_filter_by_member() {
        let records = [
            make_record_with_state("alice", SessionType::Loop, SessionState::Completed),
            make_record_with_state("bob", SessionType::Brain, SessionState::Completed),
        ];
        let refs: Vec<&SessionRecord> = records.iter().collect();
        let query = SessionHistoryQuery {
            member: Some("alice".to_string()),
            ..Default::default()
        };
        let entries = query_history(&refs, &query);
        assert_eq!(entries.len(), 1, "should only return alice's sessions");
        assert_eq!(entries[0].member, "alice");
    }

    #[test]
    fn history_filter_by_time_range() {
        let mut old_record = make_record_with_state(
            "alice",
            SessionType::Loop,
            SessionState::Completed,
        );
        old_record.created_at = Utc::now() - chrono::Duration::hours(48);
        old_record.state_transitioned_at = old_record.created_at;

        let mut recent_record = make_record_with_state(
            "bob",
            SessionType::Brain,
            SessionState::Completed,
        );
        recent_record.created_at = Utc::now() - chrono::Duration::hours(1);
        recent_record.state_transitioned_at = recent_record.created_at;

        let refs: Vec<&SessionRecord> = vec![&old_record, &recent_record];
        let query = SessionHistoryQuery {
            since: Some(Utc::now() - chrono::Duration::hours(24)),
            ..Default::default()
        };
        let entries = query_history(&refs, &query);
        assert_eq!(
            entries.len(),
            1,
            "should only return sessions since 24h ago"
        );
    }
}
