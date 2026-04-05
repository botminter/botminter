use std::path::Path;

use anyhow::Result;

use crate::formation::start_members::StartResult;
use crate::formation::stop_members::StopResult;
use crate::formation::{
    CredentialDomain, EnvironmentStatus, Formation, KeyValueCredentialStore, MemberHandle,
    MemberStatus, SetupParams, StartParams, StopParams,
};

use super::{common, credential::LocalKeyValueCredentialStore};

/// macOS local formation — runs members as local processes on the operator's machine.
pub struct MacosLocalFormation {
    team_name: String,
}

impl MacosLocalFormation {
    pub fn new(team_name: &str) -> Self {
        Self {
            team_name: team_name.to_string(),
        }
    }
}

impl Formation for MacosLocalFormation {
    fn name(&self) -> &str {
        "local"
    }

    fn setup(&self, _params: &SetupParams) -> Result<()> {
        common::check_prerequisites()
    }

    fn check_environment(&self) -> Result<EnvironmentStatus> {
        common::check_environment()
    }

    fn check_prerequisites(&self) -> Result<()> {
        common::check_prerequisites()
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
                Ok(Box::new(LocalKeyValueCredentialStore::new(
                    service, keys_path,
                )))
            }
            CredentialDomain::GitHubApp {
                team_name,
                member_name: _,
            } => {
                let service = format!("botminter.{}.github-app", team_name);
                let config_dir = crate::config::config_dir()?;
                let keys_path =
                    config_dir.join(format!("credential-keys-{}-github-app.json", team_name));
                Ok(Box::new(LocalKeyValueCredentialStore::new(
                    service, keys_path,
                )))
            }
        }
    }

    fn setup_token_delivery(&self, _member: &str, workspace: &Path, bot_user: &str) -> Result<()> {
        common::setup_token_delivery(workspace, bot_user)
    }

    fn refresh_token(&self, _member: &str, workspace: &Path, token: &str) -> Result<()> {
        common::refresh_token(workspace, token)
    }

    fn start_members(&self, params: &StartParams) -> Result<StartResult> {
        common::start_members(&self.team_name, params)
    }

    fn stop_members(&self, params: &StopParams) -> Result<StopResult> {
        common::stop_members(params)
    }

    fn member_status(&self) -> Result<Vec<MemberStatus>> {
        common::member_status(&self.team_name)
    }

    fn exec_in(&self, workspace: &Path, cmd: &[&str]) -> Result<()> {
        common::exec_in(workspace, cmd)
    }

    fn shell(&self) -> Result<()> {
        common::shell()
    }

    fn write_topology(
        &self,
        workzone: &Path,
        team_name: &str,
        members: &[(String, MemberHandle)],
    ) -> Result<()> {
        common::write_topology(workzone, team_name, members)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn macos_formation_name_is_local() {
        let f = MacosLocalFormation::new("test-team");
        assert_eq!(f.name(), "local");
    }

    #[test]
    fn macos_formation_setup_checks_prerequisites() {
        let f = MacosLocalFormation::new("test-team");
        let params = SetupParams {
            coding_agent: "claude".to_string(),
            coding_agent_api_key: None,
        };
        let result = f.setup(&params);
        if which::which("ralph").is_ok() {
            result.unwrap();
        } else {
            assert!(result
                .unwrap_err()
                .to_string()
                .contains("'ralph' not found in PATH"));
        }
    }

    #[test]
    fn macos_formation_credential_store_returns_bridge_store() {
        let tmp = tempfile::tempdir().unwrap();
        let f = MacosLocalFormation::new("test-team");
        let domain = CredentialDomain::Bridge {
            team_name: "test-team".to_string(),
            bridge_name: "matrix".to_string(),
            state_path: tmp.path().join("bridge-state.json"),
        };
        let store = f.credential_store(domain).unwrap();
        let keys = store.list_keys("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn macos_formation_credential_store_returns_github_app_store() {
        let f = MacosLocalFormation::new("test-team");
        let domain = CredentialDomain::GitHubApp {
            team_name: "test-team".to_string(),
            member_name: "superman".to_string(),
        };
        let store = f.credential_store(domain).unwrap();
        let keys = store.list_keys("").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn macos_formation_setup_token_delivery_writes_workspace_config() {
        let f = MacosLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "[core]\n\tbare = false\n").unwrap();

        f.setup_token_delivery("superman", tmp.path(), "bot[bot]")
            .unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("user: bot[bot]"));

        let git_config = std::fs::read_to_string(tmp.path().join(".git/config")).unwrap();
        assert!(git_config.contains("[credential \"https://github.com\"]"));
        assert!(git_config.contains("auth git-credential"));
    }

    #[test]
    fn macos_formation_refresh_token_updates_hosts_yml() {
        let f = MacosLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(tmp.path().join(".git")).unwrap();
        std::fs::write(tmp.path().join(".git/config"), "").unwrap();

        f.setup_token_delivery("superman", tmp.path(), "bot[bot]")
            .unwrap();
        f.refresh_token("superman", tmp.path(), "ghs_token")
            .unwrap();

        let hosts = std::fs::read_to_string(tmp.path().join(".config/gh/hosts.yml")).unwrap();
        assert!(hosts.contains("oauth_token: ghs_token"));
        assert!(!hosts.contains("placeholder"));
    }

    #[test]
    fn macos_formation_shell_returns_error() {
        let f = MacosLocalFormation::new("test-team");
        let err = f.shell().unwrap_err();
        assert!(err.to_string().contains("not applicable"));
    }

    #[test]
    fn macos_formation_exec_in_empty_cmd_returns_error() {
        let f = MacosLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        let err = f.exec_in(tmp.path(), &[]).unwrap_err();
        assert!(err.to_string().contains("No command specified"));
    }

    #[test]
    fn macos_formation_exec_in_runs_command() {
        let f = MacosLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        f.exec_in(tmp.path(), &["true"]).unwrap();
    }

    #[test]
    fn macos_formation_exec_in_reports_failure() {
        let f = MacosLocalFormation::new("test-team");
        let tmp = tempfile::tempdir().unwrap();
        let err = f.exec_in(tmp.path(), &["false"]).unwrap_err();
        assert!(err.to_string().contains("exited with status"));
    }

    #[test]
    fn macos_formation_check_environment_returns_status() {
        let f = MacosLocalFormation::new("test-team");
        let status = f.check_environment().unwrap();
        assert_eq!(status.checks.len(), 2);
        assert_eq!(status.checks[0].name, "ralph");
        assert_eq!(status.checks[1].name, "just");
    }

    #[test]
    fn macos_formation_is_object_safe() {
        let f: Box<dyn Formation> = Box::new(MacosLocalFormation::new("test-team"));
        assert_eq!(f.name(), "local");
    }
}
