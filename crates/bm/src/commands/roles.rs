use anyhow::Result;
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table};

use crate::config;
use crate::profile;

/// Handles `bm roles list [-t team]`.
pub fn list(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let manifest = profile::read_manifest(&team.profile)?;

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Role", "Description"]);

    for role in &manifest.roles {
        table.add_row(vec![role.name.as_str(), role.description.as_str()]);
    }

    println!("{table}");
    Ok(())
}
