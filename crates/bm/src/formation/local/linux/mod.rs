pub(crate) mod credential;

use std::fs;
use std::io::Write;
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;
use std::process::Command;

use anyhow::{bail, Context, Result};

use crate::daemon::{self, DaemonClient};
use crate::formation::{
    self, CredentialDomain, EnvironmentStatus, EnvironmentCheck, Formation,
    KeyValueCredentialStore, MemberHandle, MemberStatus, SetupParams, StartParams, StopParams,
};
use crate::formation::start_members::StartResult;
use crate::formation::stop_members::{MemberStopped, StopResult};
use crate::state;

/// Linux local formation — runs members as local processes on the operator's machine.
///
/// Delegates to `stop_local_members` and `write_local_topology` for member lifecycle.
/// Member launch is handled exclusively via the daemon sessions API — `start_members`
/// bails immediately to enforce the ephemeral session model.
pub struct LinuxLocalFormation {
    team_name: String,
}

impl LinuxLocalFormation {
    pub fn new(team_name: &str) -> Self {
        Self {
            team_name: team_name.to_string(),
        }
    }
}

impl Formation for LinuxLocalFormation {
    fn name(&self) -> &str {
        "local"
    }

    fn setup(&self, _params: &SetupParams) -> Result<()> {
        // Local formation: verify prerequisites are met. No environment to create.
        self.check_prerequisites()
    }

    fn check_environment(&self) -> Result<EnvironmentStatus> {
        let ralph_installed = which::which("ralph").is_ok();
        let just_installed = which::which("just").is_ok();

        let checks = vec![
            EnvironmentCheck {
                name: "ralph".to_string(),
                passed: ralph_installed,
                detail: if ralph_installed {
                    "ralph-orchestrator found in PATH".to_string()
                } else {
                    "ralph-orchestrator not found in PATH. Install it first.".to_string()
                },
            },
            EnvironmentCheck {
                name: "just".to_string(),
                passed: just_installed,
                detail: if just_installed {
                    "just command runner found in PATH".to_string()
                } else {
                    "just not found in PATH. Required for bridge lifecycle.".to_string()
                },
            },
        ];

        let ready = checks.iter().all(|c| c.passed);
        Ok(EnvironmentStatus { ready, checks })
    }

    fn check_prerequisites(&self) -> Result<()> {
        if which::which("ralph").is_err() {
            bail!("'ralph' not found in PATH. Install ralph-orchestrator first.");
        }
        Ok(())
    }

    fn credential_store(
        &self,
        domain: CredentialDomain,
    ) -> Result<Box<dyn KeyValueCredentialStore>> {
        match domain {
            CredentialDomain::Bridge {
                team_name,
                bridge_name,
                state_path,
            } => {
                let service = format!("botminter.{}.{}", team_name, bridge_name);
                let keys_path = state_path
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("credential-keys.json");
                Ok(Box::new(credential::LocalKeyValueCredentialStore::new(
                    service, keys_path,
                )))
            }
            CredentialDomain::GitHubApp {
                team_name,
                member_name: _,
            } => {
                let service = format!("botminter.{}.github-app", team_name);
                let config_dir = crate::config::config_dir()?;
                let keys_path = config_dir.join(format!(
                    "credential-keys-{}-github-app.json",
                    team_name
                ));
                Ok(Box::new(credential::LocalKeyValueCredentialStore::new(
                    service, keys_path,
                )))
            }
        }
    }

    fn setup_token_delivery(
        &self,
        _member: &str,
        workspace: &Path,
        bot_user: &str,
    ) -> Result<()> {
        // Create GH_CONFIG_DIR at {workspace}/.config/gh/
        let gh_config_dir = workspace.join(".config").join("gh");
        fs::create_dir_all(&gh_config_dir)
            .with_context(|| format!("Failed to create {}", gh_config_dir.display()))?;

        // Write an initial hosts.yml with a placeholder token.
        // The actual token is written by refresh_token() immediately after.
        let hosts_yml = gh_config_dir.join("hosts.yml");
        let hosts_content = format!(
            "github.com:\n    user: {bot_user}\n    oauth_token: placeholder\n    git_protocol: https\n"
        );
        fs::write(&hosts_yml, &hosts_content)
            .with_context(|| format!("Failed to write {}", hosts_yml.display()))?;

        // Configure git credential helper in {workspace}/.git/config (NOT global).
        // This tells git to use `gh auth git-credential` which reads from GH_CONFIG_DIR.
        let git_config = workspace.join(".git").join("config");
        if git_config.exists() {
            let existing = fs::read_to_string(&git_config)
                .with_context(|| format!("Failed to read {}", git_config.display()))?;

            // Only add if not already configured
            if !existing.contains("[credential \"https://github.com\"]") {
                let credential_block = "\n[credential \"https://github.com\"]\n\thelper = \n\thelper = !/usr/bin/gh auth git-credential\n";
                let mut file = fs::OpenOptions::new()
                    .append(true)
                    .open(&git_config)
                    .with_context(|| format!("Failed to open {}", git_config.display()))?;
                file.write_all(credential_block.as_bytes())
                    .with_context(|| format!("Failed to append to {}", git_config.display()))?;
            }
        }

        Ok(())
    }

    fn refresh_token(&self, _member: &str, workspace: &Path, token: &str) -> Result<()> {
        let gh_config_dir = workspace.join(".config").join("gh");
        let hosts_yml = gh_config_dir.join("hosts.yml");

        // Read existing hosts.yml to preserve bot_user
        let existing = fs::read_to_string(&hosts_yml)
            .with_context(|| format!("Failed to read {}", hosts_yml.display()))?;

        // Extract bot_user from existing file
        let bot_user = existing
            .lines()
            .find_map(|line| {
                let trimmed = line.trim();
                trimmed.strip_prefix("user: ")
            })
            .unwrap_or("bot");

        let hosts_content = format!(
            "github.com:\n    user: {bot_user}\n    oauth_token: {token}\n    git_protocol: https\n"
        );

        // Atomic write: write to temp file, then rename
        let tmp_path = gh_config_dir.join("hosts.yml.tmp");
        fs::OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&tmp_path)
            .with_context(|| format!("Failed to create {}", tmp_path.display()))?
            .write_all(hosts_content.as_bytes())
            .with_context(|| format!("Failed to write {}", tmp_path.display()))?;

        fs::rename(&tmp_path, &hosts_yml)
            .with_context(|| format!("Failed to rename {} → {}", tmp_path.display(), hosts_yml.display()))?;

        Ok(())
    }

    fn start_members(&self, _params: &StartParams) -> Result<StartResult> {
        // Permanent workspace member launch is eradicated — use `bm start` which
        // routes through the daemon sessions API (POST /api/sessions/start).
        bail!("permanent workspace member launch is removed; use the daemon sessions API")
    }

    fn stop_members(&self, params: &StopParams) -> Result<StopResult> {
        // Route through daemon HTTP API (symmetric with start_members).
        // The daemon owns state.json — all state mutations go through it
        // per ADR-0008. Fall back to direct stop if daemon is unreachable.
        match DaemonClient::connect(&self.team_name) {
            Ok(client) => {
                let req = daemon::StopMembersRequest {
                    member: params.member_filter.map(|s| s.to_string()),
                    force: params.force,
                };
                let resp = client.stop_members(&req)?;

                Ok(StopResult {
                    stopped: resp
                        .stopped
                        .into_iter()
                        .map(|m| MemberStopped {
                            name: m.name,
                            already_exited: m.already_exited,
                            forced: m.forced,
                        })
                        .collect(),
                    errors: resp
                        .errors
                        .into_iter()
                        .map(|m| formation::MemberFailed {
                            name: m.name,
                            error: m.error,
                        })
                        .collect(),
                    no_members_running: resp.no_members_running,
                    topology_removed: false,
                })
            }
            Err(_) => {
                // Daemon not running — fall back to direct stop.
                formation::stop_local_members(
                    params.team,
                    params.config,
                    params.member_filter,
                    params.force,
                )
            }
        }
    }

    fn member_status(&self) -> Result<Vec<MemberStatus>> {
        let runtime_state = state::load()?;
        let team_prefix = format!("{}/", self.team_name);

        let statuses = runtime_state
            .members
            .iter()
            .filter(|(key, _)| key.starts_with(&team_prefix))
            .map(|(key, rt)| {
                let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);
                MemberStatus {
                    name: member_name.to_string(),
                    running: state::is_alive(rt.pid),
                    pid: Some(rt.pid),
                    workspace: Some(rt.workspace.clone()),
                    brain_mode: rt.brain_mode,
                }
            })
            .collect();

        Ok(statuses)
    }

    fn exec_in(&self, workspace: &Path, cmd: &[&str]) -> Result<()> {
        if cmd.is_empty() {
            bail!("No command specified");
        }

        let status = Command::new(cmd[0])
            .args(&cmd[1..])
            .current_dir(workspace)
            .status()?;

        if !status.success() {
            bail!(
                "Command '{}' exited with status {}",
                cmd.join(" "),
                status.code().unwrap_or(-1)
            );
        }

        Ok(())
    }

    fn shell(&self) -> Result<()> {
        // Local formation: the operator is already in the local environment.
        // This is a no-op — unlike Lima/K8s where you'd SSH into the VM or exec into a pod.
        bail!(
            "shell() is not applicable for local formation — \
             you are already in the local environment"
        )
    }

    fn write_topology(
        &self,
        workzone: &Path,
        team_name: &str,
        _members: &[(String, MemberHandle)],
    ) -> Result<()> {
        // Delegate to existing free function which reads from RuntimeState.
        let runtime_state = state::load()?;
        formation::write_local_topology(workzone, team_name, &runtime_state)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn linux_formation_name_is_local() {
        let f = LinuxLocalFormation::new("test-team");
        assert_eq!(f.name(), "local");
    }

    #[test]
    fn linux_formation_is_object_safe() {
        let f: Box<dyn Formation> = Box::new(LinuxLocalFormation::new("test-team"));
        assert_eq!(f.name(), "local");
    }

    #[test]
    fn setup_token_delivery_creates_gh_config_dir_and_hosts_yml() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();

        // Create a .git/config so credential helper can be appended
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "[core]\n\tbare = false\n").unwrap();

        f.setup_token_delivery("superman", tmp.path(), "test-bot[bot]")
            .unwrap();

        // Verify GH config dir created
        let gh_dir = tmp.path().join(".config/gh");
        assert!(gh_dir.exists(), "GH config dir should exist");

        // Verify hosts.yml written with bot user
        let hosts = std::fs::read_to_string(gh_dir.join("hosts.yml")).unwrap();
        assert!(hosts.contains("user: test-bot[bot]"));
        assert!(hosts.contains("git_protocol: https"));

        // Verify credential helper appended to .git/config
        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        assert!(git_config.contains("[credential \"https://github.com\"]"));
        assert!(git_config.contains("gh auth git-credential"));
    }

    #[test]
    fn setup_token_delivery_idempotent_credential_helper() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "[core]\n\tbare = false\n").unwrap();

        // Call twice — credential helper should only be added once
        f.setup_token_delivery("superman", tmp.path(), "bot").unwrap();
        f.setup_token_delivery("superman", tmp.path(), "bot").unwrap();

        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        let count = git_config.matches("[credential \"https://github.com\"]").count();
        assert_eq!(count, 1, "credential helper should be added only once");
    }

    #[test]
    fn refresh_token_atomically_updates_hosts_yml() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();

        // Setup first
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "").unwrap();
        f.setup_token_delivery("superman", tmp.path(), "my-bot[bot]").unwrap();

        // Refresh with a real token
        f.refresh_token("superman", tmp.path(), "ghs_installation_token_abc")
            .unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("oauth_token: ghs_installation_token_abc"));
        assert!(hosts.contains("user: my-bot[bot]"), "bot user should be preserved");
        assert!(!hosts.contains("placeholder"), "placeholder token should be replaced");

        // Verify tmp file is cleaned up (rename removes it)
        assert!(!tmp.path().join(".config/gh/hosts.yml.tmp").exists());
    }

    #[test]
    fn refresh_token_subsequent_updates_replace_token() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();

        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "").unwrap();
        f.setup_token_delivery("superman", tmp.path(), "bot").unwrap();

        f.refresh_token("superman", tmp.path(), "ghs_first").unwrap();
        f.refresh_token("superman", tmp.path(), "ghs_second").unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("ghs_second"));
        assert!(!hosts.contains("ghs_first"));
    }

    #[test]
    fn linux_formation_credential_store_returns_bridge_store() {
        let tmp = tempfile::tempdir().unwrap();
        let state_path = tmp.path().join("bridge-state.json");
        let f = LinuxLocalFormation::new("test-team");
        let domain = CredentialDomain::Bridge {
            team_name: "test-team".to_string(),
            bridge_name: "matrix".to_string(),
            state_path,
        };
        let store = f.credential_store(domain).unwrap();
        // Verify the store is functional by listing keys (empty initially)
        let keys = store.list_keys("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn linux_formation_credential_store_returns_github_app_store() {
        let f = LinuxLocalFormation::new("test-team");
        let domain = CredentialDomain::GitHubApp {
            team_name: "test-team".to_string(),
            member_name: "superman".to_string(),
        };
        let store = f.credential_store(domain).unwrap();
        let keys = store.list_keys("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn linux_formation_shell_returns_error() {
        let f = LinuxLocalFormation::new("test-team");
        let err = f.shell().unwrap_err();
        assert!(err.to_string().contains("not applicable"));
    }

    #[test]
    fn linux_formation_exec_in_empty_cmd_returns_error() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        let err = f.exec_in(tmp.path(), &[]).unwrap_err();
        assert!(err.to_string().contains("No command specified"));
    }

    #[test]
    fn linux_formation_exec_in_runs_command() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        // Run a simple command that should succeed
        f.exec_in(tmp.path(), &["true"]).unwrap();
    }

    #[test]
    fn linux_formation_exec_in_reports_failure() {
        let f = LinuxLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        let err = f.exec_in(tmp.path(), &["false"]).unwrap_err();
        assert!(err.to_string().contains("exited with status"));
    }

    #[test]
    fn linux_formation_check_environment_returns_status() {
        let f = LinuxLocalFormation::new("test-team");
        let status = f.check_environment().unwrap();
        // At minimum, the checks vector should contain ralph and just entries
        assert_eq!(status.checks.len(), 2);
        assert_eq!(status.checks[0].name, "ralph");
        assert_eq!(status.checks[1].name, "just");
    }
}
