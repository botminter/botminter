//! Member lifecycle domain operations: hire and fire.
//!
//! Single entry point for adding and removing members from a team.
//! Composes profile extraction, credential management, bridge identity,
//! and GitHub repo operations into cohesive lifecycle methods.

use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::bridge;
use crate::config::{BotminterConfig, TeamEntry};
use crate::formation::{self, CredentialDomain, Formation};
use crate::git::{self, app_auth, manifest_flow};
use crate::profile;

// ── Hire ─────────────────────────────────────────────────────────────

/// Parameters for hiring a member.
pub struct HireParams<'a> {
    pub team: &'a TeamEntry,
    pub role: &'a str,
    pub name: Option<&'a str>,
    pub app_credentials: Option<AppCredentials>,
    pub save_credentials_path: Option<&'a str>,
}

/// Result of hiring a member.
pub struct HireResult {
    pub member_dir_name: String,
    pub member_name: String,
    pub already_existed: bool,
    pub app_credentials_stored: bool,
    pub repos_checked: Vec<String>,
    pub credentials_saved_to: Option<String>,
}

/// Hires a member: extracts profile skeleton, stores App credentials if provided.
pub fn hire_member(params: &HireParams) -> Result<HireResult> {
    let team_repo = params.team.path.join("team");

    // Read and validate manifest
    let manifest: profile::ProfileManifest = {
        let contents = fs::read_to_string(team_repo.join("botminter.yml"))
            .context("Failed to read team repo's botminter.yml")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };
    profile::check_schema_version(&params.team.profile, &manifest.schema_version)?;
    let coding_agent = profile::resolve_coding_agent(params.team, &manifest)?;

    // Profile extraction (creates member dir, renders placeholders)
    let profile_result = profile::hire_member(
        &team_repo,
        &params.team.profile,
        params.role,
        params.name,
        coding_agent,
    )?;

    // When member already exists and no credentials provided, return early.
    // The caller (commands/hire.rs) will run the manifest flow afterward if needed.

    // App credential setup (if provided)
    let mut app_credentials_stored = false;
    let mut repos_checked = Vec::new();
    let mut credentials_saved_to = None;

    if let Some(ref creds) = params.app_credentials {
        let app_result = setup_app_credentials(
            params.team,
            &profile_result.member_dir_name,
            creds,
            params.save_credentials_path,
        )?;
        app_credentials_stored = app_result.credentials_stored;
        repos_checked = app_result.repos_checked;
        credentials_saved_to = app_result.credentials_saved_to;
    }

    Ok(HireResult {
        member_dir_name: profile_result.member_dir_name,
        member_name: profile_result.member_name,
        already_existed: profile_result.already_existed,
        app_credentials_stored,
        repos_checked,
        credentials_saved_to,
    })
}

/// Pre-generated GitHub App credentials for `--reuse-app` hire flow.
pub struct AppCredentials {
    pub app_id: String,
    pub client_id: String,
    pub private_key: String,
    pub installation_id: String,
}

/// Result of setting up GitHub App credentials for a hired member.
pub struct AppSetupResult {
    pub credentials_stored: bool,
    pub repos_checked: Vec<String>,
    pub credentials_saved_to: Option<String>,
}

/// Stores GitHub App credentials for a member and ensures the
/// App has access to all team + project repos.
pub fn setup_app_credentials(
    team: &TeamEntry,
    member_name: &str,
    creds: &AppCredentials,
    save_path: Option<&str>,
) -> Result<AppSetupResult> {
    let formation = formation::create_local_formation(&team.name)?;
    let cred_store = formation.credential_store(CredentialDomain::GitHubApp {
        team_name: team.name.clone(),
        member_name: member_name.to_string(),
    })?;

    let pre_gen = manifest_flow::PreGeneratedCredentials {
        app_id: creds.app_id.clone(),
        client_id: creds.client_id.clone(),
        private_key: creds.private_key.clone(),
        installation_id: creds.installation_id.clone(),
    };

    manifest_flow::store_pregenerated_credentials(cred_store.as_ref(), member_name, &pre_gen)?;

    // Ensure the App has access to team repo + project repos
    let repos = manifest_flow::collect_team_repos(team);
    let repo_refs: Vec<&str> = repos.iter().map(|s| s.as_str()).collect();
    if !repo_refs.is_empty() {
        manifest_flow::ensure_app_on_repos(
            &creds.installation_id,
            &creds.client_id,
            &creds.private_key,
            &repo_refs,
        )?;
    }

    let mut credentials_saved_to = None;
    if let Some(path) = save_path {
        manifest_flow::save_credentials_to_file(path, member_name, &pre_gen)?;
        credentials_saved_to = Some(path.to_string());
    }

    Ok(AppSetupResult {
        credentials_stored: true,
        repos_checked: repos,
        credentials_saved_to,
    })
}

// ── Fire: member teardown ───────────────────────────────────────────

/// Parameters for firing a member.
pub struct FireParams<'a> {
    pub team: &'a TeamEntry,
    pub config: &'a BotminterConfig,
    pub member: &'a str,
    pub keep_app: bool,
    pub delete_repo: bool,
}

/// Result of firing a member. Each field indicates whether the step succeeded.
/// Failed steps are collected in `errors` with the step name and error message.
pub struct FireResult {
    pub stopped: bool,
    pub app_uninstalled: bool,
    pub credentials_removed: bool,
    pub bridge_identity_removed: bool,
    pub member_dir_removed: bool,
    pub workspace_removed: bool,
    pub repo_deleted: bool,
    pub errors: Vec<FireError>,
}

pub struct FireError {
    pub step: &'static str,
    pub error: String,
}

/// Executes the full member teardown sequence. Each step is best-effort —
/// failure in one step does not block subsequent steps.
pub fn fire_member(params: &FireParams, formation: &dyn Formation) -> Result<FireResult> {
    let mut result = FireResult {
        stopped: false,
        app_uninstalled: false,
        credentials_removed: false,
        bridge_identity_removed: false,
        member_dir_removed: false,
        workspace_removed: false,
        repo_deleted: false,
        errors: Vec::new(),
    };

    // Step 1: Stop the member
    match stop_member(params.team, params.config, params.member, formation) {
        Ok(stopped) => result.stopped = stopped,
        Err(e) => result.errors.push(FireError {
            step: "stop",
            error: format!("{e}"),
        }),
    }

    // Step 2: Uninstall GitHub App (unless keep_app)
    if !params.keep_app {
        match uninstall_app(params.team, params.member) {
            Ok(uninstalled) => result.app_uninstalled = uninstalled,
            Err(e) => result.errors.push(FireError {
                step: "uninstall_app",
                error: format!("{e}"),
            }),
        }
    }

    // Step 3: Remove App credentials from keyring
    match remove_app_credentials(params.team, params.member) {
        Ok(()) => result.credentials_removed = true,
        Err(e) => result.errors.push(FireError {
            step: "remove_credentials",
            error: format!("{e}"),
        }),
    }

    // Step 4: Remove bridge identity
    let team_repo = params.team.path.join("team");
    match remove_bridge_identity(&team_repo, params.team, params.config, params.member) {
        Ok(removed) => result.bridge_identity_removed = removed,
        Err(e) => result.errors.push(FireError {
            step: "remove_bridge_identity",
            error: format!("{e}"),
        }),
    }

    // Step 5: Remove member directory from team repo and commit
    let member_dir = team_repo.join("members").join(params.member);
    if member_dir.is_dir() {
        match fs::remove_dir_all(&member_dir) {
            Ok(()) => {
                // Commit the removal so the team repo stays consistent
                let member_rel = format!("members/{}/", params.member);
                let _ = crate::git::run_git(&team_repo, &["add", &member_rel]);
                let commit_msg = format!("feat: fire {}", params.member);
                let _ = crate::git::run_git(&team_repo, &["commit", "-m", &commit_msg]);
                result.member_dir_removed = true;
            }
            Err(e) => result.errors.push(FireError {
                step: "remove_member_dir",
                error: format!("{e}"),
            }),
        }
    }

    // Step 6: Remove member workspace
    let workspace_dir = params.config.workzone.join(&params.team.name).join(params.member);
    if workspace_dir.is_dir() {
        match fs::remove_dir_all(&workspace_dir) {
            Ok(()) => result.workspace_removed = true,
            Err(e) => result.errors.push(FireError {
                step: "remove_workspace",
                error: format!("{e}"),
            }),
        }
    }

    // Step 7: Delete GitHub workspace repo (conditional)
    if params.delete_repo {
        if let Some(org) = params.team.github_repo.split('/').next().filter(|s| !s.is_empty()) {
            let ws_repo_name = format!("{}/{}-{}", org, params.team.name, params.member);
            match git::delete_repo(&ws_repo_name) {
                Ok(()) => result.repo_deleted = true,
                Err(e) => result.errors.push(FireError {
                    step: "delete_repo",
                    error: format!("{e}"),
                }),
            }
        }
    }

    Ok(result)
}

// ── Private helpers ─────────────────────────────────────────────────

fn stop_member(
    team: &TeamEntry,
    config: &BotminterConfig,
    member: &str,
    formation: &dyn Formation,
) -> Result<bool> {
    let result = formation.stop_members(&crate::formation::StopParams {
        team,
        config,
        member_filter: Some(member),
        force: false,
    })?;

    if result.no_members_running {
        return Ok(false);
    }
    if !result.errors.is_empty() {
        anyhow::bail!("{}", result.errors[0].error);
    }
    Ok(true)
}

fn uninstall_app(team: &TeamEntry, member: &str) -> Result<bool> {
    let formation = formation::create_local_formation(&team.name)?;
    let cred_store = formation.credential_store(CredentialDomain::GitHubApp {
        team_name: team.name.clone(),
        member_name: member.to_string(),
    })?;

    let client_id = cred_store.retrieve(&manifest_flow::credential_keys::client_id(member))?;
    let private_key = cred_store.retrieve(&manifest_flow::credential_keys::private_key(member))?;
    let installation_id = cred_store.retrieve(&manifest_flow::credential_keys::installation_id(member))?;

    match (client_id, private_key, installation_id) {
        (Some(cid), Some(key), Some(iid)) => {
            let inst_id: u64 = iid.parse().context("Invalid installation ID")?;
            let jwt = app_auth::generate_jwt(&cid, &key)?;
            app_auth::uninstall_app(&jwt, inst_id)?;
            Ok(true)
        }
        _ => Ok(false), // No credentials found
    }
}

fn remove_app_credentials(team: &TeamEntry, member: &str) -> Result<()> {
    let formation = formation::create_local_formation(&team.name)?;
    let cred_store = formation.credential_store(CredentialDomain::GitHubApp {
        team_name: team.name.clone(),
        member_name: member.to_string(),
    })?;
    manifest_flow::remove_member_credentials(cred_store.as_ref(), member)
}

fn remove_bridge_identity(
    team_repo: &Path,
    team: &TeamEntry,
    config: &BotminterConfig,
    member: &str,
) -> Result<bool> {
    let bridge_dir = match bridge::discover(team_repo, &team.name)? {
        Some(d) => d,
        None => return Ok(false),
    };

    let state_path = bridge::state_path(&config.workzone, &team.name);
    let mut b = bridge::Bridge::new(bridge_dir, state_path, team.name.clone())?;

    if b.identities().contains_key(member) {
        b.remove_identity(member);
        b.save()?;
        Ok(true)
    } else {
        Ok(false)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fire_result_tracks_all_steps() {
        let result = FireResult {
            stopped: true,
            app_uninstalled: true,
            credentials_removed: true,
            bridge_identity_removed: false,
            member_dir_removed: true,
            workspace_removed: true,
            repo_deleted: false,
            errors: vec![FireError {
                step: "remove_bridge_identity",
                error: "no bridge".to_string(),
            }],
        };

        assert!(result.stopped);
        assert!(result.app_uninstalled);
        assert!(!result.bridge_identity_removed);
        assert_eq!(result.errors.len(), 1);
        assert_eq!(result.errors[0].step, "remove_bridge_identity");
    }

    #[test]
    fn app_credentials_struct() {
        let creds = AppCredentials {
            app_id: "123".to_string(),
            client_id: "Iv1.abc".to_string(),
            private_key: "PEM".to_string(),
            installation_id: "456".to_string(),
        };
        assert_eq!(creds.app_id, "123");
        assert_eq!(creds.installation_id, "456");
    }
}
