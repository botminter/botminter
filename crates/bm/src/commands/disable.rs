use anyhow::Result;

use crate::config;
use crate::formation;
use crate::team::Team;

/// Handles `bm disable [member] [-t team] [--now]`.
pub fn run(member_filter: Option<&str>, team_flag: Option<&str>, now: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let local_formation = formation::create_local_formation(&team.name)?;
    let team_api = Team::new(team, local_formation);

    let result = team_api.disable(&cfg, member_filter, now)?;

    for name in &result.disabled {
        println!("Member '{}' disabled.", name);
    }

    if let Some(ref stop) = result.stop {
        for m in &stop.stopped {
            eprintln!("Stopping {}... done", m.name);
        }
    }

    Ok(())
}
