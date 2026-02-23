use std::fs;

use anyhow::{Context, Result};
use comfy_table::{modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table};

use crate::commands::init::run_git;
use crate::config;
use crate::profile;
use crate::workspace;

/// Handles `bm teams list` — displays a table of all registered teams.
pub fn list() -> Result<()> {
    let cfg = config::load()?;

    if cfg.teams.is_empty() {
        println!("No teams registered. Run `bm init` to create one.");
        return Ok(());
    }

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_header(vec!["Team", "Profile", "GitHub", "Default"]);

    for team in &cfg.teams {
        let is_default = cfg.default_team.as_ref() == Some(&team.name);
        let default_marker = if is_default { "✔" } else { "" };
        table.add_row(vec![
            team.name.as_str(),
            team.profile.as_str(),
            team.github_repo.as_str(),
            default_marker,
        ]);
    }

    println!("{table}");
    Ok(())
}

/// Handles `bm teams sync [--push] [-t team]` — provisions and reconciles workspaces.
pub fn sync(push: bool, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Schema version guard
    let manifest_path = team_repo.join("botminter.yml");
    let manifest: profile::ProfileManifest = {
        let contents = fs::read_to_string(&manifest_path)
            .context("Failed to read team repo's botminter.yml")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };
    profile::check_schema_version(&team.profile, &manifest.schema_version)?;

    // Optional push
    if push {
        run_git(&team_repo, &["push"])?;
    }

    // Discover hired members (scan team/team/ dir)
    let members_dir = team_repo.join("team");
    let mut members: Vec<String> = Vec::new();
    if members_dir.is_dir() {
        for entry in fs::read_dir(&members_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                members.push(name);
            }
        }
    }
    members.sort();

    if members.is_empty() {
        println!("No members hired. Run `bm hire <role>` first.");
        return Ok(());
    }

    let projects = &manifest.projects;
    let mut created = 0u32;
    let mut updated = 0u32;

    for member_dir_name in &members {
        if projects.is_empty() {
            // No-project mode: workspace at {team.path}/{member_dir}/
            let ws = team.path.join(member_dir_name);
            let gh = Some(team.github_repo.as_str());
            if ws.join(".botminter").is_dir() {
                workspace::sync_workspace(&ws, member_dir_name, None, false, gh)?;
                updated += 1;
            } else {
                workspace::create_workspace(&team_repo, &team.path, member_dir_name, None, gh)?;
                created += 1;
            }
        } else {
            // Project mode: one workspace per member × project
            let gh = Some(team.github_repo.as_str());
            for proj in projects {
                let ws = team.path.join(member_dir_name).join(&proj.name);
                if ws.join(".botminter").is_dir() {
                    workspace::sync_workspace(
                        &ws,
                        member_dir_name,
                        Some(&proj.name),
                        true,
                        gh,
                    )?;
                    updated += 1;
                } else {
                    workspace::create_workspace(
                        &team_repo,
                        &team.path,
                        member_dir_name,
                        Some((&proj.name, &proj.fork_url)),
                        gh,
                    )?;
                    created += 1;
                }
            }
        }
    }

    let total = created + updated;
    println!(
        "Synced {} workspace{} ({} created, {} updated)",
        total,
        if total == 1 { "" } else { "s" },
        created,
        updated,
    );

    Ok(())
}
