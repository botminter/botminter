use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use anyhow::Result;

use super::SessionId;

/// In-memory lock table preventing concurrent work-item claims across sessions.
pub struct WorkItemLock {
    locks: Arc<Mutex<HashMap<String, SessionId>>>,
}

impl WorkItemLock {
    pub fn new() -> Self {
        Self {
            locks: Arc::new(Mutex::new(HashMap::new())),
        }
    }

    /// Acquire a lock on `work_item_id` for `session_id`.
    /// Returns Ok if the lock was acquired, or an error if another session holds it.
    pub fn acquire(&self, work_item_id: &str, session_id: &SessionId) -> Result<()> {
        let mut map = self.locks.lock().unwrap();
        if let Some(holder) = map.get(work_item_id) {
            if holder != session_id {
                anyhow::bail!(
                    "Work item {work_item_id} already held by session {holder}"
                );
            }
            return Ok(());
        }
        map.insert(work_item_id.to_string(), session_id.clone());
        Ok(())
    }

    /// Release the lock on `work_item_id` held by `session_id`.
    /// No-op if the lock is not held or held by a different session.
    pub fn release(&self, work_item_id: &str, session_id: &SessionId) {
        let mut map = self.locks.lock().unwrap();
        if map.get(work_item_id) == Some(session_id) {
            map.remove(work_item_id);
        }
    }

    /// Release all locks held by `session_id`.
    pub fn release_all(&self, session_id: &SessionId) {
        let mut map = self.locks.lock().unwrap();
        map.retain(|_, holder| holder != session_id);
    }

    /// Return the current holder of the lock on `work_item_id`, if any.
    pub fn holder_of(&self, work_item_id: &str) -> Option<SessionId> {
        let map = self.locks.lock().unwrap();
        map.get(work_item_id).cloned()
    }
}

impl Default for WorkItemLock {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::Arc;
    use std::thread;

    // AC-3: Work-Item Lock — acquire/release semantics

    #[test]
    fn acquire_succeeds_for_unclaimed_item() {
        let lock = WorkItemLock::new();
        let session = SessionId::new();
        lock.acquire("ISSUE-42", &session)
            .expect("acquire must succeed for an unclaimed work item");
    }

    #[test]
    fn acquire_fails_when_another_session_holds_lock() {
        let lock = WorkItemLock::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();

        lock.acquire("ISSUE-42", &s1).unwrap();

        let err = lock
            .acquire("ISSUE-42", &s2)
            .expect_err("second session acquiring same item must fail");
        let msg = err.to_string().to_lowercase();
        assert!(
            msg.contains("held") || msg.contains("lock") || msg.contains("acquired"),
            "error must describe the conflict, got: {msg}"
        );
    }

    #[test]
    fn release_makes_item_available_again() {
        let lock = WorkItemLock::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();

        lock.acquire("ISSUE-42", &s1).unwrap();
        lock.release("ISSUE-42", &s1);

        lock.acquire("ISSUE-42", &s2)
            .expect("acquire must succeed after the previous holder releases");
    }

    #[test]
    fn release_all_clears_all_locks_for_session() {
        let lock = WorkItemLock::new();
        let s1 = SessionId::new();
        let s2 = SessionId::new();

        lock.acquire("ISSUE-1", &s1).unwrap();
        lock.acquire("ISSUE-2", &s1).unwrap();

        lock.release_all(&s1);

        lock.acquire("ISSUE-1", &s2)
            .expect("ISSUE-1 must be available after release_all");
        lock.acquire("ISSUE-2", &s2)
            .expect("ISSUE-2 must be available after release_all");
    }

    #[test]
    fn concurrent_acquire_exactly_one_winner() {
        let lock = Arc::new(WorkItemLock::new());
        let n = 10;

        let handles: Vec<_> = (0..n)
            .map(|_| {
                let lock = Arc::clone(&lock);
                thread::spawn(move || {
                    let session = SessionId::new();
                    lock.acquire("CONTESTED-ITEM", &session)
                })
            })
            .collect();

        let results: Vec<_> = handles.into_iter().map(|h| h.join().unwrap()).collect();
        let winners = results.iter().filter(|r| r.is_ok()).count();
        assert_eq!(
            winners, 1,
            "exactly one thread must win the lock, got {winners} winners"
        );
    }

    #[test]
    fn release_nonexistent_lock_is_noop() {
        let lock = WorkItemLock::new();
        let session = SessionId::new();
        lock.release("NEVER-LOCKED", &session);
    }
}
