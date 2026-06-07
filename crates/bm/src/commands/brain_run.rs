use std::path::PathBuf;

use anyhow::{Context, Result};
use tracing_subscriber::EnvFilter;

use crate::brain::{
    bridge_adapter::{self, MatrixBridgeConfig, MatrixBridgeReader, MatrixBridgeWriter},
    EventWatcher, EventWatcherConfig, Heartbeat, HeartbeatConfig, Multiplexer, MultiplexerConfig,
};

/// Runs the brain multiplexer event loop.
///
/// This is the internal handler for `bm brain-run`, spawned as a background
/// process by `bm start` for chat-first members. It creates a tokio runtime
/// and runs the multiplexer with event watcher and heartbeat components.
pub fn run(workspace: &str, system_prompt: &str, acp_binary: &str) -> Result<()> {
    // Initialize tracing to stderr so diagnostics appear in brain-stderr.log.
    // Without this, all tracing::info!/error!/warn! calls are no-ops.
    tracing_subscriber::fmt()
        .with_env_filter(
            EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new("info")),
        )
        .with_writer(std::io::stderr)
        .with_ansi(false)
        .init();

    let workspace_path = PathBuf::from(workspace);
    let prompt_content = std::fs::read_to_string(system_prompt)
        .with_context(|| format!("Failed to read brain system prompt at {system_prompt}"))?;

    tracing::info!(
        workspace = %workspace,
        acp_binary = %acp_binary,
        "Brain multiplexer starting"
    );

    let rt = tokio::runtime::Runtime::new().context("Failed to create tokio runtime")?;

    rt.block_on(async {
        run_brain(workspace_path, prompt_content, acp_binary.to_string()).await
    })
}

async fn run_brain(
    workspace: PathBuf,
    system_prompt: String,
    acp_binary: String,
) -> Result<()> {
    let config = MultiplexerConfig {
        acp_binary,
        cwd: workspace.clone(),
        system_prompt: Some(system_prompt),
        env_vars: collect_env_vars(),
    };

    let (mut mux, input, output, shutdown) = Multiplexer::new(config);

    // Get raw senders for event watcher, heartbeat, and bridge reader
    let event_sender = input.sender();
    let heartbeat_sender = input.sender();

    // Spawn bridge adapter (reader + writer) if all env vars are present
    let bridge_config = resolve_bridge_config(&workspace);
    let bridge_reader_shutdown_tx = if let Some(cfg) = bridge_config {
        let mode = if cfg.room_id.is_some() { "locked" } else { "discovery" };
        tracing::info!(
            room_id = ?cfg.room_id,
            own_user_id = %cfg.own_user_id,
            mode = mode,
            "Bridge adapter enabled — spawning reader and writer"
        );

        let bridge_sender = input.sender();

        // Shared active room state between reader and writer
        let shared_room = bridge_adapter::active_room(cfg.room_id.clone());

        // Spawn reader
        let reader = MatrixBridgeReader::new(cfg.clone(), bridge_sender, shared_room.clone());
        let (reader_shutdown_tx, reader_shutdown_rx) = tokio::sync::mpsc::channel(1);
        tokio::spawn(async move {
            reader.run(reader_shutdown_rx).await;
        });

        // Spawn writer
        let writer = MatrixBridgeWriter::new(cfg, shared_room);
        tokio::spawn(async move {
            writer.run(output).await;
        });

        Some(reader_shutdown_tx)
    } else {
        tracing::info!("Bridge adapter disabled (missing env vars), output will be dropped");
        drop(output);
        None
    };

    // Spawn event watcher
    let event_config = EventWatcherConfig {
        workspace_root: workspace.clone(),
        poll_interval: std::time::Duration::from_secs(1),
    };
    let event_watcher = EventWatcher::new(event_config, event_sender);
    let (event_shutdown_tx, event_shutdown_rx) = tokio::sync::mpsc::channel(1);
    let event_handle = tokio::spawn(async move {
        event_watcher.run(event_shutdown_rx).await;
    });

    // Spawn heartbeat
    let heartbeat_config = HeartbeatConfig::default();
    let (heartbeat, heartbeat_shutdown, pending) =
        Heartbeat::new(heartbeat_config, heartbeat_sender);
    mux.set_heartbeat_pending(pending);
    let heartbeat_handle = tokio::spawn(async move {
        if let Err(e) = heartbeat.run().await {
            tracing::error!("Heartbeat error: {e}");
        }
    });

    // Handle SIGTERM for graceful shutdown
    let shutdown_clone = shutdown.clone();
    tokio::spawn(async move {
        match tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate()) {
            Ok(mut sig) => {
                sig.recv().await;
                tracing::info!("Received SIGTERM, shutting down brain");
                shutdown_clone.shutdown().await;
            }
            Err(e) => {
                tracing::error!("Failed to register SIGTERM handler: {e}");
            }
        }
    });

    // Run the multiplexer (blocks until shutdown)
    let result = mux.run().await;

    // Clean up
    if let Some(tx) = bridge_reader_shutdown_tx {
        let _ = tx.send(()).await;
    }
    heartbeat_shutdown.shutdown().await;
    let _ = event_shutdown_tx.send(()).await;
    let _ = event_handle.await;
    let _ = heartbeat_handle.await;

    match result {
        Ok(()) => Ok(()),
        Err(e) => {
            tracing::error!("Brain multiplexer error: {e}");
            anyhow::bail!("Brain multiplexer failed: {e}")
        }
    }
}

/// Attempt to build a `MatrixBridgeConfig` from environment variables.
///
/// Required: `RALPH_MATRIX_HOMESERVER_URL`, `RALPH_MATRIX_ACCESS_TOKEN`, `BM_BRAIN_USER_ID`.
/// Optional: `BM_BRAIN_ROOM_ID` (when absent, enters DM discovery mode),
///           `BM_BRAIN_OPERATOR_USER_ID` (required for DM discovery security).
fn resolve_bridge_config(workspace: &std::path::Path) -> Option<MatrixBridgeConfig> {
    let homeserver_url = std::env::var("RALPH_MATRIX_HOMESERVER_URL").ok()?;
    let access_token = std::env::var("RALPH_MATRIX_ACCESS_TOKEN").ok()?;
    let own_user_id = std::env::var("BM_BRAIN_USER_ID").ok()?;

    // Room ID is optional — when absent, brain enters DM discovery mode.
    // Check workspace dm-room.json first (persisted from previous discovery).
    let room_id = std::env::var("BM_BRAIN_ROOM_ID").ok().or_else(|| {
        let dm_file = workspace.join("dm-room.json");
        std::fs::read_to_string(&dm_file)
            .ok()
            .and_then(|s| serde_json::from_str::<serde_json::Value>(&s).ok())
            .and_then(|v| v.get("room_id")?.as_str().map(String::from))
    });

    let operator_user_id = std::env::var("BM_BRAIN_OPERATOR_USER_ID").ok();

    Some(MatrixBridgeConfig {
        homeserver_url,
        access_token,
        room_id,
        own_user_id,
        operator_user_id,
        workspace: Some(workspace.to_path_buf()),
    })
}

/// Keys that `collect_env_vars` forwards to the ACP child process.
const ENV_VAR_ALLOWLIST: &[&str] = &[
    // Essential system
    "GH_TOKEN",
    "PATH",
    "HOME",
    // Anthropic direct auth
    "ANTHROPIC_API_KEY",
    "ANTHROPIC_AUTH_TOKEN",
    "ANTHROPIC_MODEL",
    "ANTHROPIC_BASE_URL",
    // Vertex AI auth (used by claude CLI and claude-agent-acp)
    "ANTHROPIC_VERTEX_PROJECT_ID",
    "CLAUDE_CODE_USE_VERTEX",
    "CLOUD_ML_REGION",
    // Google Cloud credentials (needed for Vertex AI token exchange)
    "GOOGLE_APPLICATION_CREDENTIALS",
    "GOOGLE_CLOUD_PROJECT",
    "CLOUDSDK_CONFIG",
    "CLOUDSDK_CORE_PROJECT",
    // Bridge adapter config (room + identity for Matrix bridge I/O)
    "BM_BRAIN_ROOM_ID",
    "BM_BRAIN_USER_ID",
    "BM_BRAIN_OPERATOR_USER_ID",
    // Team repo path (for gh commands and board awareness)
    "BM_TEAM_REPO",
    // Durable member-state directory for cross-session persistence (dm-room.json, etc.)
    "BM_MEMBER_STATE_DIR",
];

/// Collect relevant environment variables for the ACP process.
///
/// Includes Anthropic API keys, Vertex AI credentials, Google Cloud auth,
/// and essential system variables needed by the ACP binary.
fn collect_env_vars() -> Vec<(String, String)> {
    collect_env_vars_with(|key| std::env::var(key).ok())
}

/// Testable core: collects env vars using the provided lookup function.
fn collect_env_vars_with<F>(lookup: F) -> Vec<(String, String)>
where
    F: Fn(&str) -> Option<String>,
{
    let mut vars = Vec::new();

    for key in ENV_VAR_ALLOWLIST {
        if let Some(val) = lookup(key) {
            vars.push((key.to_string(), val));
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    /// Helper: build a mock lookup from a set of key-value pairs.
    fn mock_env(pairs: &[(&str, &str)]) -> impl Fn(&str) -> Option<String> {
        let map: HashMap<String, String> = pairs
            .iter()
            .map(|(k, v)| (k.to_string(), v.to_string()))
            .collect();
        move |key: &str| map.get(key).cloned()
    }

    #[test]
    fn collect_env_vars_includes_path() {
        let vars = collect_env_vars_with(mock_env(&[("PATH", "/usr/bin")]));
        assert!(
            vars.iter().any(|(k, _)| k == "PATH"),
            "PATH should be collected"
        );
    }

    #[test]
    fn collect_env_vars_skips_missing() {
        let vars = collect_env_vars_with(mock_env(&[("PATH", "/usr/bin")]));
        assert!(
            !vars.iter().any(|(k, _)| k == "NONEXISTENT_VAR_12345"),
            "Missing vars should not appear"
        );
    }

    #[test]
    fn collect_env_vars_includes_vertex_vars_when_set() {
        let vars = collect_env_vars_with(mock_env(&[
            ("PATH", "/usr/bin"),
            ("ANTHROPIC_VERTEX_PROJECT_ID", "my-project"),
            ("CLAUDE_CODE_USE_VERTEX", "1"),
            ("CLOUD_ML_REGION", "us-east5"),
        ]));
        assert!(vars.iter().any(|(k, _)| k == "PATH"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "ANTHROPIC_VERTEX_PROJECT_ID" && v == "my-project"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "CLAUDE_CODE_USE_VERTEX" && v == "1"));
        assert!(vars
            .iter()
            .any(|(k, v)| k == "CLOUD_ML_REGION" && v == "us-east5"));
    }

    #[test]
    fn collect_env_vars_includes_brain_room_id_when_set() {
        let vars = collect_env_vars_with(mock_env(&[
            ("PATH", "/usr/bin"),
            ("BM_BRAIN_ROOM_ID", "!test-room:localhost"),
        ]));
        assert!(
            vars.iter()
                .any(|(k, v)| k == "BM_BRAIN_ROOM_ID" && v == "!test-room:localhost"),
            "BM_BRAIN_ROOM_ID should be collected when set"
        );
    }

    #[test]
    fn collect_env_vars_includes_brain_user_id_when_set() {
        let vars = collect_env_vars_with(mock_env(&[
            ("PATH", "/usr/bin"),
            ("BM_BRAIN_USER_ID", "@bot:localhost"),
        ]));
        assert!(
            vars.iter()
                .any(|(k, v)| k == "BM_BRAIN_USER_ID" && v == "@bot:localhost"),
            "BM_BRAIN_USER_ID should be collected when set"
        );
    }

    #[test]
    fn collect_env_vars_skips_brain_vars_when_absent() {
        let vars = collect_env_vars_with(mock_env(&[("PATH", "/usr/bin")]));
        assert!(
            !vars.iter().any(|(k, _)| k == "BM_BRAIN_ROOM_ID"),
            "BM_BRAIN_ROOM_ID should not appear when unset"
        );
        assert!(
            !vars.iter().any(|(k, _)| k == "BM_BRAIN_USER_ID"),
            "BM_BRAIN_USER_ID should not appear when unset"
        );
    }

    #[test]
    fn env_var_allowlist_contains_expected_keys() {
        assert!(ENV_VAR_ALLOWLIST.contains(&"PATH"));
        assert!(ENV_VAR_ALLOWLIST.contains(&"GH_TOKEN"));
        assert!(ENV_VAR_ALLOWLIST.contains(&"ANTHROPIC_API_KEY"));
        assert!(ENV_VAR_ALLOWLIST.contains(&"BM_BRAIN_ROOM_ID"));
        assert!(ENV_VAR_ALLOWLIST.contains(&"BM_BRAIN_USER_ID"));
    }
}
