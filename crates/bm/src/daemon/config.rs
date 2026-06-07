use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;

/// Daemon config file stored at `~/.botminter/daemon-<team>.json`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaemonConfig {
    pub team: String,
    pub mode: String,
    pub port: u16,
    pub interval_secs: u64,
    pub pid: u32,
    pub started_at: String,
}

/// Poll state tracking for poll mode.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PollState {
    pub last_event_id: Option<String>,
    pub last_poll_at: Option<String>,
}

/// Encapsulates path resolution for daemon files.
///
/// All daemon files live under `~/.botminter/` and are keyed by team name.
/// This struct avoids passing `team_name` to every path helper.
#[derive(Clone)]
pub struct DaemonPaths {
    team_name: String,
    config_dir: PathBuf,
}

impl DaemonPaths {
    pub fn new(team_name: &str) -> Result<Self> {
        Ok(Self {
            team_name: team_name.to_string(),
            config_dir: config::config_dir()?,
        })
    }

    /// Creates paths with a custom config directory. Used by tests.
    #[cfg(test)]
    pub fn new_with_dir(team_name: &str, dir: &str) -> Self {
        Self {
            team_name: team_name.to_string(),
            config_dir: std::path::PathBuf::from(dir),
        }
    }

    /// PID file path: `~/.botminter/daemon-<team>.pid`
    pub fn pid(&self) -> PathBuf {
        self.config_dir
            .join(format!("daemon-{}.pid", self.team_name))
    }

    /// Config file path: `~/.botminter/daemon-<team>.json`
    pub fn config(&self) -> PathBuf {
        self.config_dir
            .join(format!("daemon-{}.json", self.team_name))
    }

    /// Poll state file path: `~/.botminter/daemon-<team>-poll.json`
    pub fn poll_state(&self) -> PathBuf {
        self.config_dir
            .join(format!("daemon-{}-poll.json", self.team_name))
    }

    /// Logs directory: `~/.botminter/logs/`
    pub fn log_dir(&self) -> Result<PathBuf> {
        let logs_dir = self.config_dir.join("logs");
        fs::create_dir_all(&logs_dir)?;
        Ok(logs_dir)
    }

    /// Log file path: `~/.botminter/logs/daemon-<team>.log`
    ///
    /// Used by lifecycle.rs to redirect stderr for crash diagnostics
    /// before tracing initializes.
    pub fn log(&self) -> Result<PathBuf> {
        let logs_dir = self.config_dir.join("logs");
        fs::create_dir_all(&logs_dir)?;
        Ok(logs_dir.join(format!("daemon-{}.log", self.team_name)))
    }

    /// Sessions registry file path: `~/.botminter/sessions-<team>.json`
    pub fn sessions_registry(&self) -> PathBuf {
        self.config_dir
            .join(format!("sessions-{}.json", self.team_name))
    }

    /// Base directory for ephemeral session workspaces: `~/.botminter/sessions/<team>/`
    ///
    /// Each session workspace is created at `sessions_base/<member>/<session_id>/`.
    pub fn sessions_base(&self) -> PathBuf {
        self.config_dir.join("sessions").join(&self.team_name)
    }

    /// Per-member log file path: `~/.botminter/logs/member-<team>-<member>.log`
    pub fn member_log(&self, member_name: &str) -> Result<PathBuf> {
        let logs_dir = self.config_dir.join("logs");
        fs::create_dir_all(&logs_dir)?;
        Ok(logs_dir.join(format!(
            "member-{}-{}.log",
            self.team_name, member_name
        )))
    }
}

/// Loads poll state from disk. Returns default if file is missing or corrupt.
pub fn load_poll_state(path: &Path) -> PollState {
    if !path.exists() {
        tracing::debug!(path = %path.display(), "Poll state file missing, using default");
        return PollState::default();
    }
    match fs::read_to_string(path) {
        Ok(contents) => {
            let state: PollState = serde_json::from_str(&contents).unwrap_or_default();
            tracing::debug!(
                path = %path.display(),
                last_event_id = state.last_event_id.as_deref().unwrap_or("none"),
                "Poll state loaded"
            );
            state
        }
        Err(_) => {
            tracing::debug!(path = %path.display(), "Poll state file unreadable, using default");
            PollState::default()
        }
    }
}

/// Saves poll state to disk. Silently ignores write errors.
pub fn save_poll_state(path: &Path, state: &PollState) {
    if let Ok(contents) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, contents);
        tracing::trace!(path = %path.display(), "Poll state saved");
    }
}

/// Reads the schema version from the team's botminter.yml.
pub fn read_team_schema(team_repo: &Path) -> Result<String> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        anyhow::bail!(
            "Team repo at {} has no botminter.yml",
            team_repo.display()
        );
    }
    let contents =
        fs::read_to_string(&manifest_path).context("Failed to read team botminter.yml")?;
    let val: serde_yml::Value =
        serde_yml::from_str(&contents).context("Failed to parse team botminter.yml")?;
    let version = val["schema_version"]
        .as_str()
        .unwrap_or("")
        .to_string();
    tracing::debug!(path = %manifest_path.display(), version = %version, "Team schema read");
    Ok(version)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_config_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.json");

        let cfg = DaemonConfig {
            team: "my-team".to_string(),
            mode: "webhook".to_string(),
            port: 8484,
            interval_secs: 60,
            pid: 12345,
            started_at: "2026-02-21T10:00:00Z".to_string(),
        };

        let contents = serde_json::to_string_pretty(&cfg).unwrap();
        fs::write(&path, &contents).unwrap();

        let loaded_str = fs::read_to_string(&path).unwrap();
        let loaded: DaemonConfig = serde_json::from_str(&loaded_str).unwrap();

        assert_eq!(loaded.team, "my-team");
        assert_eq!(loaded.mode, "webhook");
        assert_eq!(loaded.port, 8484);
        assert_eq!(loaded.interval_secs, 60);
        assert_eq!(loaded.pid, 12345);
    }

    #[test]
    fn poll_state_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("poll.json");

        let state = PollState {
            last_event_id: Some("12345678".to_string()),
            last_poll_at: Some("2026-02-21T10:00:00Z".to_string()),
        };

        let contents = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &contents).unwrap();

        let loaded_str = fs::read_to_string(&path).unwrap();
        let loaded: PollState = serde_json::from_str(&loaded_str).unwrap();

        assert_eq!(loaded.last_event_id, Some("12345678".to_string()));
    }

    #[test]
    fn poll_state_default_is_empty() {
        let state = PollState::default();
        assert!(state.last_event_id.is_none());
        assert!(state.last_poll_at.is_none());
    }

    #[test]
    fn poll_state_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("poll-state.json");

        let state = PollState {
            last_event_id: Some("99999".to_string()),
            last_poll_at: Some("2026-02-21T12:00:00Z".to_string()),
        };

        save_poll_state(&path, &state);
        let loaded = load_poll_state(&path);

        assert_eq!(loaded.last_event_id, Some("99999".to_string()));
        assert_eq!(
            loaded.last_poll_at,
            Some("2026-02-21T12:00:00Z".to_string())
        );
    }

    #[test]
    fn poll_state_load_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let state = load_poll_state(&path);
        assert!(state.last_event_id.is_none());
        assert!(state.last_poll_at.is_none());
    }

    #[test]
    fn poll_state_load_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("corrupt.json");
        fs::write(&path, "not valid json!!!").unwrap();

        let state = load_poll_state(&path);
        assert!(state.last_event_id.is_none());
    }

    #[test]
    fn member_log_path_format() {
        // Use DaemonPaths with a known config dir to test path formatting.
        let paths = DaemonPaths {
            team_name: "my-team".to_string(),
            config_dir: std::path::PathBuf::from("/tmp/test-config"),
        };
        let path = paths.member_log("alice").unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "member-my-team-alice.log");
        assert!(path.to_str().unwrap().contains("logs"));
    }
}
