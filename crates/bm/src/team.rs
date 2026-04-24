use anyhow::Result;

use crate::config::{BotminterConfig, TeamEntry};
use crate::daemon;
use crate::formation::start_members::StartResult;
use crate::formation::stop_members::StopResult;
use crate::formation::{self, BridgeStopOutcome, Formation, StartParams, StopParams};
use crate::state;
use crate::workspace;

/// Operator-facing API boundary wrapping a team entry and its formation.
///
/// Commands resolve a Team and call its methods instead of calling formation
/// free functions directly. The Team delegates to the formation trait, which
/// delegates to platform-specific implementations.
///
/// Bridge lifecycle is orchestrated here (not in the formation, not in the
/// command). The formation handles member lifecycle only per ADR-0008.
pub struct Team<'a> {
    entry: &'a TeamEntry,
    formation: Box<dyn Formation>,
}

/// Full outcome of `Team::stop()` — members, bridge, and daemon.
pub struct TeamStopResult {
    pub members: StopResult,
    pub bridge: Option<BridgeStopOutcome>,
    pub daemon_stopped: bool,
    /// True when an event source (polling/webhook) is configured and the
    /// daemon is still running. Used by the command to print a notice.
    pub daemon_events_active: bool,
}

impl<'a> Team<'a> {
    pub fn new(entry: &'a TeamEntry, formation: Box<dyn Formation>) -> Self {
        Self { entry, formation }
    }

    pub fn entry(&self) -> &TeamEntry {
        self.entry
    }

    pub fn formation(&self) -> &dyn Formation {
        &*self.formation
    }

    /// Start members via the formation.
    ///
    /// Bridge auto-start is NOT handled here — it's a command-layer concern.
    /// The formation starts members only (no bridge involvement).
    pub fn start(
        &self,
        config: &BotminterConfig,
        member_filter: Option<&str>,
    ) -> Result<StartResult> {
        let team_repo = self.entry.path.join("team");
        self.formation.start_members(&StartParams {
            team: self.entry,
            config,
            team_repo: &team_repo,
            member_filter,
        })
    }

    /// Stop members, bridge, and optionally the daemon.
    ///
    /// Orchestrates three concerns:
    /// 1. Member lifecycle — delegated to formation
    /// 2. Bridge lifecycle — domain operation (not a formation concern)
    /// 3. Daemon lifecycle:
    ///    - `--all` → always stop daemon
    ///    - No event source configured → auto-stop daemon (nothing to keep it alive)
    ///    - Event source active → leave daemon running, report via `daemon_events_active`
    pub fn stop(
        &self,
        config: &BotminterConfig,
        member_filter: Option<&str>,
        force: bool,
        bridge_flag: bool,
        stop_all: bool,
    ) -> Result<TeamStopResult> {
        let members = self.formation.stop_members(&StopParams {
            team: self.entry,
            config,
            member_filter,
            force,
        })?;

        let effective_bridge_flag = bridge_flag || stop_all;
        let bridge = if member_filter.is_none() {
            formation::stop_bridge(self.entry, config, effective_bridge_flag)?
        } else {
            None
        };

        // Daemon lifecycle: stop when --all, or when no event source keeps it useful.
        // When stopping a single member, leave the daemon alone.
        let has_events = self.entry.daemon.has_event_source();
        let should_stop_daemon = stop_all
            || (member_filter.is_none() && !has_events);

        let daemon_stopped = if should_stop_daemon {
            match daemon::query_status(&self.entry.name)? {
                daemon::DaemonStatusInfo::Running { .. } => {
                    daemon::stop_daemon(&self.entry.name)?;
                    true
                }
                daemon::DaemonStatusInfo::NotRunning { .. } => false,
            }
        } else {
            false
        };

        // When an event source is active and the daemon stays running,
        // the command should warn the operator.
        let daemon_events_active = has_events
            && !daemon_stopped
            && member_filter.is_none();

        Ok(TeamStopResult {
            members,
            bridge,
            daemon_stopped,
            daemon_events_active,
        })
    }

    /// Enable members for event-driven restart by the daemon.
    /// With `now=true`, also starts the members.
    pub fn enable(
        &self,
        config: &BotminterConfig,
        member_filter: Option<&str>,
        now: bool,
    ) -> Result<EnableResult> {
        let team_repo = self.entry.path.join("team");
        let mut runtime_state = state::load()?;

        let enabled = if let Some(member) = member_filter {
            let key = format!("{}/{}", self.entry.name, member);
            state::enable_member(&mut runtime_state, &key);
            vec![member.to_string()]
        } else {
            let members_dir = team_repo.join("members");
            let member_dirs = workspace::list_member_dirs(&members_dir)?;
            for m in &member_dirs {
                let key = format!("{}/{}", self.entry.name, m);
                state::enable_member(&mut runtime_state, &key);
            }
            member_dirs
        };
        state::save(&runtime_state)?;

        let start = if now {
            Some(self.start(config, member_filter)?)
        } else {
            None
        };

        Ok(EnableResult { enabled, start })
    }

    /// Disable members from event-driven restart by the daemon.
    /// With `now=true`, also stops the members.
    pub fn disable(
        &self,
        config: &BotminterConfig,
        member_filter: Option<&str>,
        now: bool,
    ) -> Result<DisableResult> {
        let team_repo = self.entry.path.join("team");
        let mut runtime_state = state::load()?;

        let disabled = if let Some(member) = member_filter {
            let key = format!("{}/{}", self.entry.name, member);
            state::disable_member(&mut runtime_state, &key);
            vec![member.to_string()]
        } else {
            let members_dir = team_repo.join("members");
            let member_dirs = workspace::list_member_dirs(&members_dir)?;
            for m in &member_dirs {
                let key = format!("{}/{}", self.entry.name, m);
                state::disable_member(&mut runtime_state, &key);
            }
            member_dirs
        };
        state::save(&runtime_state)?;

        let stop = if now {
            Some(self.formation.stop_members(&StopParams {
                team: self.entry,
                config,
                member_filter,
                force: false,
            })?)
        } else {
            None
        };

        Ok(DisableResult { disabled, stop })
    }
}

/// Outcome of `Team::enable()`.
pub struct EnableResult {
    pub enabled: Vec<String>,
    pub start: Option<StartResult>,
}

/// Outcome of `Team::disable()`.
pub struct DisableResult {
    pub disabled: Vec<String>,
    pub stop: Option<StopResult>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::{Path, PathBuf};

    use crate::config::{BridgeLifecycle, Credentials};
    use crate::formation::{
        CredentialDomain, EnvironmentCheck, EnvironmentStatus, KeyValueCredentialStore,
        MemberHandle, MemberStatus, SetupParams,
    };
    use crate::formation::start_members::MemberLaunched;
    use crate::formation::stop_members::MemberStopped;

    /// Mock formation that returns canned results for testing delegation.
    struct MockFormation {
        start_result: std::sync::Mutex<Option<StartResult>>,
        stop_result: std::sync::Mutex<Option<StopResult>>,
    }

    impl MockFormation {
        fn new() -> Self {
            Self {
                start_result: std::sync::Mutex::new(Some(StartResult {
                    launched: vec![MemberLaunched {
                        name: "superman".to_string(),
                        pid: 42,
                        brain_mode: false,
                    }],
                    skipped: vec![],
                    errors: vec![],
                    stale_cleaned: vec![],
                    bridge: None,
                })),
                stop_result: std::sync::Mutex::new(Some(StopResult {
                    stopped: vec![MemberStopped {
                        name: "superman".to_string(),
                        forced: false,
                        already_exited: false,
                    }],
                    errors: vec![],
                    no_members_running: false,
                    topology_removed: false,
                })),
            }
        }

        fn erroring() -> Self {
            Self {
                start_result: std::sync::Mutex::new(None),
                stop_result: std::sync::Mutex::new(None),
            }
        }
    }

    impl Formation for MockFormation {
        fn name(&self) -> &str {
            "mock"
        }

        fn setup(&self, _params: &SetupParams) -> Result<()> {
            Ok(())
        }

        fn check_environment(&self) -> Result<EnvironmentStatus> {
            Ok(EnvironmentStatus {
                ready: true,
                checks: vec![EnvironmentCheck {
                    name: "mock".to_string(),
                    passed: true,
                    detail: "mock check".to_string(),
                }],
            })
        }

        fn check_prerequisites(&self) -> Result<()> {
            Ok(())
        }

        fn credential_store(
            &self,
            _domain: CredentialDomain,
        ) -> Result<Box<dyn KeyValueCredentialStore>> {
            Ok(Box::new(crate::formation::InMemoryKeyValueCredentialStore::new()))
        }

        fn setup_token_delivery(
            &self,
            _member: &str,
            _workspace: &Path,
            _bot_user: &str,
        ) -> Result<()> {
            Ok(())
        }

        fn refresh_token(&self, _member: &str, _workspace: &Path, _token: &str) -> Result<()> {
            Ok(())
        }

        fn start_members(&self, _params: &StartParams) -> Result<StartResult> {
            self.start_result
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| anyhow::anyhow!("mock start error"))
        }

        fn stop_members(&self, _params: &StopParams) -> Result<StopResult> {
            self.stop_result
                .lock()
                .unwrap()
                .take()
                .ok_or_else(|| anyhow::anyhow!("mock stop error"))
        }

        fn member_status(&self) -> Result<Vec<MemberStatus>> {
            Ok(vec![])
        }

        fn exec_in(&self, _workspace: &Path, _cmd: &[&str]) -> Result<()> {
            Ok(())
        }

        fn shell(&self) -> Result<()> {
            Ok(())
        }

        fn write_topology(
            &self,
            _workzone: &Path,
            _team_name: &str,
            _members: &[(String, MemberHandle)],
        ) -> Result<()> {
            Ok(())
        }
    }

    fn test_team_entry() -> TeamEntry {
        TeamEntry {
            name: "test-team".to_string(),
            path: PathBuf::from("/tmp/test-team"),
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: "org/repo".to_string(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: BridgeLifecycle::default(),
            daemon: Default::default(),
            vm: None,
        }
    }

    fn test_config() -> BotminterConfig {
        BotminterConfig {
            workzone: PathBuf::from("/tmp/workzone"),
            default_team: None,
            teams: vec![],
            vms: vec![],
            keyring_collection: None,
        }
    }

    #[test]
    fn team_wraps_entry_and_formation() {
        let entry = test_team_entry();
        let formation = MockFormation::new();
        let team = Team::new(&entry, Box::new(formation));

        assert_eq!(team.entry().name, "test-team");
        assert_eq!(team.formation().name(), "mock");
    }

    #[test]
    fn team_start_delegates_to_formation() {
        let entry = test_team_entry();
        let formation = MockFormation::new();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.start(&config, None).unwrap();

        assert_eq!(result.launched.len(), 1);
        assert_eq!(result.launched[0].name, "superman");
        assert_eq!(result.launched[0].pid, 42);
        assert!(result.bridge.is_none(), "Team.start() should not handle bridge");
    }

    #[test]
    fn team_start_with_member_filter() {
        let entry = test_team_entry();
        let formation = MockFormation::new();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.start(&config, Some("superman")).unwrap();
        assert_eq!(result.launched.len(), 1);
    }

    #[test]
    fn team_stop_delegates_to_formation() {
        let entry = test_team_entry();
        let formation = MockFormation::new();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.stop(&config, None, false, false, false).unwrap();

        assert_eq!(result.members.stopped.len(), 1);
        assert_eq!(result.members.stopped[0].name, "superman");
        assert!(!result.members.stopped[0].forced);
        assert!(result.bridge.is_none());
        assert!(!result.daemon_events_active);
    }

    #[test]
    fn team_stop_with_force() {
        let entry = test_team_entry();
        let formation = MockFormation::new();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.stop(&config, None, true, false, false).unwrap();
        assert_eq!(result.members.stopped.len(), 1);
    }

    #[test]
    fn team_start_propagates_formation_error() {
        let entry = test_team_entry();
        let formation = MockFormation::erroring();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.start(&config, None);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("mock start error"));
    }

    #[test]
    fn team_stop_propagates_formation_error() {
        let entry = test_team_entry();
        let formation = MockFormation::erroring();
        let team = Team::new(&entry, Box::new(formation));
        let config = test_config();

        let result = team.stop(&config, None, false, false, false);
        assert!(result.is_err());
        assert!(result.err().unwrap().to_string().contains("mock stop error"));
    }

    #[test]
    fn team_is_generic_over_formation() {
        // Verify Team works with any Formation implementation
        let entry = test_team_entry();

        // With MockFormation
        let mock = MockFormation::new();
        let team = Team::new(&entry, Box::new(mock));
        assert_eq!(team.formation().name(), "mock");

        // With InMemory credential store formation (just checking type erasure works)
        let mock2 = MockFormation::new();
        let boxed: Box<dyn Formation> = Box::new(mock2);
        let team2 = Team::new(&entry, boxed);
        assert_eq!(team2.formation().name(), "mock");
    }
}
