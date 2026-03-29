use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::brain;
use crate::bridge::{self, Bridge, LocalCredentialStore};
use crate::profile::{self, CodingAgentDef, ProfileManifest};
use crate::workspace;
use crate::workspace::GhRemoteOps;

// ── Sync parameters ─────────────────────────────────────────────────

/// All parameters needed for `bm teams sync`.
pub struct TeamSyncParams<'a> {
    pub team_repo: &'a Path,
    pub team_path: &'a Path,
    pub team_name: &'a str,
    pub manifest: &'a ProfileManifest,
    pub coding_agent: &'a CodingAgentDef,
    pub github_repo: Option<&'a str>,
    pub project_number: Option<u64>,
    pub repos: bool,
    pub verbose: bool,
    pub bridge_flag: bool,
    pub workzone: &'a Path,
    pub keyring_collection: Option<String>,
}

// ── Sync result ─────────────────────────────────────────────────────

/// Result of syncing all team workspaces.
pub struct TeamSyncResult {
    pub created: u32,
    pub updated: u32,
    pub failures: Vec<String>,
    pub events: Vec<TeamSyncEvent>,
}

/// Events emitted during team sync, for the command layer to display.
pub enum TeamSyncEvent {
    NoMembers,
    GitPush,
    NoBridge,
    BridgeAutoStart { name: String, result: bridge::BridgeStartResult },
    BridgeAutoStartSkipped { reason: String },
    BridgeProvisionMember { name: String, result: bridge::ProvisionMemberResult },
    BridgeRoomCreated(String),
    BridgeSaved,
    WorkspaceCreated(String),
    WorkspaceSynced { name: String, events: Vec<workspace::SyncEvent> },
    WorkspaceCreateFailed { name: String, error: String },
    RobotInjected { member: String, enabled: bool },
    BrainPromptSurfaced { member: String },
}

// ── Sync orchestration ──────────────────────────────────────────────

/// Orchestrates the full `bm teams sync` operation: git push, bridge
/// provisioning, workspace creation/sync, and RObot config injection.
///
/// Returns a structured result with counts and events for display.
pub fn sync_team_workspaces(params: &TeamSyncParams) -> Result<TeamSyncResult> {
    let mut events = Vec::new();

    // Optional git push (--repos flag)
    if params.repos {
        crate::git::run_git(params.team_repo, &["push"])?;
        events.push(TeamSyncEvent::GitPush);
    }

    // Bridge provisioning (--bridge flag)
    let bridge_dir = bridge::discover(params.team_repo, params.team_name)?;
    if params.bridge_flag {
        provision_bridge(params, &bridge_dir, &mut events)?;
    }

    // Discover hired members
    let members = profile::discover_member_dirs(params.team_repo);
    if members.is_empty() {
        events.push(TeamSyncEvent::NoMembers);
        return Ok(TeamSyncResult {
            created: 0,
            updated: 0,
            failures: Vec::new(),
            events,
        });
    }

    // Build project list for workspace repo creation
    let project_refs: Vec<(&str, &str)> = params
        .manifest
        .projects
        .iter()
        .map(|p| (p.name.as_str(), p.fork_url.as_str()))
        .collect();

    // Set up bridge context for RObot injection
    let robot_context = build_robot_context(params, &bridge_dir)?;

    // Build remote ops for push mode
    let gh_ops = if params.repos {
        crate::git::detect_token().map(|token| GhRemoteOps {
            gh_token: token,
        })
    } else {
        None
    };

    let mut created = 0u32;
    let mut updated = 0u32;
    let mut failures: Vec<String> = Vec::new();

    for member_dir_name in &members {
        let ws = params.team_path.join(member_dir_name);

        // Clean up stale local dir without a workspace marker
        if ws.exists() && !ws.join(".botminter.workspace").exists() {
            fs::remove_dir_all(&ws).ok();
        }

        if ws.join(".botminter.workspace").exists() {
            // Existing workspace — sync it
            let sync_result = workspace::sync_workspace(
                &ws,
                member_dir_name,
                params.coding_agent,
                params.verbose,
                params.repos,
                params.project_number,
                params.github_repo,
            )?;
            events.push(TeamSyncEvent::WorkspaceSynced {
                name: member_dir_name.clone(),
                events: sync_result.events,
            });
            updated += 1;
        } else {
            // New workspace — create
            let ws_params = workspace::WorkspaceRepoParams {
                team_repo_path: params.team_repo,
                workspace_base: params.team_path,
                member_dir_name,
                team_name: params.team_name,
                projects: &project_refs,
                github_repo: params.github_repo,
                project_number: params.project_number,
                push: params.repos,
                coding_agent: params.coding_agent,
                remote_ops: gh_ops.as_ref().map(|o| o as &dyn workspace::RemoteRepoOps),
                team_submodule_url: None,
            };
            match workspace::create_workspace_repo(&ws_params) {
                Ok(()) => {
                    events.push(TeamSyncEvent::WorkspaceCreated(member_dir_name.clone()));
                    created += 1;
                }
                Err(e) => {
                    events.push(TeamSyncEvent::WorkspaceCreateFailed {
                        name: member_dir_name.clone(),
                        error: format!("{:#}", e),
                    });
                    failures.push(member_dir_name.clone());
                    continue;
                }
            }
        }

        // Inject RObot config
        if let Some(ref ctx) = robot_context {
            inject_robot_for_member(&ws, member_dir_name, ctx, params, &mut events)?;
        }

        // Surface brain prompt (rendered from profile template)
        surface_brain_prompt_for_member(
            params.team_repo,
            &ws,
            member_dir_name,
            params.team_name,
            params.github_repo,
            params.verbose,
            &mut events,
        );
    }

    Ok(TeamSyncResult {
        created,
        updated,
        failures,
        events,
    })
}

// ── Bridge provisioning ─────────────────────────────────────────────

fn provision_bridge(
    params: &TeamSyncParams,
    bridge_dir: &Option<PathBuf>,
    events: &mut Vec<TeamSyncEvent>,
) -> Result<()> {
    let bdir = match bridge_dir {
        Some(d) => d,
        None => {
            events.push(TeamSyncEvent::NoBridge);
            return Ok(());
        }
    };

    // Discover members for bridge provisioning
    let members_dir = params.team_repo.join("members");
    let mut bridge_members: Vec<bridge::BridgeMember> = Vec::new();
    if members_dir.is_dir() {
        for entry in fs::read_dir(&members_dir)? {
            let entry = entry?;
            if entry.file_type()?.is_dir() {
                bridge_members.push(bridge::BridgeMember {
                    name: entry.file_name().to_string_lossy().to_string(),
                    is_operator: false,
                });
            }
        }
    }

    // Add operator to bridge members
    if let Some(op) = params.manifest.operator.as_ref() {
        if !bridge_members.iter().any(|m| m.name == op.bridge_username) {
            bridge_members.push(bridge::BridgeMember {
                name: op.bridge_username.clone(),
                is_operator: true,
            });
        }
    }
    bridge_members.sort_by(|a, b| a.name.cmp(&b.name));

    let bstate_path = bridge::state_path(params.workzone, params.team_name);
    let mut b = Bridge::new(bdir.clone(), bstate_path.clone(), params.team_name.to_string())?;

    let cred_store = LocalCredentialStore::new(
        params.team_name,
        b.bridge_name(),
        bstate_path,
    )
    .with_collection(params.keyring_collection.clone());

    // Ensure local bridge is running before provisioning.
    // Always call start() — it's idempotent: health-checks first and
    // returns AlreadyRunning if healthy, or restarts if the container
    // died (e.g., after VM reboot while state still says "running").
    if b.is_local() {
        if which::which("just").is_err() {
            events.push(TeamSyncEvent::BridgeAutoStartSkipped {
                reason: "'just' not found. Cannot start bridge for provisioning. \
                         Install: https://just.systems/"
                    .to_string(),
            });
        } else {
            let result = b.start()?;
            events.push(TeamSyncEvent::BridgeAutoStart {
                name: b.bridge_name().to_string(),
                result,
            });
            b.save()?;
        }
    }

    // Provision identities
    let provision_result = b.provision(&bridge_members, &cred_store)?;
    for (name, member_result) in &provision_result.members {
        events.push(TeamSyncEvent::BridgeProvisionMember {
            name: name.clone(),
            result: member_result.clone(),
        });
    }
    if let Some(room_name) = &provision_result.room_created {
        events.push(TeamSyncEvent::BridgeRoomCreated(room_name.clone()));
    }
    b.save()?;
    events.push(TeamSyncEvent::BridgeSaved);

    Ok(())
}

// ── RObot injection ─────────────────────────────────────────────────

/// Pre-computed bridge context for RObot injection.
struct RobotContext {
    cred_store: LocalCredentialStore,
    bridge: Bridge,
}

fn build_robot_context(
    params: &TeamSyncParams,
    bridge_dir: &Option<PathBuf>,
) -> Result<Option<RobotContext>> {
    let bdir = match bridge_dir {
        Some(d) => d,
        None => return Ok(None),
    };
    let bstate_path = bridge::state_path(params.workzone, params.team_name);
    let b = Bridge::new(bdir.clone(), bstate_path.clone(), params.team_name.to_string())?;
    let store = LocalCredentialStore::new(params.team_name, b.bridge_name(), bstate_path)
        .with_collection(params.keyring_collection.clone());
    Ok(Some(RobotContext {
        cred_store: store,
        bridge: b,
    }))
}

fn surface_brain_prompt_for_member(
    team_repo: &Path,
    ws: &Path,
    member_dir_name: &str,
    team_name: &str,
    github_repo: Option<&str>,
    verbose: bool,
    events: &mut Vec<TeamSyncEvent>,
) {
    let (gh_org, gh_repo_name) = github_repo
        .and_then(brain::parse_github_repo)
        .map(|(org, repo)| (org.to_string(), repo.to_string()))
        .unwrap_or_default();

    let role = brain::read_member_role(team_repo, member_dir_name)
        .unwrap_or_default();
    let member_name = brain::read_member_name(team_repo, member_dir_name);

    // Read the template from the team repo root (extracted from profile)
    let vars = brain::BrainPromptVars {
        member_name,
        team_name: team_name.to_string(),
        role,
        gh_org,
        gh_repo: gh_repo_name,
    };

    match brain::surface_brain_prompt(team_repo, ws, &vars) {
        Ok(true) => {
            if verbose {
                events.push(TeamSyncEvent::BrainPromptSurfaced {
                    member: member_dir_name.to_string(),
                });
            }
        }
        Ok(false) => {} // No template in profile — skip silently
        Err(e) => {
            tracing::warn!(
                member = member_dir_name,
                error = %e,
                "Failed to surface brain prompt"
            );
        }
    }
}

fn inject_robot_for_member(
    ws: &Path,
    member_dir_name: &str,
    ctx: &RobotContext,
    params: &TeamSyncParams,
    events: &mut Vec<TeamSyncEvent>,
) -> Result<()> {
    let ralph_yml = ws.join("ralph.yml");
    if !ralph_yml.exists() {
        return Ok(());
    }

    let has_cred = bridge::resolve_credential_from_store(member_dir_name, &ctx.cred_store)?
        .is_some();

    // Build bridge config for RC/tuwunel bridges
    let bridge_config = {
        let bname = ctx.bridge.bridge_name();
        if bname == "rocketchat" || bname == "tuwunel" {
            let bot_user_id = ctx
                .bridge
                .member_user_id(member_dir_name)
                .unwrap_or_default();
            let room_id = ctx
                .bridge
                .default_room_id()
                .unwrap_or_default()
                .to_string();
            let server_url = ctx
                .bridge
                .service_url()
                .unwrap_or_default()
                .to_string();
            let operator_id = params
                .manifest
                .operator
                .as_ref()
                .and_then(|op| ctx.bridge.member_user_id(&op.bridge_username));

            Some(workspace::RobotBridgeConfig {
                bot_user_id,
                room_id,
                server_url,
                operator_id,
            })
        } else {
            None
        }
    };

    let bridge_type_name = Some(ctx.bridge.bridge_name().to_string());
    workspace::inject_robot_config(
        &ralph_yml,
        has_cred,
        bridge_type_name.as_deref(),
        bridge_config.as_ref(),
    )?;

    if params.verbose {
        events.push(TeamSyncEvent::RobotInjected {
            member: member_dir_name.to_string(),
            enabled: has_cred,
        });
    }

    Ok(())
}
