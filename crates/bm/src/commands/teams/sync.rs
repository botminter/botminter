use anyhow::{Context, Result};

use crate::bridge;
use crate::config;
use crate::profile;
use crate::workspace::{self, TeamSyncEvent};

/// Handles `bm teams sync [--repos] [--bridge] [--all] [-v] [-t team]`.
pub fn sync(repos: bool, bridge_flag: bool, verbose: bool, team_flag: Option<&str>) -> Result<()> {
    super::super::ensure_profiles(false)?;
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let manifest: profile::ProfileManifest = {
        let contents = std::fs::read_to_string(team_repo.join("botminter.yml"))
            .context("Failed to read team repo's botminter.yml")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };
    profile::check_schema_version(&team.profile, &manifest.schema_version)?;
    let coding_agent = profile::resolve_coding_agent(team, &manifest)?;

    let gh = if team.github_repo.is_empty() { None } else { Some(team.github_repo.as_str()) };

    let params = workspace::TeamSyncParams {
        team_repo: &team_repo,
        team_path: &team.path,
        team_name: &team.name,
        manifest: &manifest,
        coding_agent,
        github_repo: gh,
        project_number: team.project_number,
        repos,
        verbose,
        bridge_flag,
        workzone: &cfg.workzone,
        keyring_collection: cfg.keyring_collection.clone(),
    };
    let result = workspace::sync_team_workspaces(&params)?;

    for event in &result.events {
        display_sync_event(event);
    }

    let total = result.created + result.updated;
    println!(
        "Synced {} workspace{} ({} created, {} updated)",
        total,
        if total == 1 { "" } else { "s" },
        result.created,
        result.updated,
    );

    if !result.failures.is_empty() {
        anyhow::bail!(
            "{} workspace(s) failed to sync:\n  {}",
            result.failures.len(),
            result.failures.join("\n  ")
        );
    }

    Ok(())
}

fn display_sync_event(event: &TeamSyncEvent) {
    match event {
        TeamSyncEvent::NoMembers => println!("No members hired. Run `bm hire <role>` first."),
        TeamSyncEvent::GitPush | TeamSyncEvent::BridgeSaved => {}
        TeamSyncEvent::NoBridge => println!("No bridge configured -- skipping bridge provisioning"),
        TeamSyncEvent::BridgeAutoStart { name, result } => match result {
            bridge::BridgeStartResult::Started | bridge::BridgeStartResult::Restarted => {
                println!("Bridge '{}' started.", name);
            }
            bridge::BridgeStartResult::AlreadyRunning => println!("Bridge '{}' already running.", name),
            bridge::BridgeStartResult::External => {}
        },
        TeamSyncEvent::BridgeAutoStartSkipped { reason } => eprintln!("Warning: {}", reason),
        TeamSyncEvent::BridgeProvisionMember { name, result } => match result {
            bridge::ProvisionMemberResult::AlreadyProvisioned => println!("  {}: already provisioned -- skipping", name),
            bridge::ProvisionMemberResult::NoCreds => eprintln!("  {}: no bridge credentials -- skipping. Use `bm bridge identity add` to add later.", name),
            bridge::ProvisionMemberResult::Provisioned => println!("  {}: provisioned", name),
            bridge::ProvisionMemberResult::NoConfig => println!("  {}: onboard recipe returned no config", name),
            bridge::ProvisionMemberResult::ProvisionedWithKeyringWarning(w) => {
                eprintln!("  Warning: {}", w);
                println!("  {}: provisioned", name);
            }
            bridge::ProvisionMemberResult::ReOnboarded => println!("  {}: re-onboarded (keyring credential recovered)", name),
            bridge::ProvisionMemberResult::ReOnboardedWithKeyringWarning(w) => {
                eprintln!("  Warning: {}", w);
                println!("  {}: re-onboarded (keyring still failing)", name);
            }
        },
        TeamSyncEvent::BridgeRoomCreated(room) => println!("  Created team room: {}", room),
        TeamSyncEvent::WorkspaceCreated(name) => println!("Created workspace: {}", name),
        TeamSyncEvent::WorkspaceSynced { name, events } => {
            println!("Syncing workspace: {}", name);
            for e in events {
                display_workspace_event(e);
            }
        }
        TeamSyncEvent::WorkspaceCreateFailed { name, error } => eprintln!("Error: {}: {}", name, error),
        TeamSyncEvent::RobotInjected { member, enabled } => println!("  RObot.enabled = {} for {}", enabled, member),
        TeamSyncEvent::BrainPromptSurfaced { member } => println!("  Brain prompt surfaced for {}", member),
    }
}

fn display_workspace_event(e: &workspace::SyncEvent) {
    match e {
        workspace::SyncEvent::UpdatingSubmodule(n) => println!("  Updating {} submodule...", n),
        workspace::SyncEvent::FileCopied(n) => println!("  Copied {} (newer)", n),
        workspace::SyncEvent::FileSkipped(n) => println!("  Skipped {} (up-to-date)", n),
        workspace::SyncEvent::AgentDirRebuilt => println!("  Rebuilt agent dir symlinks"),
        workspace::SyncEvent::ChangesCommitted => println!("  Committed workspace changes"),
        workspace::SyncEvent::PushedToRemote => println!("  Pushed to remote"),
        workspace::SyncEvent::NoChanges => println!("  No changes to commit"),
        workspace::SyncEvent::BranchAlreadyOnIt(b) => println!("    Branch: {} (already on it)", b),
        workspace::SyncEvent::BranchCheckedOut(b) => println!("    Branch: {} (checked out)", b),
        workspace::SyncEvent::BranchCreated(b) => println!("    Branch: {} (created)", b),
    }
}
