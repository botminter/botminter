use std::path::Path;

use anyhow::Result;

use crate::formation::start_members::StartResult;
use crate::formation::stop_members::StopResult;
use crate::formation::{
    CredentialDomain, EnvironmentStatus, Formation, KeyValueCredentialStore, MemberHandle,
    MemberStatus, SetupParams, StartParams, StopParams,
};

use super::{common, credential::LocalKeyValueCredentialStore};

/// Linux local formation — runs members as local processes on the operator's machine.
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
}
