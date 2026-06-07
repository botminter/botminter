use std::fs;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use super::api::{
    HealthResponse, MembersStatusResponse, StopMembersRequest, StopMembersResponse,
};
use super::sessions_api::{
    AcquireLockRequest, AcquireLockResponse, BulkCleanupRequest, BulkCleanupResponse,
    CleanupSessionResponse, InspectSessionResponse, ReleaseLockResponse, RetriggerResponse,
    SessionDetailResponse, SessionHistoryResponse, SessionListResponse, StartSessionRequest,
    StartSessionResponse, StopBulkRequest, StopBulkResponse, StopSessionRequest, StopSessionResponse,
};
use super::config::{DaemonConfig, DaemonPaths};
use crate::state;

/// HTTP client for communicating with a running daemon.
///
/// Created via [`DaemonClient::connect`], which discovers the daemon's
/// address from its config file and verifies the process is alive.
pub struct DaemonClient {
    base_url: String,
    client: reqwest::blocking::Client,
}

impl DaemonClient {
    /// Connects to a running daemon for the given team.
    ///
    /// Reads `~/.botminter/daemon-<team>.json` for the port, verifies the
    /// PID is alive, and returns a client ready to make API calls.
    pub fn connect(team_name: &str) -> Result<Self> {
        let paths = DaemonPaths::new(team_name)?;
        let cfg = load_daemon_config(&paths)?;

        if !state::is_alive(cfg.pid) {
            // Clean up stale files
            let _ = fs::remove_file(paths.pid());
            let _ = fs::remove_file(paths.config());
            bail!(
                "Daemon for team '{}' is not running (stale PID {})",
                team_name,
                cfg.pid
            );
        }

        let base_url = format!("http://127.0.0.1:{}", cfg.port);

        let client = reqwest::blocking::Client::builder()
            .timeout(Duration::from_secs(30))
            .build()
            .context("Failed to build HTTP client")?;

        Ok(Self { base_url, client })
    }

    /// Returns the base URL this client is connected to.
    pub fn base_url(&self) -> &str {
        &self.base_url
    }

    /// POST /api/members/stop — stop team members.
    pub fn stop_members(&self, req: &StopMembersRequest) -> Result<StopMembersResponse> {
        let url = format!("{}/api/members/stop", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for stop: {}", status, body);
        }

        resp.json::<StopMembersResponse>()
            .context("Failed to parse stop response")
    }

    /// GET /api/members — list member status.
    pub fn list_members(&self) -> Result<MembersStatusResponse> {
        let url = format!("{}/api/members", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for members list: {}", status, body);
        }

        resp.json::<MembersStatusResponse>()
            .context("Failed to parse members response")
    }

    /// POST /api/sessions/start — create a new ephemeral session.
    pub fn start_session(&self, req: &StartSessionRequest) -> Result<StartSessionResponse> {
        let url = format!("{}/api/sessions/start", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for start session: {}", status, body);
        }

        resp.json::<StartSessionResponse>()
            .context("Failed to parse start session response")
    }

    /// GET /api/sessions — list active sessions.
    pub fn list_sessions(&self) -> Result<SessionListResponse> {
        let url = format!("{}/api/sessions", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for list sessions: {}", status, body);
        }

        resp.json::<SessionListResponse>()
            .context("Failed to parse list sessions response")
    }

    /// POST /api/sessions/{id}/stop — stop a specific session.
    pub fn stop_session(&self, session_id: &str, force: bool) -> Result<StopSessionResponse> {
        let url = format!("{}/api/sessions/{}/stop", self.base_url, session_id);
        let mut req = self.client.post(&url);

        if force {
            req = req.json(&StopSessionRequest { force });
        }

        let resp = req
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for stop session: {}", status, body);
        }

        resp.json::<StopSessionResponse>()
            .context("Failed to parse stop session response")
    }

    /// POST /api/sessions/stop — bulk stop by member or autonomous mode.
    pub fn stop_sessions_bulk(&self, req: &StopBulkRequest) -> Result<StopBulkResponse> {
        let url = format!("{}/api/sessions/stop", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for bulk stop: {}", status, body);
        }

        resp.json::<StopBulkResponse>()
            .context("Failed to parse bulk stop response")
    }

    /// POST /api/sessions/{id}/finalize — retrigger finalization on a retained session.
    pub fn retrigger_finalization(&self, session_id: &str) -> Result<RetriggerResponse> {
        let url = format!("{}/api/sessions/{}/finalize", self.base_url, session_id);
        let resp = self
            .client
            .post(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!(
                "Daemon returned {} for retrigger finalization: {}",
                status,
                body
            );
        }

        resp.json::<RetriggerResponse>()
            .context("Failed to parse retrigger response")
    }

    /// GET /api/sessions/{id} — get session details.
    pub fn get_session(&self, session_id: &str) -> Result<SessionDetailResponse> {
        let url = format!("{}/api/sessions/{}", self.base_url, session_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for get session: {}", status, body);
        }

        resp.json::<SessionDetailResponse>()
            .context("Failed to parse session detail response")
    }

    /// GET /api/sessions/{id}/inspect — inspect a session with finalization and git state.
    pub fn inspect_session(&self, session_id: &str) -> Result<InspectSessionResponse> {
        let url = format!("{}/api/sessions/{}/inspect", self.base_url, session_id);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for inspect session: {}", status, body);
        }

        resp.json::<InspectSessionResponse>()
            .context("Failed to parse inspect session response")
    }

    /// DELETE /api/sessions/{id} — clean up a single retained session.
    pub fn cleanup_session(&self, session_id: &str) -> Result<CleanupSessionResponse> {
        let url = format!("{}/api/sessions/{}", self.base_url, session_id);
        let resp = self
            .client
            .delete(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for cleanup session: {}", status, body);
        }

        resp.json::<CleanupSessionResponse>()
            .context("Failed to parse cleanup session response")
    }

    /// POST /api/sessions/cleanup — bulk cleanup of retained sessions.
    pub fn bulk_cleanup_sessions(&self, req: &BulkCleanupRequest) -> Result<BulkCleanupResponse> {
        let url = format!("{}/api/sessions/cleanup", self.base_url);
        let resp = self
            .client
            .post(&url)
            .json(req)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for bulk cleanup: {}", status, body);
        }

        resp.json::<BulkCleanupResponse>()
            .context("Failed to parse bulk cleanup response")
    }

    /// GET /api/sessions/history — list session history.
    pub fn list_session_history(&self) -> Result<SessionHistoryResponse> {
        let url = format!("{}/api/sessions/history", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for session history: {}", status, body);
        }

        resp.json::<SessionHistoryResponse>()
            .context("Failed to parse session history response")
    }

    /// POST /api/sessions/{id}/locks — acquire a work-item lock.
    pub fn acquire_lock(&self, session_id: &str, work_item_id: &str) -> Result<AcquireLockResponse> {
        let url = format!("{}/api/sessions/{}/locks", self.base_url, session_id);
        let req_body = AcquireLockRequest { work_item_id: work_item_id.to_string() };
        let resp = self
            .client
            .post(&url)
            .json(&req_body)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for acquire lock: {}", status, body);
        }
        resp.json::<AcquireLockResponse>().context("Failed to parse acquire lock response")
    }

    /// DELETE /api/sessions/{id}/locks/{work_item_id} — release a work-item lock.
    pub fn release_lock(&self, session_id: &str, work_item_id: &str) -> Result<ReleaseLockResponse> {
        let url = format!("{}/api/sessions/{}/locks/{}", self.base_url, session_id, work_item_id);
        let resp = self
            .client
            .delete(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;
        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for release lock: {}", status, body);
        }
        resp.json::<ReleaseLockResponse>().context("Failed to parse release lock response")
    }

    /// GET /api/health — daemon health check.
    pub fn health(&self) -> Result<HealthResponse> {
        let url = format!("{}/api/health", self.base_url);
        let resp = self
            .client
            .get(&url)
            .send()
            .with_context(|| format!("Failed to connect to daemon at {}", url))?;

        let status = resp.status();
        if !status.is_success() {
            let body = resp.text().unwrap_or_default();
            bail!("Daemon returned {} for health: {}", status, body);
        }

        resp.json::<HealthResponse>()
            .context("Failed to parse health response")
    }
}

/// Reads the daemon config file for a team.
fn load_daemon_config(paths: &DaemonPaths) -> Result<DaemonConfig> {
    let cfg_path = paths.config();
    if !cfg_path.exists() {
        anyhow::bail!(
            "Daemon config not found at {}. Is the daemon running?",
            cfg_path.display()
        );
    }
    let contents = fs::read_to_string(&cfg_path)
        .with_context(|| format!("Failed to read daemon config at {}", cfg_path.display()))?;
    serde_json::from_str::<DaemonConfig>(&contents)
        .with_context(|| format!("Failed to parse daemon config at {}", cfg_path.display()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn load_daemon_config_missing_file() {
        let paths = DaemonPaths::new_with_dir("test-team", "/tmp/nonexistent-dir-12345");
        let result = load_daemon_config(&paths);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Daemon config not found")
        );
    }

    #[test]
    fn load_daemon_config_valid_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::new_with_dir("my-team", tmp.path().to_str().unwrap());

        let cfg = DaemonConfig {
            team: "my-team".to_string(),
            mode: "poll".to_string(),
            port: 9090,
            interval_secs: 30,
            pid: 99999,
            started_at: "2026-03-24T10:00:00Z".to_string(),
        };
        let contents = serde_json::to_string_pretty(&cfg).unwrap();
        fs::write(paths.config(), contents).unwrap();

        let loaded = load_daemon_config(&paths).unwrap();
        assert_eq!(loaded.team, "my-team");
        assert_eq!(loaded.port, 9090);
        assert_eq!(loaded.pid, 99999);
    }

    #[test]
    fn load_daemon_config_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let paths = DaemonPaths::new_with_dir("my-team", tmp.path().to_str().unwrap());

        fs::write(paths.config(), "not valid json!!!").unwrap();

        let result = load_daemon_config(&paths);
        assert!(result.is_err());
        assert!(
            result
                .unwrap_err()
                .to_string()
                .contains("Failed to parse daemon config")
        );
    }

    #[test]
    fn connect_no_config_file() {
        // DaemonClient::connect with a team that has no config file
        let result = DaemonClient::connect("nonexistent-team-xyz-12345");
        assert!(result.is_err());
    }

    #[test]
    fn connect_stale_pid() {
        let tmp = tempfile::tempdir().unwrap();
        // Write a config with a PID that definitely doesn't exist
        let cfg = DaemonConfig {
            team: "stale-team".to_string(),
            mode: "poll".to_string(),
            port: 19999,
            interval_secs: 30,
            pid: 4294967, // Very unlikely to be a real PID
            started_at: "2026-03-24T10:00:00Z".to_string(),
        };
        let cfg_path = tmp.path().join("daemon-stale-team.json");
        let pid_path = tmp.path().join("daemon-stale-team.pid");
        fs::write(&cfg_path, serde_json::to_string_pretty(&cfg).unwrap()).unwrap();
        fs::write(&pid_path, "4294967").unwrap();

        // Can't easily test with DaemonClient::connect since it uses DaemonPaths::new
        // which reads from the real config dir. But the load_daemon_config + is_alive
        // logic is tested above. This test verifies the error path shape.
        let paths = DaemonPaths::new_with_dir("stale-team", tmp.path().to_str().unwrap());
        let loaded = load_daemon_config(&paths).unwrap();
        assert!(!state::is_alive(loaded.pid));
    }

    #[test]
    fn client_base_url_format() {
        // Verify the base_url construction logic
        let port: u16 = 8484;
        let base_url = format!("http://127.0.0.1:{}", port);
        assert_eq!(base_url, "http://127.0.0.1:8484");
    }

    #[test]
    fn stop_request_serializes_for_client() {
        let req = StopMembersRequest {
            member: Some("bob".to_string()),
            force: true,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["member"], "bob");
        assert_eq!(parsed["force"], true);
    }

    #[test]
    fn stop_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "stopped": [{"name": "alice", "already_exited": false, "forced": true}],
            "errors": []
        });
        let resp: StopMembersResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.stopped.len(), 1);
        assert!(resp.stopped[0].forced);
    }

    #[test]
    fn members_status_response_deserializes_for_client() {
        let json = serde_json::json!({
            "members": [{
                "name": "alice",
                "status": "running",
                "pid": 1234,
                "workspace": "/tmp/ws/alice",
                "brain_mode": false,
                "started_at": "2026-03-24T10:00:00Z"
            }]
        });
        let resp: MembersStatusResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.members.len(), 1);
        assert_eq!(resp.members[0].name, "alice");
        assert_eq!(resp.members[0].pid, Some(1234));
    }

    #[test]
    fn health_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "version": "0.2.0",
            "team": "my-team",
            "daemon_mode": "poll",
            "member_count": 2,
            "uptime_secs": 300
        });
        let resp: HealthResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.team, "my-team");
        assert_eq!(resp.member_count, 2);
        assert_eq!(resp.uptime_secs, Some(300));
    }

    // ── CT-04: Session Client Tests ──────────────────────────────────

    // AC-1: bm start Creates Session — request/response serde

    #[test]
    fn session_start_request_serializes_for_client() {
        let req = StartSessionRequest {
            member_name: "alice".to_string(),
            session_type: "Interactive".to_string(),
            work_item_id: Some("ISSUE-42".to_string()),
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["member_name"], "alice");
        assert_eq!(parsed["session_type"], "Interactive");
        assert_eq!(parsed["work_item_id"], "ISSUE-42");
    }

    #[test]
    fn session_start_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "session_id": "a1b2c3d4",
            "error": null
        });
        let resp: StartSessionResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.session_id, Some("a1b2c3d4".to_string()));
        assert!(resp.error.is_none());
    }

    // AC-5: bm stop — stop response serde

    #[test]
    fn session_stop_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "error": null
        });
        let resp: StopSessionResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert!(resp.error.is_none());
    }

    // AC-6: bm status — list sessions response serde

    #[test]
    fn session_list_response_deserializes_for_client() {
        let json = serde_json::json!({
            "sessions": [{
                "session_id": "a1b2c3d4",
                "member_name": "alice",
                "session_type": "Interactive",
                "current_state": "Active",
                "started_at": "2026-06-03T10:00:00Z"
            }]
        });
        let resp: SessionListResponse = serde_json::from_value(json).unwrap();
        assert_eq!(resp.sessions.len(), 1);
        assert_eq!(resp.sessions[0].session_id, "a1b2c3d4");
        assert_eq!(resp.sessions[0].member_name, "alice");
    }

    // AC-8: Daemon Not Running Detection — clear error

    #[test]
    fn daemon_not_running_connect_fails_with_clear_error() {
        let result = DaemonClient::connect("nonexistent-team-for-session-tests-xyz");
        let err_msg = match result {
            Err(e) => e.to_string(),
            Ok(_) => panic!("connect must fail when daemon is not running"),
        };
        assert!(
            err_msg.contains("not found") || err_msg.contains("not running") || err_msg.contains("Daemon"),
            "error must indicate daemon is not running, got: {err_msg}"
        );
    }

    // ── CT-89-06: Inspect/Cleanup Client Serde ──────────────────────

    #[test]
    fn inspect_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "session_id": "abc123",
            "member_name": "alice",
            "session_type": "Loop",
            "current_state": "Retained",
            "workspace_path": "/tmp/ws",
            "finalization_results": {
                "exit_status": "Completed",
                "committed_repos": [{"repo_name": "botminter", "branch": "main"}],
                "pushed_branches": ["main"],
                "recovery_branches": [],
                "github_issue_urls": []
            },
            "git_state": {
                "repos": [{"repo_name": "botminter", "current_branch": "main", "uncommitted_files": [], "unpushed_branches": []}]
            },
            "error": null
        });
        let resp: InspectSessionResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.session_id, Some("abc123".to_string()));
        assert_eq!(resp.member_name, Some("alice".to_string()));
        assert!(resp.finalization_results.is_some());
        assert!(resp.git_state.is_some());
    }

    #[test]
    fn cleanup_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "session_id": "abc123",
            "workspace_removed": true,
            "registry_removed": true,
            "error": null
        });
        let resp: CleanupSessionResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert!(resp.workspace_removed);
        assert!(resp.registry_removed);
    }

    #[test]
    fn bulk_cleanup_request_serializes_for_client() {
        let req = BulkCleanupRequest {
            filter: "all".to_string(),
            value: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        let parsed: serde_json::Value = serde_json::from_str(&json).unwrap();
        assert_eq!(parsed["filter"], "all");
    }

    #[test]
    fn bulk_cleanup_response_deserializes_for_client() {
        let json = serde_json::json!({
            "ok": true,
            "cleaned": 3,
            "reports": [
                {"session_id": "s1", "workspace_removed": true, "registry_removed": true},
                {"session_id": "s2", "workspace_removed": false, "registry_removed": true},
                {"session_id": "s3", "workspace_removed": true, "registry_removed": true}
            ],
            "error": null
        });
        let resp: BulkCleanupResponse = serde_json::from_value(json).unwrap();
        assert!(resp.ok);
        assert_eq!(resp.cleaned, 3);
        assert_eq!(resp.reports.len(), 3);
        assert!(resp.reports[0].workspace_removed);
        assert!(!resp.reports[1].workspace_removed);
    }
}
