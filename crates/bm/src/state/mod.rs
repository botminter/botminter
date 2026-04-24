mod dashboard;

pub use dashboard::{
    gather_status, BridgeDisplay, BridgeIdentityRow, DaemonDisplay, MemberRow,
    RalphMemberInfo, StatusInfo, SubmoduleRow, VerboseDisplay, WorkspaceVerbose,
};

use std::collections::{HashMap, HashSet};
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
    /// Members opted into event-driven restart via `bm enable`.
    /// The daemon's poll/webhook loop only starts members in this set.
    /// Keys use the same format as `members`: "team_name/member_name".
    #[serde(default, skip_serializing_if = "HashSet::is_empty")]
    pub enabled: HashSet<String>,
}

/// Runtime info for a single running member.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemberRuntime {
    pub pid: u32,
    pub started_at: String, // ISO 8601
    pub workspace: PathBuf,
    /// When true, this member runs the brain multiplexer (chat-first mode)
    /// instead of a raw Ralph Orchestrator process.
    #[serde(default)]
    pub brain_mode: bool,
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

/// Checks if a process with the given PID is alive and not a zombie.
///
/// Uses `kill(pid, 0)` for existence check, then reads `/proc/<pid>/stat`
/// to exclude zombie processes. Zombies are dead processes that haven't been
/// waited for — they appear alive to `kill()` but are not actually running.
pub fn is_alive(pid: u32) -> bool {
    // Safety: kill with signal 0 only checks existence, sends no signal.
    if unsafe { libc::kill(pid as i32, 0) } != 0 {
        return false;
    }

    // Check if the process is a zombie by reading /proc/<pid>/stat.
    // The third field is the state character: R=running, S=sleeping,
    // D=disk sleep, Z=zombie, T=stopped, etc.
    if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
        // Format: "pid (comm) state ..." — state is after the closing paren
        if let Some(state_start) = stat.rfind(')') {
            let rest = &stat[state_start + 1..];
            let state_char = rest.trim_start().chars().next().unwrap_or('?');
            if state_char == 'Z' {
                return false; // zombie — not actually alive
            }
        }
    }

    true
}

/// Status of a team member process.
///
/// Enable/disable is NOT a status — it's an orthogonal property stored
/// separately in `RuntimeState.enabled`. A disabled member can be
/// Running, Crashed, or Stopped.
#[derive(Debug)]
pub enum MemberStatus {
    Running {
        pid: u32,
        started_at: String,
        brain_mode: bool,
    },
    Crashed {
        pid: u32,
        started_at: String,
    },
    Stopped,
}

impl MemberStatus {
    pub fn label(&self) -> &'static str {
        match self {
            MemberStatus::Running { brain_mode: true, .. } => "brain",
            MemberStatus::Running { .. } => "running",
            MemberStatus::Crashed { .. } => "crashed",
            MemberStatus::Stopped => "stopped",
        }
    }
}

/// Resolves runtime status for display/inspection by external callers.
pub fn resolve_member_status(
    state: &RuntimeState,
    team_name: &str,
    member_dir_name: &str,
) -> MemberStatus {
    let key = format!("{}/{}", team_name, member_dir_name);
    match state.members.get(&key) {
        Some(rt) => {
            if is_alive(rt.pid) {
                MemberStatus::Running {
                    pid: rt.pid,
                    started_at: rt.started_at.clone(),
                    brain_mode: rt.brain_mode,
                }
            } else {
                MemberStatus::Crashed {
                    pid: rt.pid,
                    started_at: rt.started_at.clone(),
                }
            }
        }
        None => MemberStatus::Stopped,
    }
}

// ── Enable/disable helpers ───────────────────────────────────────

/// Adds a member to the enabled set (eligible for event-driven restart).
pub fn enable_member(state: &mut RuntimeState, key: &str) {
    state.enabled.insert(key.to_string());
}

/// Removes a member from the enabled set.
pub fn disable_member(state: &mut RuntimeState, key: &str) {
    state.enabled.remove(key);
}

/// Returns true if a member is in the enabled set.
pub fn is_enabled(state: &RuntimeState, key: &str) -> bool {
    state.enabled.contains(key)
}

/// Returns enabled member keys for a team.
pub fn enabled_members(state: &RuntimeState, team_name: &str) -> Vec<String> {
    let prefix = format!("{}/", team_name);
    state.enabled.iter()
        .filter(|k| k.starts_with(&prefix))
        .cloned()
        .collect()
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
                brain_mode: false,
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
                brain_mode: false,
            },
        );
        state.members.insert(
            "alive-member".to_string(),
            MemberRuntime {
                pid: std::process::id(), // current process, definitely alive
                started_at: "2026-01-01T00:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/alive"),
                brain_mode: false,
            },
        );

        let stale = cleanup_stale(&mut state);

        assert_eq!(stale.len(), 1);
        assert!(stale.contains(&"dead-member".to_string()));
        assert_eq!(state.members.len(), 1);
        assert!(state.members.contains_key("alive-member"));
    }

    // ── resolve_member_status ─────────────────────────────────────

    #[test]
    fn resolve_member_status_running() {
        let mut state = RuntimeState::default();
        let alive_pid = std::process::id(); // current process, guaranteed alive
        state.members.insert(
            "team/member".to_string(),
            MemberRuntime {
                pid: alive_pid,
                started_at: "2026-02-21T10:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/ws"),
                brain_mode: false,
            },
        );

        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "running");
        match status {
            MemberStatus::Running { pid, .. } => assert_eq!(pid, alive_pid),
            other => panic!("Expected Running, got {:?}", other),
        }
    }

    #[test]
    fn resolve_member_status_crashed() {
        let mut state = RuntimeState::default();
        let dead_pid = 4_000_000u32; // unlikely to exist
        state.members.insert(
            "team/member".to_string(),
            MemberRuntime {
                pid: dead_pid,
                started_at: "2026-02-21T10:00:00Z".to_string(),
                workspace: PathBuf::from("/tmp/ws"),
                brain_mode: false,
            },
        );

        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "crashed");
        match status {
            MemberStatus::Crashed { pid, .. } => assert_eq!(pid, dead_pid),
            other => panic!("Expected Crashed, got {:?}", other),
        }
    }

    #[test]
    fn resolve_member_status_stopped() {
        let state = RuntimeState::default(); // empty — no entries
        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "stopped");
    }

    // ── MemberStatus::label ───────────────────────────────────────

    #[test]
    fn member_status_labels() {
        assert_eq!(
            MemberStatus::Running {
                pid: 1,
                started_at: String::new(),
                brain_mode: false,
            }
            .label(),
            "running"
        );
        assert_eq!(
            MemberStatus::Running {
                pid: 1,
                started_at: String::new(),
                brain_mode: true,
            }
            .label(),
            "brain"
        );
        assert_eq!(
            MemberStatus::Crashed {
                pid: 1,
                started_at: String::new()
            }
            .label(),
            "crashed"
        );
        assert_eq!(MemberStatus::Stopped.label(), "stopped");
    }

    #[test]
    fn brain_mode_backward_compat_deserialization() {
        // Old state.json without brain_mode field should deserialize with brain_mode=false
        let json = r#"{
            "members": {
                "team/member": {
                    "pid": 1234,
                    "started_at": "2026-01-01T00:00:00Z",
                    "workspace": "/tmp/ws"
                }
            }
        }"#;
        let state: RuntimeState = serde_json::from_str(json).unwrap();
        let rt = state.members.get("team/member").unwrap();
        assert!(!rt.brain_mode, "brain_mode should default to false");
    }

    // ── Enable/disable ─────────────────────────────────────────

    #[test]
    fn enabled_backward_compat_deserialization() {
        let json = r#"{ "members": {} }"#;
        let state: RuntimeState = serde_json::from_str(json).unwrap();
        assert!(state.enabled.is_empty());
    }

    #[test]
    fn enabled_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("state.json");

        let mut state = RuntimeState::default();
        enable_member(&mut state, "team/alice");
        enable_member(&mut state, "team/bob");
        save_to(&path, &state).unwrap();

        let loaded = load_from(&path).unwrap();
        assert!(loaded.enabled.contains("team/alice"));
        assert!(loaded.enabled.contains("team/bob"));
        assert_eq!(loaded.enabled.len(), 2);
    }

    #[test]
    fn disable_member_removes_from_set() {
        let mut state = RuntimeState::default();
        enable_member(&mut state, "team/alice");
        assert!(is_enabled(&state, "team/alice"));

        disable_member(&mut state, "team/alice");
        assert!(!is_enabled(&state, "team/alice"));
    }

    #[test]
    fn enabled_members_filters_by_team() {
        let mut state = RuntimeState::default();
        enable_member(&mut state, "team-a/alice");
        enable_member(&mut state, "team-a/bob");
        enable_member(&mut state, "team-b/carol");

        let members = enabled_members(&state, "team-a");
        assert_eq!(members.len(), 2);
        assert!(members.contains(&"team-a/alice".to_string()));
        assert!(members.contains(&"team-a/bob".to_string()));
    }

    #[test]
    fn new_member_not_enabled_by_default() {
        let state = RuntimeState::default();
        assert!(!is_enabled(&state, "team/new-member"));
    }

    #[test]
    fn enabled_not_serialized_when_empty() {
        let state = RuntimeState::default();
        let json = serde_json::to_string(&state).unwrap();
        assert!(!json.contains("enabled"));
    }

    #[test]
    fn enabled_does_not_affect_member_status() {
        let mut state = RuntimeState::default();
        enable_member(&mut state, "team/member");
        let status = resolve_member_status(&state, "team", "member");
        assert_eq!(status.label(), "stopped");
    }
}
