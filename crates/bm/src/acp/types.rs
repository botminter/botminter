use std::path::PathBuf;

/// Configuration for spawning an ACP agent process.
#[derive(Debug, Clone)]
pub struct AcpConfig {
    /// Path to the ACP agent binary (e.g., "claude-agent-acp").
    pub binary: String,
    /// Working directory for the ACP session.
    pub cwd: PathBuf,
    /// System prompt appended to the session.
    pub system_prompt: Option<String>,
    /// Additional environment variables for the child process.
    pub env_vars: Vec<(String, String)>,
}

/// Events received from the ACP session.
#[derive(Debug, Clone)]
pub enum AcpEvent {
    /// A text chunk from the agent's response.
    Text(String),
    /// The agent's turn completed.
    TurnComplete {
        /// Why the agent stopped (e.g., "end_turn", "max_tokens").
        stop_reason: String,
    },
    /// The agent requested permission for an action.
    PermissionRequest {
        /// Unique ID for this request.
        request_id: String,
        /// Human-readable description of what the agent wants to do.
        description: String,
        /// Available options (typically "allow" / "deny").
        options: Vec<PermissionOption>,
    },
}

/// An option in a permission request.
#[derive(Debug, Clone)]
pub struct PermissionOption {
    pub id: String,
    pub label: String,
}

/// How to handle permission requests from the agent.
#[derive(Debug, Clone, Copy)]
pub enum PermissionHandler {
    /// Auto-approve all permission requests (YOLO mode).
    AutoApprove,
    /// Auto-deny all permission requests.
    AutoDeny,
}

/// Outcome of a permission decision.
#[derive(Debug, Clone)]
pub enum PermissionOutcome {
    /// Selected a specific option by ID.
    Selected(String),
    /// Cancelled the permission request.
    Cancelled,
}

/// Errors from the ACP client.
#[derive(Debug)]
pub enum AcpError {
    /// Failed to spawn the ACP agent process.
    SpawnFailed(String),
    /// The ACP connection was lost.
    ConnectionLost(String),
    /// A JSON-RPC protocol error occurred.
    Protocol(String),
    /// The ACP session failed to initialize.
    InitFailed(String),
    /// The session was not found or already closed.
    SessionNotFound(String),
    /// An internal channel was closed unexpectedly.
    ChannelClosed,
}

impl std::fmt::Display for AcpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            AcpError::SpawnFailed(msg) => write!(f, "failed to spawn ACP process: {msg}"),
            AcpError::ConnectionLost(msg) => write!(f, "ACP connection lost: {msg}"),
            AcpError::Protocol(msg) => write!(f, "ACP protocol error: {msg}"),
            AcpError::InitFailed(msg) => write!(f, "ACP initialization failed: {msg}"),
            AcpError::SessionNotFound(msg) => write!(f, "ACP session not found: {msg}"),
            AcpError::ChannelClosed => write!(f, "ACP internal channel closed"),
        }
    }
}

impl std::error::Error for AcpError {}


#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn acp_config_debug_display() {
        let config = AcpConfig {
            binary: "claude-agent-acp".into(),
            cwd: PathBuf::from("/workspace"),
            system_prompt: Some("You are a helpful assistant.".into()),
            env_vars: vec![("FOO".into(), "bar".into())],
        };
        let debug = format!("{config:?}");
        assert!(debug.contains("claude-agent-acp"));
        assert!(debug.contains("/workspace"));
    }

    #[test]
    fn acp_error_display() {
        let err = AcpError::SpawnFailed("binary not found".into());
        assert_eq!(
            err.to_string(),
            "failed to spawn ACP process: binary not found"
        );

        let err = AcpError::ConnectionLost("pipe broken".into());
        assert_eq!(err.to_string(), "ACP connection lost: pipe broken");

        let err = AcpError::ChannelClosed;
        assert_eq!(err.to_string(), "ACP internal channel closed");
    }

    #[test]
    fn acp_event_variants() {
        let text = AcpEvent::Text("hello".into());
        assert!(matches!(text, AcpEvent::Text(s) if s == "hello"));

        let complete = AcpEvent::TurnComplete {
            stop_reason: "end_turn".into(),
        };
        assert!(
            matches!(complete, AcpEvent::TurnComplete { stop_reason } if stop_reason == "end_turn")
        );

        let perm = AcpEvent::PermissionRequest {
            request_id: "req-1".into(),
            description: "run bash".into(),
            options: vec![
                PermissionOption {
                    id: "allow".into(),
                    label: "Allow".into(),
                },
                PermissionOption {
                    id: "deny".into(),
                    label: "Deny".into(),
                },
            ],
        };
        assert!(matches!(perm, AcpEvent::PermissionRequest { options, .. } if options.len() == 2));
    }

    #[test]
    fn permission_outcome_variants() {
        let selected = PermissionOutcome::Selected("allow".into());
        assert!(matches!(selected, PermissionOutcome::Selected(id) if id == "allow"));

        let cancelled = PermissionOutcome::Cancelled;
        assert!(matches!(cancelled, PermissionOutcome::Cancelled));
    }
}
