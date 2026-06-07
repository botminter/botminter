use std::path::PathBuf;
use std::sync::{Arc, RwLock};

use tokio::sync::mpsc;

use super::multiplexer::MultiplexerOutput;
use super::types::{BrainMessage, BridgeOutput};

/// Converts markdown text to HTML for Matrix `formatted_body`.
/// Enables GFM extensions (tables, strikethrough, task lists) since
/// LLM responses commonly use these.
fn markdown_to_html(md: &str) -> String {
    use pulldown_cmark::Options;
    let mut options = Options::empty();
    options.insert(Options::ENABLE_TABLES);
    options.insert(Options::ENABLE_STRIKETHROUGH);
    options.insert(Options::ENABLE_TASKLISTS);
    let parser = pulldown_cmark::Parser::new_ext(md, options);
    let mut html = String::new();
    pulldown_cmark::html::push_html(&mut html, parser);
    html
}

/// Extracts the operator-facing chat content from a brain response.
///
/// Parses `<bm-chat>...</bm-chat>` from the accumulated text.
/// Returns None if tags not found or content is empty.
/// No fallback — if the LLM does not use the tags, nothing is forwarded.
pub(crate) fn extract_chat_content(text: &str) -> Option<String> {
    let start_tag = "<bm-chat>";
    let end_tag = "</bm-chat>";

    let start = text.find(start_tag)?;
    let after = &text[start + start_tag.len()..];
    let end = after.find(end_tag)?;
    let content = after[..end].trim();

    if content.is_empty() {
        None
    } else {
        Some(content.to_string())
    }
}

/// Configuration for connecting to a Matrix homeserver.
#[derive(Debug, Clone)]
pub struct MatrixBridgeConfig {
    /// Matrix homeserver base URL (e.g., `http://localhost:8008`).
    pub homeserver_url: String,
    /// Access token for authentication.
    pub access_token: String,
    /// Room ID to listen on and send messages to.
    /// When `None`, the brain enters DM discovery mode — it accepts
    /// one invite from the operator and locks to that room.
    pub room_id: Option<String>,
    /// The member's own Matrix user ID, used to filter out echo messages.
    pub own_user_id: String,
    /// The operator's Matrix user ID. In discovery mode, only invites
    /// from this user with `is_direct: true` are accepted.
    pub operator_user_id: Option<String>,
    /// Workspace path for persisting the discovered DM room ID.
    pub workspace: Option<PathBuf>,
}

/// Shared active room state between reader and writer.
///
/// The reader sets this when it discovers a DM room via invite.
/// The writer reads it to know where to send responses.
pub type ActiveRoom = Arc<RwLock<Option<String>>>;

/// Create a new `ActiveRoom`, optionally pre-populated with a known room ID.
pub fn active_room(room_id: Option<String>) -> ActiveRoom {
    Arc::new(RwLock::new(room_id))
}

// ── Reader ──────────────────────────────────────────────────────────────

/// Polls Matrix for new messages and injects them into the multiplexer.
///
/// Operates in two modes:
/// - **Locked mode** (`room_id` is set): listens to one specific room
/// - **Discovery mode** (`room_id` is None): watches for invites from the
///   operator, auto-joins the first DM invite, then locks to that room
pub struct MatrixBridgeReader {
    config: MatrixBridgeConfig,
    client: reqwest::Client,
    input_tx: mpsc::Sender<BrainMessage>,
    active_room: ActiveRoom,
}

impl MatrixBridgeReader {
    pub fn new(
        config: MatrixBridgeConfig,
        input_tx: mpsc::Sender<BrainMessage>,
        active_room: ActiveRoom,
    ) -> Self {
        let client = reqwest::Client::new();
        Self {
            config,
            client,
            input_tx,
            active_room,
        }
    }

    /// Run the reader loop. Polls `/sync` with long-polling and injects
    /// messages into the multiplexer. Stops on shutdown signal or when
    /// the multiplexer channel closes.
    pub async fn run(self, mut shutdown_rx: mpsc::Receiver<()>) {
        let mut since: Option<String> = None;
        let mut backoff_secs: u64 = 1;
        const MAX_BACKOFF_SECS: u64 = 30;

        // In locked mode, join the configured room before polling.
        if let Some(ref room_id) = self.config.room_id {
            if let Err(e) = self.join_room(room_id).await {
                tracing::warn!(error = %e, "Failed to join room (may already be joined)");
            }
        } else {
            tracing::info!("Bridge reader starting in DM discovery mode — waiting for operator invite");
        }

        // Do an initial sync with timeout=0 to get the `since` token
        // without processing old messages.
        match self.initial_sync().await {
            Ok(token) => {
                since = Some(token);
                tracing::info!("Bridge reader initial sync complete");
            }
            Err(e) => {
                tracing::error!(error = %e, "Bridge reader initial sync failed, will retry in poll loop");
            }
        }

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::info!("Bridge reader shutting down");
                    return;
                }
                result = self.poll_sync(since.as_deref()) => {
                    match result {
                        Ok((next_batch, sync)) => {
                            backoff_secs = 1;
                            since = Some(next_batch);

                            // In discovery mode, check for invites before processing messages
                            let current_room = self.get_active_room();
                            if current_room.is_none() {
                                if let Some(room_id) = self.check_invites(&sync).await {
                                    self.set_active_room(&room_id);
                                    self.persist_dm_room(&room_id);
                                    tracing::info!(
                                        room_id = %room_id,
                                        "DM room discovered and joined — locked to this room"
                                    );
                                    // Messages from this room will appear in the next sync
                                    continue;
                                }
                            }

                            // Extract messages from the active room
                            if let Some(ref room_id) = self.get_active_room() {
                                let messages = extract_room_messages(
                                    &sync, room_id, &self.config.own_user_id,
                                );
                                for (body, sender) in messages {
                                    let msg = BrainMessage::human_from(&body, &sender);
                                    if self.input_tx.send(msg).await.is_err() {
                                        tracing::info!("Multiplexer channel closed, bridge reader stopping");
                                        return;
                                    }
                                    tracing::info!(
                                        sender = %sender,
                                        body_len = body.len(),
                                        "Injected bridge message into multiplexer"
                                    );
                                }
                            }
                        }
                        Err(e) => {
                            tracing::warn!(
                                error = %e,
                                backoff_secs = backoff_secs,
                                "Bridge reader sync error, retrying after backoff"
                            );
                            tokio::select! {
                                _ = shutdown_rx.recv() => {
                                    tracing::info!("Bridge reader shutting down during backoff");
                                    return;
                                }
                                _ = tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)) => {}
                            }
                            backoff_secs = (backoff_secs * 2).min(MAX_BACKOFF_SECS);
                        }
                    }
                }
            }
        }
    }

    /// Build the server-side filter JSON for `/sync`.
    ///
    /// In locked mode, restricts to the configured room.
    /// In discovery mode, no room filter — receives all rooms + invites.
    fn sync_filter(&self) -> String {
        if let Some(ref room_id) = self.get_active_room() {
            // Locked mode: filter to specific room
            serde_json::json!({
                "room": {
                    "rooms": [room_id],
                    "timeline": {
                        "types": ["m.room.message"]
                    }
                }
            })
            .to_string()
        } else {
            // Discovery mode: no room filter, receive invites
            serde_json::json!({
                "room": {
                    "timeline": {
                        "types": ["m.room.message"]
                    }
                }
            })
            .to_string()
        }
    }

    /// Join a Matrix room by ID. Idempotent — succeeds if already joined,
    /// and accepts a pending invite if one exists.
    async fn join_room(&self, room_id: &str) -> Result<(), BridgeAdapterError> {
        let url = format!(
            "{}/_matrix/client/v3/join/{}",
            self.config.homeserver_url,
            urlencoded(room_id)
        );
        let resp = self
            .client
            .post(&url)
            .bearer_auth(&self.config.access_token)
            .json(&serde_json::json!({}))
            .timeout(std::time::Duration::from_secs(10))
            .send()
            .await
            .map_err(|e| BridgeAdapterError::Http(e.to_string()))?;
        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BridgeAdapterError::Http(format!(
                "join room failed: {status} — {body}"
            )));
        }
        tracing::info!(room_id = %room_id, "Joined Matrix room");
        Ok(())
    }

    /// Perform an initial sync with `timeout=0` to get the `since` token
    /// without processing historical messages.
    async fn initial_sync(&self) -> Result<String, BridgeAdapterError> {
        let url = format!(
            "{}/_matrix/client/v3/sync",
            self.config.homeserver_url
        );

        let resp = self
            .client
            .get(&url)
            .query(&[("timeout", "0"), ("filter", &self.sync_filter())])
            .bearer_auth(&self.config.access_token)
            .timeout(std::time::Duration::from_secs(30))
            .send()
            .await
            .map_err(|e| BridgeAdapterError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BridgeAdapterError::Http(format!(
                "initial sync failed: {status} — {body}"
            )));
        }

        let sync: SyncResponse = resp
            .json()
            .await
            .map_err(|e| BridgeAdapterError::Parse(e.to_string()))?;

        Ok(sync.next_batch)
    }

    /// Long-poll `/sync` for new events since the given token.
    /// Returns the parsed sync response for further processing.
    async fn poll_sync(
        &self,
        since: Option<&str>,
    ) -> Result<(String, SyncResponse), BridgeAdapterError> {
        let url = format!(
            "{}/_matrix/client/v3/sync",
            self.config.homeserver_url
        );

        let filter = self.sync_filter();
        let mut params: Vec<(&str, &str)> =
            vec![("timeout", "30000"), ("filter", &filter)];
        if let Some(since) = since {
            params.push(("since", since));
        }

        let resp = self
            .client
            .get(&url)
            .query(&params)
            .bearer_auth(&self.config.access_token)
            .timeout(std::time::Duration::from_secs(60))
            .send()
            .await
            .map_err(|e| BridgeAdapterError::Http(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let body = resp.text().await.unwrap_or_default();
            return Err(BridgeAdapterError::Http(format!(
                "sync failed: {status} — {body}"
            )));
        }

        let sync: SyncResponse = resp
            .json()
            .await
            .map_err(|e| BridgeAdapterError::Parse(e.to_string()))?;

        let next_batch = sync.next_batch.clone();
        Ok((next_batch, sync))
    }

    /// Check sync response for DM invites from the operator.
    /// If found, auto-joins the room and returns the room ID.
    async fn check_invites(&self, sync: &SyncResponse) -> Option<String> {
        let rooms = sync.rooms.as_ref()?;
        let invites = rooms.invite.as_ref()?;

        let operator = self.config.operator_user_id.as_deref()?;

        for (room_id, invited_room) in invites {
            let invite_state = invited_room.invite_state.as_ref()?;

            // Check if this is a direct invite from the operator
            let is_dm_from_operator = invite_state.events.iter().any(|event| {
                event.event_type == "m.room.member"
                    && event.state_key == self.config.own_user_id
                    && event.sender == operator
                    && event
                        .content
                        .as_ref()
                        .and_then(|c| c.get("is_direct"))
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
            });

            if !is_dm_from_operator {
                tracing::debug!(
                    room_id = %room_id,
                    "Ignoring invite — not a DM from operator"
                );
                continue;
            }

            tracing::info!(
                room_id = %room_id,
                operator = %operator,
                "Received DM invite from operator — joining"
            );

            // Auto-join the DM room
            match self.join_room(room_id).await {
                Ok(()) => return Some(room_id.clone()),
                Err(e) => {
                    tracing::error!(
                        room_id = %room_id,
                        error = %e,
                        "Failed to join operator DM room"
                    );
                }
            }
        }

        None
    }

    /// Get the current active room ID (from config or discovery).
    fn get_active_room(&self) -> Option<String> {
        // Check shared state first (set by discovery)
        if let Ok(guard) = self.active_room.read() {
            if guard.is_some() {
                return guard.clone();
            }
        }
        // Fall back to config
        self.config.room_id.clone()
    }

    /// Set the active room ID (called when a DM room is discovered).
    fn set_active_room(&self, room_id: &str) {
        if let Ok(mut guard) = self.active_room.write() {
            *guard = Some(room_id.to_string());
        }
    }

    /// Persist the discovered DM room ID to the session workspace and the durable member-state path.
    fn persist_dm_room(&self, room_id: &str) {
        let json = serde_json::json!({
            "room_id": room_id,
            "discovered_at": chrono::Utc::now().to_rfc3339(),
        });
        let json_str = serde_json::to_string_pretty(&json).unwrap_or_default();

        if let Some(ref workspace) = self.config.workspace {
            let dm_file = workspace.join("dm-room.json");
            if let Err(e) = std::fs::write(&dm_file, &json_str) {
                tracing::warn!(error = %e, "Failed to persist DM room ID to {}", dm_file.display());
            } else {
                tracing::info!(path = %dm_file.display(), "Persisted DM room ID");
            }
        }

        // Also write to durable member-state path so the room survives session restarts.
        if let Ok(state_dir) = std::env::var("BM_MEMBER_STATE_DIR") {
            let durable_file = std::path::PathBuf::from(&state_dir).join("dm-room.json");
            if let Some(parent) = durable_file.parent() {
                if let Err(e) = std::fs::create_dir_all(parent) {
                    tracing::warn!(error = %e, "Failed to create durable state directory {}", parent.display());
                }
            }
            if let Err(e) = std::fs::write(&durable_file, &json_str) {
                tracing::warn!(error = %e, "Failed to persist DM room ID to durable path {}", durable_file.display());
            } else {
                tracing::info!(path = %durable_file.display(), "Persisted DM room ID to durable member state");
            }
        }
    }
}

// ── Writer ──────────────────────────────────────────────────────────────

/// Reads `BridgeOutput` events from the multiplexer and sends accumulated
/// text to the Matrix room on `TurnComplete`.
pub struct MatrixBridgeWriter {
    config: MatrixBridgeConfig,
    client: reqwest::Client,
    active_room: ActiveRoom,
}

impl MatrixBridgeWriter {
    pub fn new(config: MatrixBridgeConfig, active_room: ActiveRoom) -> Self {
        let client = reqwest::Client::new();
        Self {
            config,
            client,
            active_room,
        }
    }

    /// Run the writer loop. Reads from `MultiplexerOutput` and sends
    /// text to the Matrix room using debounced streaming with `<bm-chat>` parsing.
    pub async fn run(self, mut output: MultiplexerOutput) {
        let mut buffer = String::new();
        let debounce = tokio::time::Duration::from_millis(500);

        loop {
            let event = if !buffer.is_empty() {
                match tokio::time::timeout(debounce, output.recv()).await {
                    Ok(event) => event,
                    Err(_) => {
                        // Debounce expired — only flush if we have complete tags
                        if buffer.contains("</bm-chat>") {
                            let text = std::mem::take(&mut buffer);
                            self.flush_chat_content(&text).await;
                        }
                        continue;
                    }
                }
            } else {
                output.recv().await
            };

            match event {
                Some(BridgeOutput::Text(chunk)) => {
                    buffer.push_str(&chunk);
                }
                Some(BridgeOutput::TurnComplete) => {
                    if !buffer.is_empty() {
                        let text = std::mem::take(&mut buffer);
                        self.flush_chat_content(&text).await;
                    }
                }
                Some(BridgeOutput::Error(err)) => {
                    let text = format!("[Brain error]: {err}");
                    if let Err(e) = self.send_message(&text).await {
                        tracing::error!(error = %e, "Failed to send error message to Matrix room");
                    }
                    buffer.clear();
                }
                None => {
                    if !buffer.is_empty() {
                        self.flush_chat_content(&buffer).await;
                    }
                    tracing::info!("Bridge writer stopping (multiplexer shut down)");
                    return;
                }
            }
        }
    }

    /// Extract `<bm-chat>` content and send to Matrix.
    async fn flush_chat_content(&self, text: &str) {
        match extract_chat_content(text) {
            Some(msg) => {
                if let Err(e) = self.send_message(&msg).await {
                    tracing::error!(error = %e, "Failed to send chat message to Matrix");
                }
            }
            None => {
                tracing::debug!("No <bm-chat> content to forward to operator");
            }
        }
    }

    /// Get the room ID to send to (from shared state or config).
    fn get_room_id(&self) -> Option<String> {
        if let Ok(guard) = self.active_room.read() {
            if guard.is_some() {
                return guard.clone();
            }
        }
        self.config.room_id.clone()
    }

    /// Send a message to the active Matrix room with retry on transient errors.
    async fn send_message(&self, body: &str) -> Result<(), BridgeAdapterError> {
        let room_id = match self.get_room_id() {
            Some(rid) => rid,
            None => {
                tracing::warn!("No active room — skipping message send (waiting for DM discovery)");
                return Ok(());
            }
        };

        let txn_id = uuid::Uuid::new_v4().to_string();
        let url = format!(
            "{}/_matrix/client/v3/rooms/{}/send/m.room.message/{}",
            self.config.homeserver_url,
            urlencoded(&room_id),
            txn_id
        );

        let payload = serde_json::json!({
            "msgtype": "m.text",
            "body": body,
            "format": "org.matrix.custom.html",
            "formatted_body": markdown_to_html(body)
        });

        let mut backoff_secs: u64 = 1;
        const MAX_RETRIES: u32 = 3;

        for attempt in 0..=MAX_RETRIES {
            let result = self
                .client
                .put(&url)
                .bearer_auth(&self.config.access_token)
                .json(&payload)
                .timeout(std::time::Duration::from_secs(30))
                .send()
                .await;

            match result {
                Ok(resp) if resp.status().is_success() => {
                    tracing::debug!(txn_id = %txn_id, body_len = body.len(), "Message sent to Matrix room");
                    return Ok(());
                }
                Ok(resp) => {
                    let status = resp.status();
                    let resp_body = resp.text().await.unwrap_or_default();
                    if attempt < MAX_RETRIES
                        && (status.is_server_error() || status.as_u16() == 429)
                    {
                        tracing::warn!(
                            status = %status,
                            attempt = attempt,
                            "Transient error sending message, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs *= 2;
                        continue;
                    }
                    return Err(BridgeAdapterError::Http(format!(
                        "send message failed: {status} — {resp_body}"
                    )));
                }
                Err(e) => {
                    if attempt < MAX_RETRIES {
                        tracing::warn!(
                            error = %e,
                            attempt = attempt,
                            "HTTP error sending message, retrying"
                        );
                        tokio::time::sleep(std::time::Duration::from_secs(backoff_secs)).await;
                        backoff_secs *= 2;
                        continue;
                    }
                    return Err(BridgeAdapterError::Http(e.to_string()));
                }
            }
        }

        unreachable!("retry loop should return before this point")
    }
}

// ── Shared types ────────────────────────────────────────────────────────

/// Errors from the bridge adapter.
#[derive(Debug)]
pub enum BridgeAdapterError {
    Http(String),
    Parse(String),
}

impl std::fmt::Display for BridgeAdapterError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            BridgeAdapterError::Http(e) => write!(f, "HTTP error: {e}"),
            BridgeAdapterError::Parse(e) => write!(f, "parse error: {e}"),
        }
    }
}

impl std::error::Error for BridgeAdapterError {}

/// Minimal URL-encoding for room IDs (which contain `!` and `:`).
fn urlencoded(s: &str) -> String {
    s.replace('!', "%21")
        .replace('#', "%23")
        .replace(':', "%3A")
}

// ── /sync response types ────────────────────────────────────────────────

#[derive(Debug, serde::Deserialize)]
pub(crate) struct SyncResponse {
    pub next_batch: String,
    #[serde(default)]
    pub rooms: Option<SyncRooms>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct SyncRooms {
    #[serde(default)]
    pub join: Option<std::collections::HashMap<String, JoinedRoom>>,
    #[serde(default)]
    pub invite: Option<std::collections::HashMap<String, InvitedRoom>>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct JoinedRoom {
    #[serde(default)]
    pub timeline: Option<Timeline>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct Timeline {
    #[serde(default)]
    pub events: Vec<TimelineEvent>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct TimelineEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub sender: String,
    #[serde(default)]
    pub content: Option<MessageContent>,
}

#[derive(Debug, serde::Deserialize)]
pub(crate) struct MessageContent {
    #[serde(default)]
    #[allow(dead_code)]
    pub msgtype: Option<String>,
    #[serde(default)]
    pub body: Option<String>,
}

/// A room the user has been invited to.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct InvitedRoom {
    #[serde(default)]
    pub invite_state: Option<InviteState>,
}

/// Stripped state events included with an invite.
#[derive(Debug, serde::Deserialize)]
pub(crate) struct InviteState {
    #[serde(default)]
    pub events: Vec<StrippedStateEvent>,
}

/// A stripped state event (included in invite previews).
#[derive(Debug, serde::Deserialize)]
pub(crate) struct StrippedStateEvent {
    #[serde(rename = "type")]
    pub event_type: String,
    pub sender: String,
    #[serde(default)]
    pub state_key: String,
    #[serde(default)]
    pub content: Option<serde_json::Value>,
}

/// Extract `(body, sender)` pairs from a sync response for the target room,
/// filtering out messages from `own_user_id`.
pub(crate) fn extract_room_messages(
    sync: &SyncResponse,
    room_id: &str,
    own_user_id: &str,
) -> Vec<(String, String)> {
    let rooms = match &sync.rooms {
        Some(r) => r,
        None => return Vec::new(),
    };
    let joined = match &rooms.join {
        Some(j) => j,
        None => return Vec::new(),
    };
    let room = match joined.get(room_id) {
        Some(r) => r,
        None => return Vec::new(),
    };
    let timeline = match &room.timeline {
        Some(t) => t,
        None => return Vec::new(),
    };

    timeline
        .events
        .iter()
        .filter(|e| e.event_type == "m.room.message")
        .filter(|e| e.sender != own_user_id)
        .filter_map(|e| {
            let content = e.content.as_ref()?;
            let body = content.body.as_ref()?;
            if body.is_empty() {
                return None;
            }
            Some((body.clone(), e.sender.clone()))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── SyncResponse deserialization tests ───────────────────────────

    #[test]
    fn deserialize_empty_sync_response() {
        let json = r#"{"next_batch": "s123_456"}"#;
        let sync: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(sync.next_batch, "s123_456");
        assert!(sync.rooms.is_none());
    }

    #[test]
    fn deserialize_sync_with_room_message() {
        let json = r#"{
            "next_batch": "s42_0",
            "rooms": {
                "join": {
                    "!room1:localhost": {
                        "timeline": {
                            "events": [
                                {
                                    "type": "m.room.message",
                                    "sender": "@alice:localhost",
                                    "content": {
                                        "msgtype": "m.text",
                                        "body": "Hello brain!"
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;

        let sync: SyncResponse = serde_json::from_str(json).unwrap();
        assert_eq!(sync.next_batch, "s42_0");

        let rooms = sync.rooms.unwrap();
        let joined = rooms.join.unwrap();
        let room = joined.get("!room1:localhost").unwrap();
        let timeline = room.timeline.as_ref().unwrap();
        assert_eq!(timeline.events.len(), 1);
        assert_eq!(timeline.events[0].sender, "@alice:localhost");
        assert_eq!(
            timeline.events[0].content.as_ref().unwrap().body.as_deref(),
            Some("Hello brain!")
        );
    }

    #[test]
    fn deserialize_sync_with_invite() {
        let json = r#"{
            "next_batch": "s50",
            "rooms": {
                "invite": {
                    "!dm_room:localhost": {
                        "invite_state": {
                            "events": [
                                {
                                    "type": "m.room.member",
                                    "sender": "@operator:localhost",
                                    "state_key": "@brain:localhost",
                                    "content": {
                                        "membership": "invite",
                                        "is_direct": true
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;

        let sync: SyncResponse = serde_json::from_str(json).unwrap();
        let rooms = sync.rooms.unwrap();
        let invites = rooms.invite.unwrap();
        assert!(invites.contains_key("!dm_room:localhost"));

        let invited = &invites["!dm_room:localhost"];
        let state = invited.invite_state.as_ref().unwrap();
        assert_eq!(state.events.len(), 1);
        assert_eq!(state.events[0].sender, "@operator:localhost");
        assert_eq!(state.events[0].state_key, "@brain:localhost");

        let is_direct = state.events[0]
            .content
            .as_ref()
            .unwrap()
            .get("is_direct")
            .unwrap()
            .as_bool()
            .unwrap();
        assert!(is_direct);
    }

    #[test]
    fn deserialize_sync_with_multiple_event_types() {
        let json = r#"{
            "next_batch": "s100",
            "rooms": {
                "join": {
                    "!room:localhost": {
                        "timeline": {
                            "events": [
                                {
                                    "type": "m.room.member",
                                    "sender": "@bob:localhost",
                                    "content": {}
                                },
                                {
                                    "type": "m.room.message",
                                    "sender": "@alice:localhost",
                                    "content": {
                                        "msgtype": "m.text",
                                        "body": "Check the board"
                                    }
                                },
                                {
                                    "type": "m.room.message",
                                    "sender": "@bot:localhost",
                                    "content": {
                                        "msgtype": "m.text",
                                        "body": "Bot response"
                                    }
                                }
                            ]
                        }
                    }
                }
            }
        }"#;

        let sync: SyncResponse = serde_json::from_str(json).unwrap();
        let messages = extract_room_messages(&sync, "!room:localhost", "@bot:localhost");

        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "Check the board");
        assert_eq!(messages[0].1, "@alice:localhost");
    }

    // ── extract_room_messages tests ─────────────────────────────────

    #[test]
    fn extract_filters_own_messages() {
        let sync = make_sync_with_messages(
            "!r:localhost",
            vec![
                ("@me:localhost", "echo"),
                ("@alice:localhost", "real message"),
            ],
        );

        let messages = extract_room_messages(&sync, "!r:localhost", "@me:localhost");
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].0, "real message");
        assert_eq!(messages[0].1, "@alice:localhost");
    }

    #[test]
    fn extract_ignores_wrong_room() {
        let sync = make_sync_with_messages(
            "!room_a:localhost",
            vec![("@alice:localhost", "hello")],
        );

        let messages = extract_room_messages(&sync, "!room_b:localhost", "@me:localhost");
        assert!(messages.is_empty());
    }

    #[test]
    fn extract_ignores_empty_body() {
        let sync = SyncResponse {
            next_batch: "s1".into(),
            rooms: Some(SyncRooms {
                join: Some({
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "!r:localhost".into(),
                        JoinedRoom {
                            timeline: Some(Timeline {
                                events: vec![TimelineEvent {
                                    event_type: "m.room.message".into(),
                                    sender: "@alice:localhost".into(),
                                    content: Some(MessageContent {
                                        msgtype: Some("m.text".into()),
                                        body: Some("".into()),
                                    }),
                                }],
                            }),
                        },
                    );
                    m
                }),
                invite: None,
            }),
        };

        let messages = extract_room_messages(&sync, "!r:localhost", "@me:localhost");
        assert!(messages.is_empty());
    }

    #[test]
    fn extract_handles_no_rooms() {
        let sync = SyncResponse {
            next_batch: "s1".into(),
            rooms: None,
        };
        let messages = extract_room_messages(&sync, "!r:localhost", "@me:localhost");
        assert!(messages.is_empty());
    }

    #[test]
    fn extract_handles_no_join() {
        let sync = SyncResponse {
            next_batch: "s1".into(),
            rooms: Some(SyncRooms {
                join: None,
                invite: None,
            }),
        };
        let messages = extract_room_messages(&sync, "!r:localhost", "@me:localhost");
        assert!(messages.is_empty());
    }

    #[test]
    fn extract_handles_no_timeline() {
        let sync = SyncResponse {
            next_batch: "s1".into(),
            rooms: Some(SyncRooms {
                join: Some({
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        "!r:localhost".into(),
                        JoinedRoom { timeline: None },
                    );
                    m
                }),
                invite: None,
            }),
        };
        let messages = extract_room_messages(&sync, "!r:localhost", "@me:localhost");
        assert!(messages.is_empty());
    }

    // ── URL encoding tests ──────────────────────────────────────────

    #[test]
    fn urlencoded_room_id() {
        let encoded = urlencoded("!abc:localhost");
        assert_eq!(encoded, "%21abc%3Alocalhost");
    }

    // ── Error display tests ─────────────────────────────────────────

    #[test]
    fn error_display() {
        let err = BridgeAdapterError::Http("connection refused".into());
        assert_eq!(err.to_string(), "HTTP error: connection refused");

        let err = BridgeAdapterError::Parse("invalid json".into());
        assert_eq!(err.to_string(), "parse error: invalid json");
    }

    // ── Test helpers ────────────────────────────────────────────────

    fn make_sync_with_messages(
        room_id: &str,
        messages: Vec<(&str, &str)>,
    ) -> SyncResponse {
        let events = messages
            .into_iter()
            .map(|(sender, body)| TimelineEvent {
                event_type: "m.room.message".into(),
                sender: sender.into(),
                content: Some(MessageContent {
                    msgtype: Some("m.text".into()),
                    body: Some(body.into()),
                }),
            })
            .collect();

        SyncResponse {
            next_batch: "s1".into(),
            rooms: Some(SyncRooms {
                join: Some({
                    let mut m = std::collections::HashMap::new();
                    m.insert(
                        room_id.into(),
                        JoinedRoom {
                            timeline: Some(Timeline { events }),
                        },
                    );
                    m
                }),
                invite: None,
            }),
        }
    }

    #[test]
    fn extract_chat_content_returns_trimmed() {
        let text = "<bm-response>\n<bm-chat>\nHello operator!\n</bm-chat>\n</bm-response>";
        assert_eq!(extract_chat_content(text), Some("Hello operator!".into()));
    }

    #[test]
    fn extract_chat_content_empty_tags() {
        let text = "<bm-response><bm-chat>  </bm-chat></bm-response>";
        assert_eq!(extract_chat_content(text), None);
    }

    #[test]
    fn extract_chat_content_no_tags() {
        let text = "Just some plain text without any tags";
        assert_eq!(extract_chat_content(text), None);
    }

    #[test]
    fn extract_chat_content_missing_end_tag() {
        let text = "<bm-chat>partial content";
        assert_eq!(extract_chat_content(text), None);
    }

    #[test]
    fn extract_chat_content_multiline() {
        let text = "<bm-chat>\nLine 1\nLine 2\nLine 3\n</bm-chat>";
        assert_eq!(extract_chat_content(text), Some("Line 1\nLine 2\nLine 3".into()));
    }

    #[test]
    fn extract_chat_content_ignores_surrounding() {
        let text = "internal stuff <bm-response><bm-chat>visible</bm-chat></bm-response> more internal";
        assert_eq!(extract_chat_content(text), Some("visible".into()));
    }
}
