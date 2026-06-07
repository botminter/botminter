use anyhow::Result;

pub const SYNC_REMOVED_MESSAGE: &str = "bm teams sync has been removed. Sessions automatically \
    use the latest committed state — no manual synchronization needed. Run `bm minty` to migrate \
    existing workspaces, or `bm start` to create a new session.";

pub fn sync(
    _repos: bool,
    _bridge_flag: bool,
    _verbose: bool,
    _team_flag: Option<&str>,
) -> Result<()> {
    anyhow::bail!(SYNC_REMOVED_MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sync_removed_message_explains_replacement() {
        assert!(
            SYNC_REMOVED_MESSAGE.contains("bm teams sync has been removed"),
            "message must state sync was removed, got: {:?}",
            SYNC_REMOVED_MESSAGE
        );
    }

    #[test]
    fn sync_removed_message_points_to_minty() {
        assert!(
            SYNC_REMOVED_MESSAGE.contains("bm minty"),
            "message must reference bm minty for migration, got: {:?}",
            SYNC_REMOVED_MESSAGE
        );
    }

    #[test]
    fn sync_removed_message_mentions_sessions() {
        assert!(
            SYNC_REMOVED_MESSAGE.contains("bm start"),
            "message must reference bm start for new sessions, got: {:?}",
            SYNC_REMOVED_MESSAGE
        );
    }
}
