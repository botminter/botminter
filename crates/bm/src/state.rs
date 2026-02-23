use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

const STATE_FILE: &str = "state.json";

/// Runtime state tracking PIDs of running Ralph processes.
/// Stored at `~/.botminter/state.json`.
#[derive(Debug, Serialize, Deserialize, Default)]
pub struct RuntimeState {
    #[serde(default)]
    pub members: HashMap<String, MemberRuntime>,
}

/// Runtime info for a single running member.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemberRuntime {
    pub pid: u32,
    pub started_at: String, // ISO 8601
    pub workspace: PathBuf,
}

/// Returns the path to state.json.
fn state_path() -> Result<PathBuf> {
    Ok(config::config_dir()?.join(STATE_FILE))
}

/// Loads runtime state from disk. Returns empty state if file is missing.
pub fn load() -> Result<RuntimeState> {
    load_from(&state_path()?)
}

/// Loads runtime state from a specific path.
pub fn load_from(path: &Path) -> Result<RuntimeState> {
    if !path.exists() {
        return Ok(RuntimeState::default());
    }
    let contents = fs::read_to_string(path).context("Failed to read state.json")?;
    let state: RuntimeState =
        serde_json::from_str(&contents).context("Failed to parse state.json")?;
    Ok(state)
}

/// Saves runtime state atomically (write to temp file, then rename).
pub fn save(state: &RuntimeState) -> Result<()> {
    save_to(&state_path()?, state)
}

/// Saves runtime state to a specific path atomically.
pub fn save_to(path: &Path, state: &RuntimeState) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create state dir {}", dir.display()))?;
    }

    let contents = serde_json::to_string_pretty(state).context("Failed to serialize state")?;

    // Atomic write: temp file → rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, contents).context("Failed to write temp state file")?;
    fs::rename(&tmp_path, path).context("Failed to rename temp state file")?;

    Ok(())
}

/// Checks if a process with the given PID is alive using `kill(pid, 0)`.
pub fn is_alive(pid: u32) -> bool {
    // Safety: kill with signal 0 only checks existence, sends no signal.
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Removes entries for dead processes from state. Returns the keys that were cleaned.
pub fn cleanup_stale(state: &mut RuntimeState) -> Vec<String> {
    let stale: Vec<String> = state
        .members
        .iter()
        .filter(|(_, rt)| !is_alive(rt.pid))
        .map(|(key, _)| key.clone())
        .collect();

    for key in &stale {
        state.members.remove(key);
    }

    stale
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");

        let mut state = RuntimeState::default();
        state.members.insert(
            "team/arch-01".to_string(),
            MemberRuntime {
                pid: 12345,
                started_at: "2026-02-20T10:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/ws/arch-01"),
            },
        );

        save_to(&path, &state).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.members.len(), 1);
        let rt = loaded.members.get("team/arch-01").unwrap();
        assert_eq!(rt.pid, 12345);
        assert_eq!(rt.started_at, "2026-02-20T10:00:00Z");
    }

    #[test]
    fn load_missing_returns_empty() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let state = load_from(&path).unwrap();
        assert!(state.members.is_empty());
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");
        let tmp_path = tmp.path().join("state.json.tmp");

        let state = RuntimeState::default();
        save_to(&path, &state).unwrap();

        assert!(path.exists());
        assert!(!tmp_path.exists(), "Temp file should be renamed away");
    }

    #[test]
    fn is_alive_current_process() {
        // Our own PID should be alive
        let pid = std::process::id();
        assert!(is_alive(pid));
    }

    #[test]
    fn is_alive_nonexistent_pid() {
        // PID 0 is the kernel scheduler — kill(0, 0) checks the calling process's group,
        // so use a very high PID unlikely to exist.
        assert!(!is_alive(4_000_000));
    }

    #[test]
    fn cleanup_stale_removes_dead() {
        let mut state = RuntimeState::default();
        state.members.insert(
            "dead-member".to_string(),
            MemberRuntime {
                pid: 4_000_000, // unlikely to exist
                started_at: "2026-01-01T00:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/dead"),
            },
        );
        state.members.insert(
            "alive-member".to_string(),
            MemberRuntime {
                pid: std::process::id(), // current process, definitely alive
                started_at: "2026-01-01T00:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/alive"),
            },
        );

        let stale = cleanup_stale(&mut state);

        assert_eq!(stale.len(), 1);
        assert!(stale.contains(&"dead-member".to_string()));
        assert_eq!(state.members.len(), 1);
        assert!(state.members.contains_key("alive-member"));
    }
}
