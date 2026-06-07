use std::collections::HashMap;
use std::io::{BufRead, BufReader, Seek, SeekFrom};
use std::path::{Path, PathBuf};

use tokio::sync::mpsc;
use tokio::time::Duration;

use super::types::BrainMessage;

/// Event types that are significant enough to inject into the brain's prompt queue.
const SIGNIFICANT_TOPICS: &[&str] = &[
    "human.interact",
    "build.blocked",
    "task.close",
    "LOOP_COMPLETE",
];

/// A single event parsed from a Ralph JSONL event file.
#[derive(Debug, Clone, serde::Deserialize)]
struct RalphEvent {
    topic: String,
    #[serde(default)]
    payload: Option<String>,
    #[allow(dead_code)]
    ts: String,
}

/// Tracks read position in a single event file to avoid re-reading.
#[derive(Debug)]
struct FileTracker {
    /// Byte offset of the next unread position.
    offset: u64,
    /// Loop ID derived from the filename (e.g., "20260320-143052" from "events-20260320-143052.jsonl").
    loop_id: String,
}

/// Configuration for the event watcher.
#[derive(Debug, Clone)]
pub struct EventWatcherConfig {
    /// Directory containing `.ralph/events-*.jsonl` files (the workspace root).
    pub workspace_root: PathBuf,
    /// How often to poll for new events (default: 1 second).
    pub poll_interval: Duration,
}

impl Default for EventWatcherConfig {
    fn default() -> Self {
        Self {
            workspace_root: PathBuf::from("."),
            poll_interval: Duration::from_secs(1),
        }
    }
}

/// Watches Ralph event files and injects significant events into the multiplexer.
///
/// The watcher polls `.ralph/events-*.jsonl` files at a configurable interval,
/// detects new files and new lines, filters for significant event types, and
/// sends matching events into the multiplexer's prompt queue at P1 (LoopEvent) priority.
pub struct EventWatcher {
    config: EventWatcherConfig,
    /// Tracked files: path -> tracker with byte offset and loop ID.
    trackers: HashMap<PathBuf, FileTracker>,
    /// Channel to send messages to the multiplexer.
    input_tx: mpsc::Sender<BrainMessage>,
}

impl EventWatcher {
    /// Create a new event watcher.
    pub fn new(config: EventWatcherConfig, input_tx: mpsc::Sender<BrainMessage>) -> Self {
        Self {
            config,
            trackers: HashMap::new(),
            input_tx,
        }
    }

    /// Run the event watcher loop until the multiplexer shuts down (channel closes)
    /// or a shutdown signal is received.
    ///
    /// This is designed to be spawned as a tokio task:
    /// ```ignore
    /// tokio::spawn(watcher.run(shutdown_rx));
    /// ```
    pub async fn run(mut self, mut shutdown_rx: mpsc::Receiver<()>) {
        let mut interval = tokio::time::interval(self.config.poll_interval);
        // Don't burst-fire ticks that were missed while processing
        interval.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

        loop {
            tokio::select! {
                _ = shutdown_rx.recv() => {
                    tracing::info!("Event watcher shutting down");
                    return;
                }
                _ = interval.tick() => {
                    match self.poll_once().await {
                        Ok(()) => {}
                        Err(EventWatcherError::ChannelClosed) => {
                            tracing::info!("Multiplexer gone, event watcher stopping");
                            return;
                        }
                        Err(e) => {
                            tracing::warn!(error = %e, "Event watcher poll error");
                        }
                    }
                }
            }
        }
    }

    /// Perform a single poll cycle: discover files, read new lines, inject events.
    async fn poll_once(&mut self) -> Result<(), EventWatcherError> {
        let ralph_dir = self.config.workspace_root.join(".ralph");
        if !ralph_dir.is_dir() {
            return Ok(());
        }

        // Discover event files
        let event_files = discover_event_files(&ralph_dir)?;

        // Process each file
        for path in event_files {
            let new_events = self.read_new_events(&path)?;
            for (topic, payload, loop_id) in new_events {
                let summary = payload.unwrap_or_default();
                let msg = BrainMessage::loop_event(&loop_id, &topic, &summary);
                if self.input_tx.send(msg).await.is_err() {
                    // Multiplexer shut down
                    tracing::info!("Multiplexer channel closed, event watcher stopping");
                    return Err(EventWatcherError::ChannelClosed);
                }
            }
        }

        Ok(())
    }

    /// Read new events from a single file since the last known offset.
    /// Returns significant events as (topic, payload, loop_id) tuples.
    fn read_new_events(
        &mut self,
        path: &Path,
    ) -> Result<Vec<(String, Option<String>, String)>, EventWatcherError> {
        let loop_id = extract_loop_id(path);

        let tracker = self.trackers.entry(path.to_path_buf()).or_insert_with(|| {
            FileTracker {
                offset: 0,
                loop_id: loop_id.clone(),
            }
        });

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                // File was removed between discovery and open — skip
                return Ok(Vec::new());
            }
            Err(e) => return Err(EventWatcherError::Io(e)),
        };

        let metadata = file.metadata().map_err(EventWatcherError::Io)?;
        let file_len = metadata.len();

        if file_len <= tracker.offset {
            // No new data (or file was truncated — reset offset)
            if file_len < tracker.offset {
                tracing::debug!(path = %path.display(), "Event file truncated, resetting offset");
                tracker.offset = 0;
            }
            return Ok(Vec::new());
        }

        let mut reader = BufReader::new(file);
        reader
            .seek(SeekFrom::Start(tracker.offset))
            .map_err(EventWatcherError::Io)?;

        let mut results = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();
            let bytes_read = reader.read_line(&mut line).map_err(EventWatcherError::Io)?;
            if bytes_read == 0 {
                break;
            }
            tracker.offset += bytes_read as u64;

            let trimmed = line.trim();
            if trimmed.is_empty() {
                continue;
            }

            match serde_json::from_str::<RalphEvent>(trimmed) {
                Ok(event) => {
                    if is_significant(&event.topic) {
                        results.push((event.topic, event.payload, tracker.loop_id.clone()));
                    }
                }
                Err(e) => {
                    tracing::debug!(
                        error = %e,
                        line = %trimmed,
                        "Skipping malformed event line"
                    );
                }
            }
        }

        Ok(results)
    }

    /// Expose the poll method for testing (runs one poll cycle synchronously
    /// with respect to event discovery, but async for channel sends).
    #[cfg(test)]
    pub(crate) async fn poll_once_for_test(&mut self) -> Result<(), EventWatcherError> {
        self.poll_once().await
    }
}

/// Check if an event topic is significant enough to inject into the brain.
fn is_significant(topic: &str) -> bool {
    SIGNIFICANT_TOPICS.contains(&topic)
}

/// Discover all `events-*.jsonl` files in the `.ralph/` directory.
fn discover_event_files(ralph_dir: &Path) -> Result<Vec<PathBuf>, EventWatcherError> {
    let mut files = Vec::new();

    let entries = match std::fs::read_dir(ralph_dir) {
        Ok(e) => e,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(files),
        Err(e) => return Err(EventWatcherError::Io(e)),
    };

    for entry in entries {
        let entry = entry.map_err(EventWatcherError::Io)?;
        let name = entry.file_name();
        let name_str = name.to_string_lossy();
        if name_str.starts_with("events-") && name_str.ends_with(".jsonl") {
            files.push(entry.path());
        }
    }

    files.sort(); // Deterministic ordering
    Ok(files)
}

/// Extract a loop ID from an event file name.
/// `events-20260320-143052.jsonl` -> `20260320-143052`
/// Falls back to the full filename if the pattern doesn't match.
fn extract_loop_id(path: &Path) -> String {
    path.file_stem()
        .and_then(|s| s.to_str())
        .and_then(|s| s.strip_prefix("events-"))
        .unwrap_or("unknown")
        .to_string()
}

/// Errors from the event watcher.
#[derive(Debug)]
pub enum EventWatcherError {
    /// I/O error reading event files.
    Io(std::io::Error),
    /// The multiplexer channel was closed.
    ChannelClosed,
}

impl std::fmt::Display for EventWatcherError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            EventWatcherError::Io(e) => write!(f, "I/O error: {e}"),
            EventWatcherError::ChannelClosed => write!(f, "multiplexer channel closed"),
        }
    }
}

impl std::error::Error for EventWatcherError {}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    fn write_event(dir: &Path, filename: &str, topic: &str, payload: &str) {
        let ralph_dir = dir.join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();
        let path = ralph_dir.join(filename);
        use std::io::Write;
        let mut file = std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(path)
            .unwrap();
        let event = serde_json::json!({
            "topic": topic,
            "payload": payload,
            "ts": "2026-03-20T14:30:52Z"
        });
        writeln!(file, "{}", event).unwrap();
    }

    fn make_config(dir: &Path) -> EventWatcherConfig {
        EventWatcherConfig {
            workspace_root: dir.to_path_buf(),
            poll_interval: Duration::from_millis(50),
        }
    }

    // --- Unit tests ---

    #[test]
    fn significant_topics() {
        assert!(is_significant("human.interact"));
        assert!(is_significant("build.blocked"));
        assert!(is_significant("task.close"));
        assert!(is_significant("LOOP_COMPLETE"));

        assert!(!is_significant("build.completed"));
        assert!(!is_significant("hat.selected"));
        assert!(!is_significant("iteration.start"));
        assert!(!is_significant(""));
    }

    #[test]
    fn extract_loop_id_standard() {
        let path = PathBuf::from("/some/dir/.ralph/events-20260320-143052.jsonl");
        assert_eq!(extract_loop_id(&path), "20260320-143052");
    }

    #[test]
    fn extract_loop_id_complex() {
        let path = PathBuf::from("/some/dir/.ralph/events-loop-abc-def.jsonl");
        assert_eq!(extract_loop_id(&path), "loop-abc-def");
    }

    #[test]
    fn extract_loop_id_fallback() {
        let path = PathBuf::from("/some/dir/.ralph/something-else.jsonl");
        assert_eq!(extract_loop_id(&path), "unknown");
    }

    #[test]
    fn discover_event_files_empty_dir() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();
        let files = discover_event_files(&ralph_dir).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn discover_event_files_finds_matching() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        // Create matching files
        std::fs::write(ralph_dir.join("events-run1.jsonl"), "").unwrap();
        std::fs::write(ralph_dir.join("events-run2.jsonl"), "").unwrap();
        // Non-matching files
        std::fs::write(ralph_dir.join("events.jsonl"), "").unwrap();
        std::fs::write(ralph_dir.join("other.txt"), "").unwrap();

        let files = discover_event_files(&ralph_dir).unwrap();
        assert_eq!(files.len(), 2);
        assert!(files[0].to_string_lossy().contains("events-run1"));
        assert!(files[1].to_string_lossy().contains("events-run2"));
    }

    #[test]
    fn discover_event_files_missing_dir() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        // Don't create it
        let files = discover_event_files(&ralph_dir).unwrap();
        assert!(files.is_empty());
    }

    #[test]
    fn ralph_event_deserialization() {
        let json = r#"{"topic":"build.blocked","payload":"waiting for review","ts":"2026-03-20T14:30:52Z"}"#;
        let event: RalphEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.topic, "build.blocked");
        assert_eq!(event.payload.as_deref(), Some("waiting for review"));
    }

    #[test]
    fn ralph_event_without_payload() {
        let json = r#"{"topic":"LOOP_COMPLETE","ts":"2026-03-20T14:30:52Z"}"#;
        let event: RalphEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.topic, "LOOP_COMPLETE");
        assert!(event.payload.is_none());
    }

    #[test]
    fn ralph_event_with_extra_fields() {
        let json = r#"{"topic":"task.close","payload":"done","ts":"2026-03-20T14:30:52Z","iteration":5,"hat":"builder"}"#;
        let event: RalphEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.topic, "task.close");
    }

    #[test]
    fn event_watcher_error_display() {
        let err = EventWatcherError::ChannelClosed;
        assert_eq!(err.to_string(), "multiplexer channel closed");

        let io_err = std::io::Error::new(std::io::ErrorKind::NotFound, "file not found");
        let err = EventWatcherError::Io(io_err);
        assert!(err.to_string().contains("I/O error"));
    }

    #[test]
    fn event_watcher_config_default() {
        let config = EventWatcherConfig::default();
        assert_eq!(config.poll_interval, Duration::from_secs(1));
        assert_eq!(config.workspace_root, PathBuf::from("."));
    }

    // --- Integration tests ---

    #[tokio::test]
    async fn poll_detects_significant_events() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "build.blocked", "CI failed");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        watcher.poll_once_for_test().await.unwrap();

        let msg = rx.try_recv().unwrap();
        assert_eq!(msg.priority, super::super::types::Priority::LoopEvent);
        assert!(msg.content.contains("build.blocked"));
        assert!(msg.content.contains("CI failed"));
        assert_eq!(msg.source.as_deref(), Some("run1"));
    }

    #[tokio::test]
    async fn poll_filters_insignificant_events() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "hat.selected", "builder");
        write_event(tmp.path(), "events-run1.jsonl", "iteration.start", "3");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        watcher.poll_once_for_test().await.unwrap();

        assert!(rx.try_recv().is_err(), "No events should pass the filter");
    }

    #[tokio::test]
    async fn poll_does_not_reread_old_events() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "task.close", "first");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        // First poll picks up the event
        watcher.poll_once_for_test().await.unwrap();
        let _ = rx.try_recv().unwrap();

        // Second poll — no new events
        watcher.poll_once_for_test().await.unwrap();
        assert!(rx.try_recv().is_err(), "Should not re-read old events");
    }

    #[tokio::test]
    async fn poll_picks_up_appended_events() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "task.close", "first");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        // First poll
        watcher.poll_once_for_test().await.unwrap();
        let _ = rx.try_recv().unwrap();

        // Append a new event
        write_event(tmp.path(), "events-run1.jsonl", "LOOP_COMPLETE", "done");

        // Second poll picks up only the new event
        watcher.poll_once_for_test().await.unwrap();
        let msg = rx.try_recv().unwrap();
        assert!(msg.content.contains("LOOP_COMPLETE"));
        assert!(rx.try_recv().is_err(), "Should only have one new event");
    }

    #[tokio::test]
    async fn poll_detects_new_event_files() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        // First poll — no files
        watcher.poll_once_for_test().await.unwrap();
        assert!(rx.try_recv().is_err());

        // New file appears
        write_event(tmp.path(), "events-run2.jsonl", "human.interact", "hello");

        // Second poll picks it up
        watcher.poll_once_for_test().await.unwrap();
        let msg = rx.try_recv().unwrap();
        assert!(msg.content.contains("human.interact"));
        assert_eq!(msg.source.as_deref(), Some("run2"));
    }

    #[tokio::test]
    async fn poll_handles_multiple_concurrent_files() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "task.close", "task A");
        write_event(tmp.path(), "events-run2.jsonl", "build.blocked", "task B");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        watcher.poll_once_for_test().await.unwrap();

        // Should receive events from both files
        let mut received = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            received.push(msg);
        }
        assert_eq!(received.len(), 2);

        // Verify both loop IDs are present
        let sources: Vec<_> = received
            .iter()
            .map(|m| m.source.as_deref().unwrap_or(""))
            .collect();
        assert!(sources.contains(&"run1"));
        assert!(sources.contains(&"run2"));
    }

    #[tokio::test]
    async fn poll_handles_missing_ralph_dir() {
        let tmp = TempDir::new().unwrap();
        // No .ralph directory

        let (tx, _rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        // Should not error
        watcher.poll_once_for_test().await.unwrap();
    }

    #[tokio::test]
    async fn poll_skips_malformed_lines() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        // Write a file with a mix of valid and invalid lines
        let path = ralph_dir.join("events-run1.jsonl");
        std::fs::write(
            &path,
            concat!(
                r#"{"topic":"task.close","payload":"valid","ts":"2026-03-20T14:30:52Z"}"#, "\n",
                "not json at all\n",
                r#"{"topic":"LOOP_COMPLETE","ts":"2026-03-20T14:31:00Z"}"#, "\n",
            ),
        )
        .unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        watcher.poll_once_for_test().await.unwrap();

        // Should get 2 events (skipping the malformed line)
        let msg1 = rx.try_recv().unwrap();
        assert!(msg1.content.contains("task.close"));

        let msg2 = rx.try_recv().unwrap();
        assert!(msg2.content.contains("LOOP_COMPLETE"));

        assert!(rx.try_recv().is_err());
    }

    #[tokio::test]
    async fn poll_handles_file_truncation() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "task.close", "first");

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        // First poll
        watcher.poll_once_for_test().await.unwrap();
        let _ = rx.try_recv().unwrap();

        // Truncate the file (simulate log rotation)
        let path = tmp.path().join(".ralph/events-run1.jsonl");
        std::fs::write(&path, "").unwrap();

        // Should handle gracefully (offset reset)
        watcher.poll_once_for_test().await.unwrap();
        assert!(rx.try_recv().is_err());

        // Write new content after truncation
        write_event(tmp.path(), "events-run1.jsonl", "LOOP_COMPLETE", "restarted");
        watcher.poll_once_for_test().await.unwrap();
        let msg = rx.try_recv().unwrap();
        assert!(msg.content.contains("LOOP_COMPLETE"));
    }

    #[tokio::test]
    async fn run_stops_on_shutdown() {
        let tmp = TempDir::new().unwrap();
        let (tx, _rx) = mpsc::channel(16);
        let watcher = EventWatcher::new(make_config(tmp.path()), tx);

        let (shutdown_tx, shutdown_rx) = mpsc::channel(1);

        let handle = tokio::spawn(watcher.run(shutdown_rx));

        // Signal shutdown
        shutdown_tx.send(()).await.unwrap();

        // Should complete within a reasonable time
        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("watcher should stop")
            .expect("watcher task should not panic");
    }

    #[tokio::test]
    async fn run_stops_when_channel_closes() {
        let tmp = TempDir::new().unwrap();
        write_event(tmp.path(), "events-run1.jsonl", "task.close", "event");

        let (tx, rx) = mpsc::channel(1);
        let config = EventWatcherConfig {
            workspace_root: tmp.path().to_path_buf(),
            poll_interval: Duration::from_millis(10),
        };
        let watcher = EventWatcher::new(config, tx);

        let (_shutdown_tx, shutdown_rx) = mpsc::channel(1);

        // Drop the receiver — channel will close after buffer is full
        drop(rx);

        let handle = tokio::spawn(watcher.run(shutdown_rx));

        tokio::time::timeout(Duration::from_secs(2), handle)
            .await
            .expect("watcher should stop when channel closes")
            .expect("watcher task should not panic");
    }

    #[tokio::test]
    async fn mixed_significant_and_insignificant() {
        let tmp = TempDir::new().unwrap();
        let ralph_dir = tmp.path().join(".ralph");
        std::fs::create_dir_all(&ralph_dir).unwrap();

        let path = ralph_dir.join("events-run1.jsonl");
        let events = [
            r#"{"topic":"iteration.start","payload":"1","ts":"t1"}"#,
            r#"{"topic":"human.interact","payload":"question","ts":"t2"}"#,
            r#"{"topic":"hat.selected","payload":"builder","ts":"t3"}"#,
            r#"{"topic":"build.blocked","payload":"error","ts":"t4"}"#,
            r#"{"topic":"build.completed","payload":"ok","ts":"t5"}"#,
            r#"{"topic":"task.close","payload":"done","ts":"t6"}"#,
            r#"{"topic":"LOOP_COMPLETE","payload":"finished","ts":"t7"}"#,
        ];
        std::fs::write(&path, events.join("\n") + "\n").unwrap();

        let (tx, mut rx) = mpsc::channel(16);
        let mut watcher = EventWatcher::new(make_config(tmp.path()), tx);

        watcher.poll_once_for_test().await.unwrap();

        let mut received = Vec::new();
        while let Ok(msg) = rx.try_recv() {
            received.push(msg.content);
        }

        // Only 4 significant events should pass
        assert_eq!(received.len(), 4);
        assert!(received[0].contains("human.interact"));
        assert!(received[1].contains("build.blocked"));
        assert!(received[2].contains("task.close"));
        assert!(received[3].contains("LOOP_COMPLETE"));
    }
}
