use std::path::PathBuf;
use std::sync::Arc;

use crate::config;
use crate::daemon::sessions_api::SessionsApiState;

/// Shared state for the console web API handlers.
#[derive(Clone)]
pub struct WebState {
    /// Path to the botminter config file (e.g., ~/.botminter/config.yml).
    pub config_path: Arc<PathBuf>,
    /// In-memory sessions state from the daemon, for console operator visibility.
    /// None when the web server runs without a live daemon (standalone mode or tests).
    pub sessions_state: Option<SessionsApiState>,
}

impl WebState {
    /// Resolves the team repo path (where botminter.yml, members/, etc. live).
    ///
    /// In production, `team.path` points to the team directory (e.g., `workzone/my-team/`),
    /// while the actual team repo is at `team.path/team/`. This matches how
    /// `commands/daemon.rs` resolves the team repo path.
    pub fn resolve_team_repo(&self, team_name: &str) -> anyhow::Result<PathBuf> {
        let cfg = config::load_from(&self.config_path)?;
        let team = cfg
            .teams
            .iter()
            .find(|t| t.name == team_name)
            .ok_or_else(|| anyhow::anyhow!("Team '{}' not found", team_name))?;
        Ok(team.path.join("team"))
    }
}
