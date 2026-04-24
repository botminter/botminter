use anyhow::{bail, Result};
use comfy_table::{
    ContentArrangement, modifiers::UTF8_ROUND_CORNERS, presets::UTF8_FULL_CONDENSED, Table,
};

use crate::config;
use crate::profile;
use crate::state::{self, MemberStatus};
use crate::workspace;

/// Handles `bm members list [-t team]`.
pub fn list(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let member_dirs = profile::discover_member_dirs(&team_repo);
    if member_dirs.is_empty() {
        println!("No members hired yet. Run `bm hire <role>` to hire a member.");
        return Ok(());
    }

    let members_dir = team_repo.join("members");
    let runtime_state = state::load().unwrap_or_default();

    let mut table = Table::new();
    table
        .load_preset(UTF8_FULL_CONDENSED)
        .apply_modifier(UTF8_ROUND_CORNERS)
        .set_content_arrangement(ContentArrangement::DynamicFullWidth)
        .set_header(vec!["Member", "Role", "Status", "Enabled"]);

    for member in &member_dirs {
        let role = profile::read_member_role(&members_dir, member);
        let status = state::resolve_member_status(&runtime_state, &team.name, member);
        let key = format!("{}/{}", team.name, member);
        let enabled = if state::is_enabled(&runtime_state, &key) { "yes" } else { "-" };
        table.add_row(vec![member.as_str(), role.as_str(), status.label(), enabled]);
    }

    println!("{table}");
    println!();
    println!("Enabled = daemon will auto-start this member when GitHub activity is detected (poll/webhook).");
    println!("Change with `bm enable <member>` / `bm disable <member>`.");
    Ok(())
}

/// Handles `bm members show <member> [-t team]`.
pub fn show(member: &str, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");
    let member_dir = team_repo.join("members").join(member);

    if !member_dir.is_dir() {
        bail!(
            "Member '{}' not found in team '{}'. Run `bm members list` to see hired members.",
            member, team.name
        );
    }

    println!("Member: {}", member);
    println!("Role: {}", profile::read_member_role(&team_repo.join("members"), member));

    // Status
    let runtime_state = state::load().unwrap_or_default();
    let status = state::resolve_member_status(&runtime_state, &team.name, member);
    match &status {
        MemberStatus::Running { pid, started_at, brain_mode } => {
            let label = if *brain_mode { "brain" } else { "running" };
            println!("Status: {}\nPID: {}\nStarted: {}", label, pid, started_at);
        }
        MemberStatus::Crashed { pid, started_at } => {
            println!("Status: crashed\nPID: {}\nStarted: {}", pid, started_at);
        }
        MemberStatus::Stopped => println!("Status: stopped"),
    }
    let key = format!("{}/{}", team.name, member);
    println!("Enabled: {}", if state::is_enabled(&runtime_state, &key) { "yes" } else { "no" });

    // Workspace
    let ws_path = team.path.join(member);
    if ws_path.join(".botminter.workspace").exists() {
        println!("\nWorkspace: {}", ws_path.display());
        if let Some(url) = workspace::workspace_remote_url(&ws_path) {
            println!("Workspace Repo: {}", url);
        }
        println!("Branch: {}", workspace::workspace_git_branch(&ws_path));
        let submodules = workspace::workspace_submodule_status(&ws_path);
        if !submodules.is_empty() {
            println!("Submodules:");
            for sub in &submodules {
                println!("  {}: {}", sub.name, sub.status.label());
            }
        }
    } else {
        let state_key = format!("{}/{}", team.name, member);
        if let Some(rt) = runtime_state.members.get(&state_key) {
            println!("Workspace: {}", rt.workspace.display());
        }
    }

    // Coding agent
    if let Ok(manifest) = profile::read_team_repo_manifest(&team_repo) {
        if let Ok(agent) = profile::resolve_coding_agent(team, &manifest) {
            println!("Coding Agent: {}", agent.display_name);
        }
    }

    // Knowledge & invariants
    display_file_list("Knowledge", &profile::list_files_in_dir(&member_dir.join("knowledge")));
    display_file_list("Invariants", &profile::list_files_in_dir(&member_dir.join("invariants")));

    println!();
    println!("Enabled = daemon will auto-start this member when GitHub activity is detected (poll/webhook).");
    println!("Change with `bm enable <member>` / `bm disable <member>`.");
    Ok(())
}

/// Displays a labeled list of files, or "none" if empty.
fn display_file_list(label: &str, files: &[String]) {
    println!();
    if files.is_empty() {
        println!("{}: none", label);
    } else {
        println!("{}:", label);
        for f in files {
            println!("  {}", f);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::profile;

    #[test]
    fn infer_role_standard_pattern() {
        assert_eq!(profile::infer_role_from_dir("architect-alice"), "architect");
    }

    #[test]
    fn infer_role_multiple_hyphens() {
        assert_eq!(profile::infer_role_from_dir("po-bob-senior"), "po-bob");
    }

    #[test]
    fn infer_role_hyphenated_role() {
        assert_eq!(profile::infer_role_from_dir("chief-of-staff-bob"), "chief-of-staff");
    }

    #[test]
    fn infer_role_no_hyphen() {
        assert_eq!(profile::infer_role_from_dir("superman"), "superman");
    }

    #[test]
    fn infer_role_empty_string() {
        assert_eq!(profile::infer_role_from_dir(""), "");
    }
}
