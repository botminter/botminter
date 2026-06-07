use std::collections::HashMap;
use std::path::PathBuf;

use anyhow::{anyhow, Result};

use super::types::{FinalizationResult, SessionId, SessionRecord, SessionState};

/// On-disk serialization format for the registry.
#[derive(serde::Serialize, serde::Deserialize)]
struct RegistryFile {
    sessions: Vec<SessionRecord>,
}

/// Persistent registry of sessions backed by an atomic JSON file on disk.
pub struct SessionRegistry {
    path: PathBuf,
    sessions: HashMap<SessionId, SessionRecord>,
}

impl SessionRegistry {
    /// Create a new empty registry that will persist to `path`.
    pub fn new(path: PathBuf) -> Self {
        Self {
            path,
            sessions: HashMap::new(),
        }
    }

    /// Load a registry from disk, returning an empty registry if the file does not exist.
    pub fn load(path: PathBuf) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new(path));
        }
        let contents = std::fs::read_to_string(&path)?;
        let data: RegistryFile = serde_json::from_str(&contents)?;
        let sessions = data
            .sessions
            .into_iter()
            .map(|r| (r.session_id.clone(), r))
            .collect();
        Ok(Self { path, sessions })
    }

    /// Atomically persist the registry to disk (write to temp file, then rename).
    ///
    /// Atomic rename prevents partial writes from corrupting the registry on crash.
    pub fn save(&self) -> Result<()> {
        let data = RegistryFile {
            sessions: self.sessions.values().cloned().collect(),
        };
        let json = serde_json::to_string_pretty(&data)?;

        if let Some(parent) = self.path.parent() {
            std::fs::create_dir_all(parent)?;
        }
        let tmp_path = self.path.with_extension("tmp");
        std::fs::write(&tmp_path, json)?;
        std::fs::rename(&tmp_path, &self.path)?;
        Ok(())
    }

    /// Register a new session. The record's `current_state` must be `Creating`.
    pub fn register(&mut self, record: SessionRecord) -> Result<()> {
        let id = record.session_id.clone();
        if self.sessions.contains_key(&id) {
            return Err(anyhow!("Session {} already exists in registry", id));
        }
        self.sessions.insert(id, record);
        self.save()?;
        Ok(())
    }

    /// Look up a session by ID.
    pub fn get(&self, id: &SessionId) -> Option<&SessionRecord> {
        self.sessions.get(id)
    }

    /// Return all tracked sessions.
    pub fn list(&self) -> Vec<&SessionRecord> {
        self.sessions.values().collect()
    }

    /// Validate and apply a state transition, recording the timestamp.
    pub fn update_state(&mut self, id: &SessionId, new_state: SessionState) -> Result<()> {
        let record = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| anyhow!("Session {} not found", id))?;

        if !record.current_state.can_transition_to(&new_state) {
            return Err(anyhow!(
                "Cannot transition from {} to {}",
                record.current_state,
                new_state
            ));
        }

        record.current_state = new_state;
        record.state_transitioned_at = chrono::Utc::now();
        self.save()?;
        Ok(())
    }

    /// Update the agent PID on an existing session record.
    pub fn set_agent_pid(&mut self, id: &SessionId, pid: u32) -> Result<()> {
        let record = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| anyhow!("Session {} not found", id))?;
        record.agent_pid = Some(pid);
        self.save()?;
        Ok(())
    }

    /// Track the PID of the finalization subagent process for a session.
    ///
    /// Called from the deactivation watcher immediately after spawning the finalization
    /// agent. Enables force-stop to SIGKILL the agent before its timeout fires.
    pub fn set_finalization_agent_pid(&mut self, id: &SessionId, pid: u32) -> Result<()> {
        let record = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| anyhow!("Session {} not found", id))?;
        record.finalization_agent_pid = Some(pid);
        self.save()?;
        Ok(())
    }

    /// Record the outcome of the finalization subagent for a session.
    pub fn set_finalization_result(
        &mut self,
        id: &SessionId,
        result: FinalizationResult,
    ) -> Result<()> {
        let record = self
            .sessions
            .get_mut(id)
            .ok_or_else(|| anyhow!("Session {} not found", id))?;
        record.finalization_result = Some(result);
        self.save()?;
        Ok(())
    }

    /// Remove a session record from the registry.
    pub fn remove(&mut self, id: &SessionId) -> Result<()> {
        self.sessions
            .remove(id)
            .ok_or_else(|| anyhow!("Session {} not found", id))?;
        self.save()?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::session::types::{SessionState, SessionType};
    use chrono::Utc;
    use std::thread;

    fn make_record(member: &str, session_type: SessionType) -> SessionRecord {
        SessionRecord {
            session_id: SessionId::new(),
            member_name: member.to_string(),
            session_type,
            current_state: SessionState::Creating,
            created_at: Utc::now(),
            state_transitioned_at: Utc::now(),
            agent_pid: None,
            workspace_path: None,
            finalization_result: None,
            finalization_agent_pid: None,
        }
    }

    fn new_registry() -> SessionRegistry {
        let tmp = tempfile::tempdir().unwrap();
        SessionRegistry::new(tmp.path().join("registry.json"))
    }

    // AC-2: Valid State Transition — Creating → Active succeeds with timestamp recorded
    #[test]
    fn update_state_to_active_records_transition_timestamp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");
        let mut reg = SessionRegistry::new(path);

        let before = Utc::now();
        let record = make_record("alice", SessionType::Interactive);
        let id = record.session_id.clone();
        reg.register(record).unwrap();

        reg.update_state(&id, SessionState::Active).unwrap();

        let updated = reg.get(&id).expect("session must exist after update_state");
        assert_eq!(updated.current_state, SessionState::Active);
        assert!(
            updated.state_transitioned_at >= before,
            "state_transitioned_at must be updated on transition"
        );
    }

    // AC-3: Invalid State Transitions Rejected — registry-level error messages
    #[test]
    fn update_state_rejects_invalid_transition_with_descriptive_error() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");
        let mut reg = SessionRegistry::new(path);

        let record = make_record("alice", SessionType::Interactive);
        let id = record.session_id.clone();
        reg.register(record).unwrap();

        reg.update_state(&id, SessionState::Active).unwrap();
        reg.update_state(&id, SessionState::Completed).unwrap();

        let err = reg
            .update_state(&id, SessionState::Active)
            .expect_err("Completed -> Active must be rejected");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("transition") || msg.contains("completed"),
            "error must describe the invalid transition, got: {msg}"
        );
    }

    // AC-3 (extended): All Active terminal paths are valid
    #[test]
    fn active_can_transition_to_failed_and_killed() {
        for target in [SessionState::Failed, SessionState::Killed] {
            let mut reg = new_registry();
            let record = make_record("bob", SessionType::Loop);
            let id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            reg.update_state(&id, target.clone())
                .unwrap_or_else(|e| panic!("Active -> {target} failed: {e}"));
            assert_eq!(reg.get(&id).unwrap().current_state, target);
        }
    }

    // AC-3 (extended): Finalizing can resolve to Completed, Failed, or Killed
    #[test]
    fn finalizing_transitions_to_all_terminal_states() {
        for target in [
            SessionState::Completed,
            SessionState::Failed,
            SessionState::Killed,
        ] {
            let mut reg = new_registry();
            let record = make_record("carol", SessionType::Brain);
            let id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            reg.update_state(&id, SessionState::Finalizing).unwrap();
            reg.update_state(&id, target.clone())
                .unwrap_or_else(|e| panic!("Finalizing -> {target} failed: {e}"));
            assert_eq!(reg.get(&id).unwrap().current_state, target);
        }
    }

    // AC-3 (extended): All three terminal states can transition to Retained
    #[test]
    fn terminal_states_can_transition_to_retained() {
        for terminal in [
            SessionState::Completed,
            SessionState::Failed,
            SessionState::Killed,
        ] {
            let mut reg = new_registry();
            let record = make_record("dan", SessionType::Interactive);
            let id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            reg.update_state(&id, terminal.clone()).unwrap();
            reg.update_state(&id, SessionState::Retained)
                .unwrap_or_else(|e| panic!("{terminal} -> Retained failed: {e}"));
            assert_eq!(reg.get(&id).unwrap().current_state, SessionState::Retained);
        }
    }

    // CRUD: remove() works correctly
    #[test]
    fn remove_session_succeeds_and_makes_get_return_none() {
        let mut reg = new_registry();
        let record = make_record("eve", SessionType::Interactive);
        let id = record.session_id.clone();
        reg.register(record).unwrap();

        assert!(reg.get(&id).is_some(), "session must exist before remove");
        reg.remove(&id)
            .expect("remove must succeed for existing session");
        assert!(reg.get(&id).is_none(), "session must be gone after remove");
    }

    #[test]
    fn remove_nonexistent_session_returns_error() {
        let mut reg = new_registry();
        let phantom_id = SessionId::new();
        let err = reg
            .remove(&phantom_id)
            .expect_err("remove must fail for unknown session");
        assert!(
            err.to_string().to_lowercase().contains("not found"),
            "error must say 'not found', got: {err}"
        );
    }

    // AC-4: Persistence Survives Restart
    #[test]
    fn registry_persists_sessions_across_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");

        let id;
        {
            let mut reg = SessionRegistry::new(path.clone());
            let record = make_record("bob", SessionType::Loop);
            id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            reg.save().unwrap();
        }

        let reg2 = SessionRegistry::load(path).unwrap();
        let reloaded = reg2
            .get(&id)
            .expect("session must survive a registry reload");
        assert_eq!(reloaded.current_state, SessionState::Active);
        assert_eq!(reloaded.member_name, "bob");
        assert_eq!(reloaded.session_type, SessionType::Loop);
        assert!(
            reloaded.state_transitioned_at >= reloaded.created_at,
            "state_transitioned_at ({:?}) must be >= created_at ({:?}) after reload",
            reloaded.state_transitioned_at,
            reloaded.created_at
        );
    }

    // AC-5: Concurrent Read Safety
    #[test]
    fn concurrent_reads_return_consistent_data() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");

        let mut reg = SessionRegistry::new(path.clone());
        for i in 0..5 {
            let record = make_record(&format!("member-{i}"), SessionType::Loop);
            reg.register(record).unwrap();
        }
        reg.save().unwrap();

        let p1 = path.clone();
        let p2 = path.clone();

        let t1 = thread::spawn(move || SessionRegistry::load(p1).unwrap().list().len());
        let t2 = thread::spawn(move || SessionRegistry::load(p2).unwrap().list().len());

        let count1 = t1.join().unwrap();
        let count2 = t2.join().unwrap();

        assert_eq!(
            count1, 5,
            "first concurrent read returned {count1}, expected 5"
        );
        assert_eq!(
            count2, 5,
            "second concurrent read returned {count2}, expected 5"
        );
    }

    // AC-4 (extended): Mutations auto-persist — no explicit save() required
    #[test]
    fn mutations_auto_persist_without_explicit_save() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");

        let id;
        {
            let mut reg = SessionRegistry::new(path.clone());
            let record = make_record("auto-save", SessionType::Interactive);
            id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            // No explicit save() — mutations must auto-persist
        }

        let reg2 = SessionRegistry::load(path).unwrap();
        let reloaded = reg2
            .get(&id)
            .expect("session must survive reload without explicit save()");
        assert_eq!(reloaded.current_state, SessionState::Active);
        assert_eq!(reloaded.member_name, "auto-save");
    }

    // AC-4 (extended): remove() auto-persists — removed session stays gone after reload
    #[test]
    fn remove_auto_persists_across_reload() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("registry.json");

        let id;
        {
            let mut reg = SessionRegistry::new(path.clone());
            let record = make_record("removable", SessionType::Loop);
            id = record.session_id.clone();
            reg.register(record).unwrap();
            reg.update_state(&id, SessionState::Active).unwrap();
            reg.remove(&id).unwrap();
        }

        let reg2 = SessionRegistry::load(path).unwrap();
        assert!(
            reg2.get(&id).is_none(),
            "removed session must stay gone after reload"
        );
    }

    // AC-6: Unified State Machine — all session types follow the same lifecycle
    #[test]
    fn all_session_types_follow_same_state_machine() {
        for session_type in [
            SessionType::Interactive,
            SessionType::Loop,
            SessionType::Brain,
        ] {
            let mut reg = new_registry();

            let record = make_record("member", session_type.clone());
            let id = record.session_id.clone();
            reg.register(record).unwrap();

            reg.update_state(&id, SessionState::Active)
                .unwrap_or_else(|e| panic!("{session_type:?}: Creating -> Active failed: {e}"));

            reg.update_state(&id, SessionState::Completed)
                .unwrap_or_else(|e| panic!("{session_type:?}: Active -> Completed failed: {e}"));
        }
    }
}
