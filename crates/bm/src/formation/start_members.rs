use std::path::Path;

use crate::bridge::{self, BridgeStartResult};

// ---------------------------------------------------------------------------
// Result types
// ---------------------------------------------------------------------------

/// Outcome of starting the local formation (members + optional bridge).
pub struct StartResult {
    pub launched: Vec<MemberLaunched>,
    pub skipped: Vec<MemberSkipped>,
    pub errors: Vec<super::MemberFailed>,
    pub stale_cleaned: Vec<String>,
    pub bridge: Option<BridgeAutoStartOutcome>,
}

pub struct MemberLaunched {
    pub name: String,
    pub pid: u32,
    pub brain_mode: bool,
}

pub struct MemberSkipped {
    pub name: String,
    pub pid: u32,
}

/// What happened when we tried to auto-start the bridge.
pub enum BridgeAutoStartOutcome {
    Started(String),
    Restarted(String),
    AlreadyRunning(String),
    External(String),
    JustNotFound,
}

// ---------------------------------------------------------------------------
// Bridge auto-start helper
// ---------------------------------------------------------------------------

/// Auto-start the bridge if configured and available.
pub fn auto_start_bridge(
    team_repo: &Path,
    team_name: &str,
    workzone: &Path,
) -> Option<BridgeAutoStartOutcome> {
    let bridge_dir = match bridge::discover(team_repo, team_name) {
        Ok(Some(d)) => d,
        _ => return None,
    };
    let state_path = bridge::state_path(workzone, team_name);
    let mut b = match bridge::Bridge::new(bridge_dir, state_path, team_name.to_string()) {
        Ok(b) => b,
        Err(_) => return None,
    };

    if b.is_local() {
        if which::which("just").is_err() {
            return Some(BridgeAutoStartOutcome::JustNotFound);
        }
        let bridge_name = b.bridge_name().to_string();
        match b.start() {
            Ok(BridgeStartResult::AlreadyRunning) => {
                let _ = b.save();
                Some(BridgeAutoStartOutcome::AlreadyRunning(bridge_name))
            }
            Ok(BridgeStartResult::Restarted) => {
                let _ = b.save();
                Some(BridgeAutoStartOutcome::Restarted(bridge_name))
            }
            Ok(BridgeStartResult::Started) => {
                let _ = b.save();
                Some(BridgeAutoStartOutcome::Started(bridge_name))
            }
            Ok(BridgeStartResult::External) => None,
            Err(_) => None,
        }
    } else if b.is_external() {
        Some(BridgeAutoStartOutcome::External(
            b.bridge_name().to_string(),
        ))
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use super::super::MemberFailed;

    #[test]
    fn start_result_tracks_all_outcomes() {
        let result = StartResult {
            launched: vec![MemberLaunched {
                name: "alice".to_string(),
                pid: 1234,
                brain_mode: false,
            }],
            skipped: vec![MemberSkipped {
                name: "bob".to_string(),
                pid: 5678,
            }],
            errors: vec![MemberFailed {
                name: "charlie".to_string(),
                error: "no workspace".to_string(),
            }],
            stale_cleaned: vec!["team/old-member".to_string()],
            bridge: Some(BridgeAutoStartOutcome::Started("tuwunel".to_string())),
        };

        assert_eq!(result.launched.len(), 1);
        assert_eq!(result.launched[0].name, "alice");
        assert_eq!(result.launched[0].pid, 1234);

        assert_eq!(result.skipped.len(), 1);
        assert_eq!(result.skipped[0].name, "bob");
        assert_eq!(result.skipped[0].pid, 5678);

        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].name, "charlie");

        assert_eq!(result.stale_cleaned.len(), 1);
        assert!(result.bridge.is_some());
    }
}
