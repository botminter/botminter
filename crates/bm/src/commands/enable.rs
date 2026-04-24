use anyhow::Result;

use crate::config;
use crate::formation;
use crate::team::Team;

/// Handles `bm enable [member] [-t team] [--now]`.
pub fn run(member_filter: Option<&str>, team_flag: Option<&str>, now: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let local_formation = formation::create_local_formation(&team.name)?;
    let team_api = Team::new(team, local_formation);

    let result = team_api.enable(&cfg, member_filter, now)?;

    for name in &result.enabled {
        println!("Member '{}' enabled.", name);
    }

    if let Some(ref start) = result.start {
        for m in &start.launched {
            eprintln!("{}: started (PID {})", m.name, m.pid);
        }
        for m in &start.skipped {
            eprintln!("{}: already running (PID {})", m.name, m.pid);
        }
    }

    Ok(())
}
