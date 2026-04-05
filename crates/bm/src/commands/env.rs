use anyhow::{bail, Result};

use crate::config;
use crate::formation::{self, SetupParams};

/// Runs `bm env create` — prepares the runtime environment via `formation.setup()`.
///
/// For a local formation, this verifies prerequisites (currently `ralph`; bridge flows may also need `just`).
/// For future formation types (Lima, K8s), this would provision infrastructure.
pub fn create(team: Option<&str>, formation_flag: Option<&str>) -> Result<()> {
    super::ensure_profiles(true)?;

    let cfg = config::load()?;
    let team_entry = config::resolve_team(&cfg, team)?;
    let team_repo = team_entry.path.join("team");

    let formation_name = formation::resolve_formation(&team_repo, formation_flag)?;
    let formation = formation::create_local_formation(&team_entry.name)?;

    if let Some(name) = &formation_name {
        eprintln!("Using formation: {}", name);
    }

    let params = SetupParams {
        coding_agent: "claude".to_string(),
        coding_agent_api_key: None,
    };

    match formation.setup(&params) {
        Ok(()) => {
            eprintln!("Environment ready.");
            Ok(())
        }
        Err(e) => {
            bail!("Environment setup failed: {}", e);
        }
    }
}

/// Runs `bm env delete` — tears down the runtime environment.
///
/// For Lima environments, this delegates to the existing VM deletion logic.
/// For local environments, there is nothing to tear down.
pub fn delete(name: Option<&str>, force: bool, _team: Option<&str>) -> Result<()> {
    match name {
        Some(vm_name) => {
            // Lima VM deletion — delegate to existing bootstrap logic
            crate::commands::bootstrap::delete(vm_name, force)
        }
        None => {
            eprintln!("Local environment has no infrastructure to tear down.");
            Ok(())
        }
    }
}
