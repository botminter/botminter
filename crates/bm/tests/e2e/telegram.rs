//! Telegram mock helpers — TgMock lifecycle via podman, control API wrappers.
//!
//! Uses `ghcr.io/watzon/tg-mock` as a drop-in Telegram Bot API mock.
//!
//! # Actual control API surface (discovered via testing)
//!
//! - `POST /__control/updates` — inject an update (global, not per-token)
//! - `GET /__control/updates` — list pending injected updates
//! - `DELETE /__control/updates` — clear all pending updates
//!
//! The per-token injection (`/__control/tokens/<TOKEN>/updates`) and
//! request inspector (`/__control/requests`) endpoints described in the
//! research doc are not available in the current image version.

use std::net::TcpListener;
use std::process::Command;
use std::time::Duration;

use super::helpers::wait_for_port;

const TG_MOCK_IMAGE: &str = "ghcr.io/watzon/tg-mock:latest";

/// A running tg-mock container managed via podman with RAII cleanup.
pub struct TgMock {
    container_id: String,
    port: u16,
}

impl TgMock {
    /// Starts a tg-mock container on a random free port.
    ///
    /// Blocks until the mock's HTTP server is accepting connections.
    pub fn start() -> Self {
        let port = find_free_port();
        let container_name = format!("bm-tg-mock-{}", port);

        let output = Command::new("podman")
            .args([
                "run",
                "-d",
                "--name",
                &container_name,
                "-p",
                &format!("{}:8081", port),
                TG_MOCK_IMAGE,
                "--faker-seed",
                "42",
                "--verbose",
            ])
            .output()
            .expect("failed to start tg-mock container");

        assert!(
            output.status.success(),
            "podman run tg-mock failed: {}",
            String::from_utf8_lossy(&output.stderr)
        );

        let container_id = String::from_utf8_lossy(&output.stdout)
            .trim()
            .to_string();

        eprintln!(
            "TgMock started: container={} port={}",
            &container_id[..12.min(container_id.len())],
            port
        );

        // Wait for the HTTP server to accept connections
        wait_for_port(port, Duration::from_secs(15));

        TgMock { container_id, port }
    }

    /// Returns the base URL for tg-mock's API.
    pub fn api_url(&self) -> String {
        format!("http://localhost:{}", self.port)
    }

    /// Injects a fake user message into the mock via the global control API.
    ///
    /// The injected update is available to any bot via `getUpdates`.
    /// The `token` parameter is accepted for API compatibility with the spec
    /// but is not used — tg-mock only supports global injection.
    pub fn inject_message(&self, _token: &str, text: &str, chat_id: i64) {
        let url = format!("{}/__control/updates", self.api_url());
        let body = serde_json::json!({
            "message": {
                "message_id": 1,
                "text": text,
                "chat": {"id": chat_id, "type": "private"},
                "from": {"id": chat_id, "is_bot": false, "first_name": "TestUser"}
            }
        });

        let client = reqwest::blocking::Client::new();
        let resp = client
            .post(&url)
            .json(&body)
            .send()
            .expect("failed to inject message into tg-mock");

        assert!(
            resp.status().is_success(),
            "inject_message failed with status {}: {}",
            resp.status(),
            resp.text().unwrap_or_default()
        );
    }

    /// Queries pending injected updates from the mock's control API.
    ///
    /// Returns the `updates` array from `GET /__control/updates`.
    /// The `token` and `method` parameters are accepted for API compatibility
    /// with the spec but are not used — tg-mock's `/__control/requests`
    /// endpoint is not available in the current version.
    pub fn get_requests(&self, _token: &str, _method: &str) -> Vec<serde_json::Value> {
        let url = format!("{}/__control/updates", self.api_url());

        let client = reqwest::blocking::Client::new();
        let resp = client
            .get(&url)
            .send()
            .expect("failed to query tg-mock updates");

        assert!(
            resp.status().is_success(),
            "get_requests failed with status {}",
            resp.status()
        );

        // Response format: {"pending": N, "updates": [...]}
        let value: serde_json::Value = resp.json().unwrap_or_default();
        value
            .get("updates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default()
    }
}

impl Drop for TgMock {
    fn drop(&mut self) {
        eprintln!(
            "TgMock dropping: container={}",
            &self.container_id[..12.min(self.container_id.len())]
        );
        // Stop the container (ignore errors — may already be stopped)
        let _ = Command::new("podman")
            .args(["stop", "-t", "2", &self.container_id])
            .output();
        // Remove the container
        let _ = Command::new("podman")
            .args(["rm", "-f", &self.container_id])
            .output();
    }
}

/// Returns `true` if the `podman` CLI is available.
pub fn podman_available() -> bool {
    Command::new("podman")
        .args(["version"])
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Finds a free TCP port by binding to port 0 and reading the assigned port.
fn find_free_port() -> u16 {
    TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind to a free port")
        .local_addr()
        .expect("failed to get local address")
        .port()
}
