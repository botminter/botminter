use std::path::Path;
use std::sync::Arc;

use sacp::schema::{
    ContentBlock, ContentChunk, InitializeRequest, NewSessionRequest, PromptRequest,
    ProtocolVersion, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, SessionUpdate,
    TextContent,
};
use sacp::role::acp::{Agent, Client};
use sacp::ConnectionTo;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot, Mutex};
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

use super::types::{AcpConfig, AcpError, AcpEvent, PermissionHandler};

/// Internal commands sent to the ACP connection task.
#[allow(dead_code)]
enum AcpCommand {
    CreateSession {
        cwd: std::path::PathBuf,
        system_prompt: Option<String>,
        reply: oneshot::Sender<Result<String, AcpError>>,
    },
    Prompt {
        session_id: String,
        text: String,
        reply: oneshot::Sender<Result<(), AcpError>>,
    },
    Cancel {
        session_id: String,
    },
    Shutdown,
}

/// A client for communicating with an ACP agent process.
///
/// Spawns the agent binary as a child process and manages the JSON-RPC
/// connection over stdio. Provides a channel-based API for sending prompts
/// and receiving streaming events.
///
/// # Architecture
///
/// ```text
/// AcpClient (public API)
///     |-- command_tx ---> [ACP connection task] ---> agent process (stdio)
///     |-- event_rx   <--- [ACP connection task] <--- agent process (stdio)
/// ```
///
/// The connection task runs `sacp::ClientToAgent` internally. Commands flow
/// in via `command_tx`, events flow out via `event_rx`.
pub struct AcpClient {
    command_tx: mpsc::Sender<AcpCommand>,
    event_rx: Mutex<mpsc::Receiver<AcpEvent>>,
    child: Arc<Mutex<Child>>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl AcpClient {
    /// Spawn an ACP agent process and establish a connection.
    ///
    /// This performs the full startup sequence:
    /// 1. Spawn the binary as a child process with piped stdio
    /// 2. Create a `sacp::ByteStreams` transport over the pipes
    /// 3. Send the `initialize` handshake
    /// 4. Return a client ready to create sessions and send prompts
    pub async fn spawn(config: AcpConfig) -> Result<Self, AcpError> {
        let permission_handler = PermissionHandler::AutoApprove;
        Self::spawn_with_permissions(config, permission_handler).await
    }

    /// Spawn with a specific permission handler.
    pub async fn spawn_with_permissions(
        config: AcpConfig,
        permission_handler: PermissionHandler,
    ) -> Result<Self, AcpError> {
        // Spawn the agent binary
        let mut cmd = Command::new(&config.binary);
        cmd.stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::piped())
            .stderr(std::process::Stdio::piped());

        for (key, value) in &config.env_vars {
            cmd.env(key, value);
        }

        let mut child = cmd.spawn().map_err(|e| {
            AcpError::SpawnFailed(format!("{}: {e}", config.binary))
        })?;

        let child_stdin = child
            .stdin
            .take()
            .ok_or_else(|| AcpError::SpawnFailed("failed to open agent stdin".into()))?;
        let child_stdout = child
            .stdout
            .take()
            .ok_or_else(|| AcpError::SpawnFailed("failed to open agent stdout".into()))?;

        // Drain stderr in a background task to prevent pipe buffer deadlock.
        // The ACP agent writes tracing logs to stderr. If the pipe buffer fills
        // (~64KB on Linux) and nobody reads, the agent blocks on write and stops
        // processing prompts entirely.
        if let Some(stderr) = child.stderr.take() {
            tokio::spawn(async move {
                use tokio::io::AsyncReadExt;
                let mut stderr = stderr;
                let mut buf = vec![0u8; 8192];
                loop {
                    match stderr.read(&mut buf).await {
                        Ok(0) => break, // EOF — child exited
                        Ok(_) => {}     // Discard output
                        Err(_) => break,
                    }
                }
            });
        }

        // Channels for command/event communication
        let (command_tx, command_rx) = mpsc::channel::<AcpCommand>(32);
        let (event_tx, event_rx) = mpsc::channel::<AcpEvent>(256);

        let child = Arc::new(Mutex::new(child));

        // Spawn the connection task
        let task = tokio::spawn(Self::connection_task(
            child_stdin,
            child_stdout,
            command_rx,
            event_tx,
            permission_handler,
        ));

        Ok(AcpClient {
            command_tx,
            event_rx: Mutex::new(event_rx),
            child,
            task: Mutex::new(Some(task)),
        })
    }

    /// Create a new session on the ACP agent.
    ///
    /// Returns the session ID which must be passed to `prompt()` and `cancel()`.
    pub async fn create_session(
        &self,
        cwd: &Path,
        system_prompt: Option<&str>,
    ) -> Result<String, AcpError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(AcpCommand::CreateSession {
                cwd: cwd.to_path_buf(),
                system_prompt: system_prompt.map(String::from),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AcpError::ChannelClosed)?;

        reply_rx.await.map_err(|_| AcpError::ChannelClosed)?
    }

    /// Send a prompt to the given session.
    ///
    /// Streaming response events will arrive via `recv_event()`. The prompt
    /// completes when an `AcpEvent::TurnComplete` event is received.
    pub async fn prompt(&self, session_id: &str, text: &str) -> Result<(), AcpError> {
        let (reply_tx, reply_rx) = oneshot::channel();
        self.command_tx
            .send(AcpCommand::Prompt {
                session_id: session_id.to_string(),
                text: text.to_string(),
                reply: reply_tx,
            })
            .await
            .map_err(|_| AcpError::ChannelClosed)?;

        reply_rx.await.map_err(|_| AcpError::ChannelClosed)?
    }

    /// Cancel an in-progress prompt on the given session.
    pub async fn cancel(&self, session_id: &str) -> Result<(), AcpError> {
        self.command_tx
            .send(AcpCommand::Cancel {
                session_id: session_id.to_string(),
            })
            .await
            .map_err(|_| AcpError::ChannelClosed)
    }

    /// Receive the next event from the ACP session.
    ///
    /// Returns `None` when the connection is closed.
    pub async fn recv_event(&self) -> Option<AcpEvent> {
        self.event_rx.lock().await.recv().await
    }

    /// Shut down the ACP client.
    ///
    /// Kills the child process first (which closes the transport and unblocks any
    /// in-flight network I/O inside the connection task), then waits for the task to finish.
    /// Sending a Shutdown command afterward is harmless but ensures a clean exit.
    pub async fn shutdown(self) -> Result<(), AcpError> {
        // Kill the child process first so the connection task's transport is torn
        // down immediately. The task may be blocked on cx.spawn()'d prompt I/O;
        // killing the child closes stdin/stdout and lets connect_with return fast.
        let mut child = self.child.lock().await;
        let _ = child.kill().await;
        drop(child);

        // Signal shutdown so the task breaks out of the command loop if it's
        // already past the transport-wait (or if kill raced).
        let _ = self.command_tx.send(AcpCommand::Shutdown).await;

        // Wait for the connection task (near-instant since child is now dead).
        if let Some(task) = self.task.lock().await.take() {
            let _ = task.await;
        }

        Ok(())
    }

    /// The background task that manages the sacp connection.
    async fn connection_task(
        child_stdin: tokio::process::ChildStdin,
        child_stdout: tokio::process::ChildStdout,
        mut command_rx: mpsc::Receiver<AcpCommand>,
        event_tx: mpsc::Sender<AcpEvent>,
        permission_handler: PermissionHandler,
    ) {
        let transport =
            sacp::ByteStreams::new(child_stdin.compat_write(), child_stdout.compat());

        let event_tx_for_notif = event_tx.clone();

        let result = Client.builder()
            .name("botminter")
            .on_receive_notification(
                async move |notification: SessionNotification, _cx: ConnectionTo<Agent>| {
                    let event = match notification.update {
                        SessionUpdate::AgentMessageChunk(ContentChunk {
                            content: ContentBlock::Text(text),
                            ..
                        }) => Some(AcpEvent::Text(text.text)),
                        _ => None,
                    };
                    if let Some(event) = event {
                        let _ = event_tx_for_notif.send(event).await;
                    }
                    Ok(())
                },
                sacp::on_receive_notification!(),
            )
            .on_receive_request(
                async move |request: RequestPermissionRequest,
                            request_cx,
                            _cx: ConnectionTo<Agent>| {
                    let response = match permission_handler {
                        PermissionHandler::AutoApprove => {
                            let option_id =
                                request.options.first().map(|opt| opt.option_id.clone());
                            match option_id {
                                Some(id) => RequestPermissionResponse::new(
                                    RequestPermissionOutcome::Selected(
                                        SelectedPermissionOutcome::new(id),
                                    ),
                                ),
                                None => RequestPermissionResponse::new(
                                    RequestPermissionOutcome::Cancelled,
                                ),
                            }
                        }
                        PermissionHandler::AutoDeny => RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ),
                    };
                    request_cx.respond(response)
                },
                sacp::on_receive_request!(),
            )
            .connect_with(transport, async |cx: ConnectionTo<Agent>| {
                // Step 1: Initialize handshake
                let _init_response = cx
                    .send_request(InitializeRequest::new(ProtocolVersion::LATEST))
                    .block_task()
                    .await?;

                tracing::debug!("ACP connection initialized");

                // Step 2: Process commands from the outer API
                while let Some(cmd) = command_rx.recv().await {
                    match cmd {
                        AcpCommand::CreateSession {
                            cwd,
                            system_prompt,
                            reply,
                        } => {
                            let mut request = NewSessionRequest::new(&cwd);
                            if let Some(prompt) = system_prompt {
                                // Set system prompt via meta field
                                let meta = serde_json::json!({
                                    "systemPrompt": {
                                        "append": prompt
                                    }
                                });
                                request.meta = Some(
                                    meta.as_object()
                                        .cloned()
                                        .unwrap_or_default(),
                                );
                            }

                            let result = cx
                                .send_request(request)
                                .block_task()
                                .await;

                            let reply_result = match result {
                                Ok(response) => {
                                    let session_id = response.session_id.to_string();
                                    tracing::debug!(session_id = %session_id, "ACP session created");
                                    Ok(session_id)
                                }
                                Err(e) => Err(AcpError::InitFailed(e.to_string())),
                            };
                            let _ = reply.send(reply_result);
                        }

                        AcpCommand::Prompt {
                            session_id,
                            text,
                            reply,
                        } => {
                            let session_id_parsed: sacp::schema::SessionId = session_id.into();
                            let request = PromptRequest::new(
                                session_id_parsed,
                                vec![ContentBlock::Text(TextContent::new(&text))],
                            );

                            // Reply immediately so the multiplexer's select loop
                            // stays responsive (can process events, shutdown, etc.)
                            let _ = reply.send(Ok(()));

                            // Spawn prompt processing as a concurrent task.
                            // block_task() is safe inside cx.spawn() per sacp docs.
                            // TurnComplete arrives via event_tx when the LLM responds.
                            let event_tx = event_tx.clone();
                            let cx_for_prompt = cx.clone();
                            cx.spawn(async move {
                                tracing::info!("ACP prompt task: sending request");
                                let result = cx_for_prompt
                                    .send_request(request)
                                    .block_task()
                                    .await;

                                match result {
                                    Ok(response) => {
                                        let stop_reason =
                                            format!("{:?}", response.stop_reason);
                                        tracing::info!(stop_reason = %stop_reason, "ACP prompt completed");
                                        let _ = event_tx
                                            .send(AcpEvent::TurnComplete { stop_reason })
                                            .await;
                                    }
                                    Err(e) => {
                                        tracing::error!(error = %e, "ACP prompt failed");
                                        let _ = event_tx
                                            .send(AcpEvent::TurnComplete {
                                                stop_reason: format!("error: {e}"),
                                            })
                                            .await;
                                    }
                                }
                                Ok(())
                            })?;
                        }

                        AcpCommand::Cancel { session_id: _session_id } => {
                            // TODO: ACP cancel is sent via session/cancel request.
                            // sacp doesn't expose a direct cancel API yet;
                            // for now this is a no-op placeholder.
                            tracing::warn!("ACP cancel not yet implemented in sacp client SDK");
                        }

                        AcpCommand::Shutdown => {
                            tracing::debug!("ACP client shutting down");
                            break;
                        }
                    }
                }

                Ok(())
            })
            .await;

        if let Err(e) = result {
            tracing::error!("ACP connection task ended with error: {e}");
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use sacp::schema::{
        ContentBlock, InitializeRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
        SessionId, StopReason, TextContent,
    };

    #[test]
    fn initialize_request_serialization() {
        let request = InitializeRequest::new(ProtocolVersion::LATEST);
        let json = serde_json::to_value(&request).unwrap();
        assert!(json.get("protocolVersion").is_some());
    }

    #[test]
    fn new_session_request_serialization() {
        let request = NewSessionRequest::new("/workspace");
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["cwd"], "/workspace");
    }

    #[test]
    fn new_session_request_with_meta() {
        let mut request = NewSessionRequest::new("/workspace");
        let meta = serde_json::json!({
            "systemPrompt": {
                "append": "You are a helpful assistant."
            }
        });
        request.meta = Some(meta.as_object().cloned().unwrap_or_default());

        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(
            json["_meta"]["systemPrompt"]["append"],
            "You are a helpful assistant."
        );
    }

    #[test]
    fn prompt_request_serialization() {
        let session_id: SessionId = "test-session-123".into();
        let request = PromptRequest::new(
            session_id,
            vec![ContentBlock::Text(TextContent::new("Hello world"))],
        );
        let json = serde_json::to_value(&request).unwrap();
        assert_eq!(json["sessionId"], "test-session-123");
        assert!(json["prompt"].is_array());
    }

    #[test]
    fn session_id_roundtrip() {
        let id: SessionId = "my-session".into();
        let json = serde_json::to_value(&id).unwrap();
        assert_eq!(json, "my-session");

        let deserialized: SessionId = serde_json::from_value(json).unwrap();
        assert_eq!(deserialized.to_string(), "my-session");
    }

    #[test]
    fn stop_reason_serialization() {
        let reason = StopReason::EndTurn;
        let json = serde_json::to_value(reason).unwrap();
        assert_eq!(json, "end_turn");

        let deserialized: StopReason = serde_json::from_value(json).unwrap();
        assert!(matches!(deserialized, StopReason::EndTurn));
    }

    #[test]
    fn content_block_text_serialization() {
        let block = ContentBlock::Text(TextContent::new("test content"));
        let json = serde_json::to_value(&block).unwrap();
        assert_eq!(json["type"], "text");
        assert_eq!(json["text"], "test content");

        let deserialized: ContentBlock = serde_json::from_value(json).unwrap();
        assert!(matches!(deserialized, ContentBlock::Text(t) if t.text == "test content"));
    }

    #[test]
    fn protocol_version_serialization() {
        let version = ProtocolVersion::LATEST;
        let json = serde_json::to_value(&version).unwrap();
        // ProtocolVersion is a u16 wrapper, serializes as a number
        assert!(json.is_number() || json.is_string());
    }

    #[test]
    fn permission_request_response_serialization() {
        use sacp::schema::{
            RequestPermissionOutcome, RequestPermissionResponse, SelectedPermissionOutcome,
        };

        // Selected outcome
        let response = RequestPermissionResponse::new(RequestPermissionOutcome::Selected(
            SelectedPermissionOutcome::new("allow_once"),
        ));
        let json = serde_json::to_value(&response).unwrap();
        assert!(json["outcome"].is_object());

        // Cancelled outcome
        let response =
            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled);
        let json = serde_json::to_value(&response).unwrap();
        assert!(json["outcome"].is_string() || json["outcome"].is_object());
    }

    #[test]
    fn acp_config_with_env_vars() {
        let config = AcpConfig {
            binary: "claude-agent-acp".into(),
            cwd: "/workspace".into(),
            system_prompt: Some("Be helpful".into()),
            env_vars: vec![
                ("ANTHROPIC_API_KEY".into(), "sk-test".into()),
                ("ANTHROPIC_MODEL".into(), "claude-sonnet-4-20250514".into()),
            ],
        };
        assert_eq!(config.env_vars.len(), 2);
        assert_eq!(config.binary, "claude-agent-acp");
    }
}
