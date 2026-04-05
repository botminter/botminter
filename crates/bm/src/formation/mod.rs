mod init;
mod launch;
pub mod lima;
pub mod local;
mod local_topology;
mod manager;
pub mod start_members;
pub mod stop_members;

pub use self::init::{register_team, setup_new_team_repo};
pub use self::launch::{
    check_robot_enabled_mismatch, is_brain_member, launch_brain, launch_ralph, BrainLaunchConfig,
};
pub use self::local::create_local_formation;
pub use self::local_topology::write_local_topology;
pub use self::manager::{run_formation_manager, FormationManagerResult};
pub use self::start_members::{
    auto_start_bridge, start_local_members, AppCredentialsCached, BridgeAutoStartOutcome,
    MemberLaunched, MemberSkipped, StartResult,
};
pub use self::stop_members::{stop_local_members, BridgeStopOutcome, MemberStopped, StopResult};

/// A member that failed during a start or stop operation.
pub struct MemberFailed {
    pub name: String,
    pub error: String,
}

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config::{BotminterConfig, TeamEntry};

// ── Formation trait ──────────────────────────────────────────────────

/// The deployment strategy abstraction. Manages environment, credentials,
/// credential delivery, and member lifecycle. Internal to the team — never
/// exposed to operators.
///
/// Implementations: `LinuxLocalFormation` (local processes, system keyring),
/// `MacosLocalFormation` (local processes, macOS Keychain), `LimaFormation` (VM-based, future).
pub trait Formation {
    /// Returns the formation name (e.g., "local", "lima", "k8s").
    fn name(&self) -> &str;

    // ── Environment ──────────────────────────────────────────────

    /// Prepares the environment for running members.
    /// Local: verifies prerequisites. Lima: creates VM. K8s: configures namespace.
    fn setup(&self, params: &SetupParams) -> Result<()>;

    /// Checks if the environment is ready.
    fn check_environment(&self) -> Result<EnvironmentStatus>;

    /// Checks hard prerequisites. Fails fast with actionable errors.
    fn check_prerequisites(&self) -> Result<()>;

    // ── Credentials ──────────────────────────────────────────────

    /// Returns a key-value credential store for the given domain.
    /// The store interface is simple: store(key, value) / retrieve(key).
    /// Each credential domain composes its own key conventions.
    fn credential_store(
        &self,
        domain: CredentialDomain,
    ) -> Result<Box<dyn KeyValueCredentialStore>>;

    /// One-time setup for token delivery to a member.
    /// Creates GH_CONFIG_DIR, writes initial config, configures git
    /// credential helper in workspace .git/config (not global .gitconfig).
    fn setup_token_delivery(&self, member: &str, workspace: &Path, bot_user: &str) -> Result<()>;

    /// Delivers a refreshed token to a member.
    /// Local: atomically writes hosts.yml. K8s: updates Secret.
    /// Called by the daemon on every token refresh cycle (every 50 min).
    fn refresh_token(&self, member: &str, workspace: &Path, token: &str) -> Result<()>;

    // ── Member lifecycle ─────────────────────────────────────────

    /// Starts members. Internally ensures daemon is running, generates
    /// tokens, delivers credentials, launches member processes.
    fn start_members(&self, params: &StartParams) -> Result<StartResult>;

    /// Stops members. Daemon keeps running unless all members stopped.
    fn stop_members(&self, params: &StopParams) -> Result<StopResult>;

    /// Returns status of all members including token health.
    fn member_status(&self) -> Result<Vec<MemberStatus>>;

    // ── Interactive access ───────────────────────────────────────

    /// Execute a command in the formation's environment.
    /// Local: exec directly. Lima: SSH into VM then exec.
    fn exec_in(&self, workspace: &Path, cmd: &[&str]) -> Result<()>;

    /// Open an interactive shell in the formation's environment.
    fn shell(&self) -> Result<()>;

    // ── Topology ─────────────────────────────────────────────────

    /// Writes a topology file recording where members are running.
    fn write_topology(
        &self,
        workzone: &Path,
        team_name: &str,
        members: &[(String, MemberHandle)],
    ) -> Result<()>;
}

// ── Key-value CredentialStore trait ───────────────────────────────────

/// A simple key-value secret store. Formation-neutral — formations provide
/// implementations (system keyring for local, K8s Secrets for k8s, etc.).
///
/// Key conventions are composed by each credential domain:
/// - Bridge: `{member}` → bridge token
/// - GitHubApp: `{member}/github-app-id`, `{member}/github-app-private-key`, etc.
pub trait KeyValueCredentialStore {
    /// Store a secret value under the given key.
    fn store(&self, key: &str, value: &str) -> Result<()>;

    /// Retrieve a secret value by key. Returns `None` if not found.
    fn retrieve(&self, key: &str) -> Result<Option<String>>;

    /// Remove a secret by key. No-op if the key doesn't exist.
    fn remove(&self, key: &str) -> Result<()>;

    /// List all keys matching a prefix.
    fn list_keys(&self, prefix: &str) -> Result<Vec<String>>;
}

// ── CredentialDomain ─────────────────────────────────────────────────

/// Determines which credential store implementation is returned and with
/// what configuration (keyring service name, K8s namespace, etc.).
pub enum CredentialDomain {
    /// Bridge credentials (e.g., Matrix/Telegram tokens).
    Bridge {
        team_name: String,
        bridge_name: String,
        state_path: PathBuf,
    },
    /// GitHub App credentials (App ID, Client ID, private key, installation ID).
    GitHubApp {
        team_name: String,
        member_name: String,
    },
}

// ── Supporting types ─────────────────────────────────────────────────

/// Parameters for `Formation::setup()`.
pub struct SetupParams {
    pub coding_agent: String,
    pub coding_agent_api_key: Option<String>,
}

/// Result of `Formation::check_environment()`.
pub struct EnvironmentStatus {
    pub ready: bool,
    pub checks: Vec<EnvironmentCheck>,
}

/// A single environment prerequisite check result.
pub struct EnvironmentCheck {
    pub name: String,
    pub passed: bool,
    pub detail: String,
}

/// Parameters for `Formation::start_members()`.
pub struct StartParams<'a> {
    pub team: &'a TeamEntry,
    pub config: &'a BotminterConfig,
    pub team_repo: &'a Path,
    pub member_filter: Option<&'a str>,
}

/// Parameters for `Formation::stop_members()`.
pub struct StopParams<'a> {
    pub team: &'a TeamEntry,
    pub config: &'a BotminterConfig,
    pub member_filter: Option<&'a str>,
    pub force: bool,
    pub bridge_flag: bool,
    /// When true, also shut down the daemon after stopping members.
    /// Corresponds to `bm stop --all`.
    pub stop_all: bool,
}

/// Status of a single member in the formation.
pub struct MemberStatus {
    pub name: String,
    pub running: bool,
    pub pid: Option<u32>,
    pub workspace: Option<PathBuf>,
    pub brain_mode: bool,
}

/// Handle to a running member. Opaque to commands — passed back to the
/// formation for topology writing and lifecycle operations.
pub enum MemberHandle {
    Local { pid: u32, workspace: PathBuf },
}

// ── InMemoryKeyValueCredentialStore (for testing) ────────────────────

/// In-memory key-value credential store for testing.
/// Avoids system keyring dependency.
pub struct InMemoryKeyValueCredentialStore {
    entries: std::sync::Mutex<std::collections::HashMap<String, String>>,
}

impl Default for InMemoryKeyValueCredentialStore {
    fn default() -> Self {
        Self {
            entries: std::sync::Mutex::new(std::collections::HashMap::new()),
        }
    }
}

impl InMemoryKeyValueCredentialStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl KeyValueCredentialStore for InMemoryKeyValueCredentialStore {
    fn store(&self, key: &str, value: &str) -> Result<()> {
        self.entries
            .lock()
            .unwrap()
            .insert(key.to_string(), value.to_string());
        Ok(())
    }

    fn retrieve(&self, key: &str) -> Result<Option<String>> {
        Ok(self.entries.lock().unwrap().get(key).cloned())
    }

    fn remove(&self, key: &str) -> Result<()> {
        self.entries.lock().unwrap().remove(key);
        Ok(())
    }

    fn list_keys(&self, prefix: &str) -> Result<Vec<String>> {
        let entries = self.entries.lock().unwrap();
        let mut keys: Vec<String> = entries
            .keys()
            .filter(|k| k.starts_with(prefix))
            .cloned()
            .collect();
        keys.sort();
        Ok(keys)
    }
}

/// Formation config parsed from `formation.yml`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct FormationConfig {
    pub name: String,
    pub description: String,
    #[serde(rename = "type")]
    pub formation_type: String,

    /// K8s-specific configuration (only for type=k8s).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub k8s: Option<K8sConfig>,

    /// Formation manager configuration (only for non-local types).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub manager: Option<ManagerConfig>,
}

/// K8s-specific formation settings.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct K8sConfig {
    pub context: String,
    pub image: String,
    #[serde(default = "default_namespace_prefix")]
    pub namespace_prefix: String,
}

fn default_namespace_prefix() -> String {
    "botminter".to_string()
}

/// Formation manager settings (Ralph session for deployment).
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ManagerConfig {
    pub ralph_yml: String,
    pub prompt: String,
    pub hats_dir: String,
}

impl FormationConfig {
    /// Returns true if this formation uses local process management.
    pub fn is_local(&self) -> bool {
        self.formation_type == "local"
    }
}

/// Resolves the formations directory for a team repo.
pub fn formations_dir(team_repo: &Path) -> PathBuf {
    team_repo.join("formations")
}

/// Loads a formation config from the team repo.
pub fn load(team_repo: &Path, formation_name: &str) -> Result<FormationConfig> {
    let formation_dir = formations_dir(team_repo).join(formation_name);
    let config_path = formation_dir.join("formation.yml");

    if !config_path.exists() {
        let available = list_formations(team_repo).unwrap_or_default();
        if available.is_empty() {
            bail!(
                "Formation '{}' not found in team repo. No formations directory exists.",
                formation_name
            );
        } else {
            bail!(
                "Formation '{}' not found in team repo. Available formations: {}",
                formation_name,
                available.join(", ")
            );
        }
    }

    let contents = fs::read_to_string(&config_path).with_context(|| {
        format!(
            "Failed to read formation config at {}",
            config_path.display()
        )
    })?;

    let config: FormationConfig = serde_yml::from_str(&contents).with_context(|| {
        format!(
            "Failed to parse formation config at {}",
            config_path.display()
        )
    })?;

    Ok(config)
}

/// Lists available formation names in the team repo.
pub fn list_formations(team_repo: &Path) -> Result<Vec<String>> {
    let dir = formations_dir(team_repo);
    if !dir.is_dir() {
        return Ok(Vec::new());
    }

    let mut names = Vec::new();
    for entry in fs::read_dir(&dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
}

/// Resolves the formation to use for `bm start`.
///
/// Resolution order:
/// 1. If `--formation` flag is specified, use that.
/// 2. If no flag, check for formations dir → default to "local".
/// 3. If no formations dir exists (v1 team), return None (legacy behavior).
pub fn resolve_formation(team_repo: &Path, flag: Option<&str>) -> Result<Option<String>> {
    match flag {
        Some(name) => {
            // Explicit flag — verify formation exists
            let dir = formations_dir(team_repo).join(name);
            if !dir.is_dir() {
                let available = list_formations(team_repo).unwrap_or_default();
                if available.is_empty() {
                    bail!(
                        "Formation '{}' not found in team repo. No formations directory exists.",
                        name
                    );
                } else {
                    bail!(
                        "Formation '{}' not found in team repo. Available formations: {}",
                        name,
                        available.join(", ")
                    );
                }
            }
            Ok(Some(name.to_string()))
        }
        None => {
            // No flag — check if formations dir exists
            let dir = formations_dir(team_repo);
            if dir.is_dir() {
                // v2 team: default to local
                Ok(Some("local".to_string()))
            } else {
                // v1 team or no formations: legacy behavior
                Ok(None)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn create_formation(tmp: &Path, name: &str, content: &str) {
        let dir = tmp.join("formations").join(name);
        fs::create_dir_all(&dir).unwrap();
        fs::write(dir.join("formation.yml"), content).unwrap();
    }

    #[test]
    fn load_local_formation() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "local",
            "name: local\ndescription: Run locally\ntype: local\n",
        );

        let config = load(tmp.path(), "local").unwrap();
        assert_eq!(config.name, "local");
        assert_eq!(config.formation_type, "local");
        assert!(config.is_local());
        assert!(config.k8s.is_none());
        assert!(config.manager.is_none());
    }

    #[test]
    fn load_k8s_formation() {
        let tmp = tempfile::tempdir().unwrap();
        let content = r#"
name: k8s
description: Deploy to k8s
type: k8s
k8s:
  context: kind-botminter
  image: ghcr.io/owner/ralph:latest
  namespace_prefix: botminter
manager:
  ralph_yml: ralph.yml
  prompt: PROMPT.md
  hats_dir: hats/
"#;
        create_formation(tmp.path(), "k8s", content);

        let config = load(tmp.path(), "k8s").unwrap();
        assert_eq!(config.name, "k8s");
        assert_eq!(config.formation_type, "k8s");
        assert!(!config.is_local());

        let k8s = config.k8s.unwrap();
        assert_eq!(k8s.context, "kind-botminter");
        assert_eq!(k8s.image, "ghcr.io/owner/ralph:latest");
        assert_eq!(k8s.namespace_prefix, "botminter");

        let mgr = config.manager.unwrap();
        assert_eq!(mgr.ralph_yml, "ralph.yml");
        assert_eq!(mgr.prompt, "PROMPT.md");
        assert_eq!(mgr.hats_dir, "hats/");
    }

    #[test]
    fn load_nonexistent_formation_errors() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "local",
            "name: local\ndescription: Run locally\ntype: local\n",
        );

        let result = load(tmp.path(), "nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("local")); // lists available
    }

    #[test]
    fn load_no_formations_dir_errors() {
        let tmp = tempfile::tempdir().unwrap();

        let result = load(tmp.path(), "local");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("local"));
        assert!(err.contains("No formations directory"));
    }

    #[test]
    fn list_formations_returns_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "k8s",
            "name: k8s\ndescription: K8s\ntype: k8s\n",
        );
        create_formation(
            tmp.path(),
            "local",
            "name: local\ndescription: Local\ntype: local\n",
        );

        let result = list_formations(tmp.path()).unwrap();
        assert_eq!(result, vec!["k8s", "local"]);
    }

    #[test]
    fn list_formations_empty_dir() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir_all(tmp.path().join("formations")).unwrap();

        let result = list_formations(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn list_formations_no_dir() {
        let tmp = tempfile::tempdir().unwrap();

        let result = list_formations(tmp.path()).unwrap();
        assert!(result.is_empty());
    }

    #[test]
    fn resolve_formation_explicit_flag() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "k8s",
            "name: k8s\ndescription: K8s\ntype: k8s\n",
        );

        let result = resolve_formation(tmp.path(), Some("k8s")).unwrap();
        assert_eq!(result, Some("k8s".to_string()));
    }

    #[test]
    fn resolve_formation_explicit_nonexistent_errors() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "local",
            "name: local\ndescription: Local\ntype: local\n",
        );

        let result = resolve_formation(tmp.path(), Some("nope"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nope"));
        assert!(err.contains("local"));
    }

    #[test]
    fn resolve_formation_no_flag_with_formations_dir() {
        let tmp = tempfile::tempdir().unwrap();
        create_formation(
            tmp.path(),
            "local",
            "name: local\ndescription: Local\ntype: local\n",
        );

        let result = resolve_formation(tmp.path(), None).unwrap();
        assert_eq!(result, Some("local".to_string()));
    }

    #[test]
    fn resolve_formation_no_flag_no_formations_dir() {
        let tmp = tempfile::tempdir().unwrap();

        let result = resolve_formation(tmp.path(), None).unwrap();
        assert_eq!(result, None);
    }

    // ── Formation trait + CredentialStore tests ────────────────────────

    #[test]
    fn formation_trait_is_object_safe() {
        // Compile-time check: Formation can be used as a trait object.
        fn _accepts_boxed(_f: Box<dyn super::Formation>) {}
    }

    #[test]
    fn credential_store_trait_is_object_safe() {
        // Compile-time check: KeyValueCredentialStore can be used as a trait object.
        fn _accepts_boxed(_s: Box<dyn super::KeyValueCredentialStore>) {}
    }

    #[test]
    fn in_memory_kv_store_and_retrieve() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("superman/github-app-id", "123").unwrap();
        assert_eq!(
            store.retrieve("superman/github-app-id").unwrap(),
            Some("123".to_string())
        );
    }

    #[test]
    fn in_memory_kv_store_retrieve_missing() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        assert_eq!(store.retrieve("nonexistent").unwrap(), None);
    }

    #[test]
    fn in_memory_kv_store_remove() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("key1", "val1").unwrap();
        store.remove("key1").unwrap();
        assert_eq!(store.retrieve("key1").unwrap(), None);
    }

    #[test]
    fn in_memory_kv_store_overwrite() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("key1", "old").unwrap();
        store.store("key1", "new").unwrap();
        assert_eq!(store.retrieve("key1").unwrap(), Some("new".to_string()));
    }

    #[test]
    fn in_memory_kv_store_list_keys_with_prefix() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("superman/github-app-id", "123").unwrap();
        store
            .store("superman/github-app-client-id", "Iv1.abc")
            .unwrap();
        store
            .store("superman/github-app-private-key", "PEM")
            .unwrap();
        store.store("batman/github-app-id", "456").unwrap();

        let keys = store.list_keys("superman/").unwrap();
        assert_eq!(
            keys,
            vec![
                "superman/github-app-client-id",
                "superman/github-app-id",
                "superman/github-app-private-key",
            ]
        );
    }

    #[test]
    fn in_memory_kv_store_list_keys_empty_prefix() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("a", "1").unwrap();
        store.store("b", "2").unwrap();
        let keys = store.list_keys("").unwrap();
        assert_eq!(keys, vec!["a", "b"]);
    }

    #[test]
    fn in_memory_kv_store_list_keys_no_match() {
        let store = super::InMemoryKeyValueCredentialStore::new();
        store.store("superman/id", "123").unwrap();
        let keys = store.list_keys("batman/").unwrap();
        assert!(keys.is_empty());
    }

    #[test]
    fn credential_domain_bridge_variant() {
        let domain = super::CredentialDomain::Bridge {
            team_name: "my-team".to_string(),
            bridge_name: "matrix".to_string(),
            state_path: PathBuf::from("/tmp/state.json"),
        };
        match domain {
            super::CredentialDomain::Bridge {
                team_name,
                bridge_name,
                state_path,
            } => {
                assert_eq!(team_name, "my-team");
                assert_eq!(bridge_name, "matrix");
                assert_eq!(state_path, PathBuf::from("/tmp/state.json"));
            }
            _ => panic!("Expected Bridge variant"),
        }
    }

    #[test]
    fn credential_domain_github_app_variant() {
        let domain = super::CredentialDomain::GitHubApp {
            team_name: "my-team".to_string(),
            member_name: "superman".to_string(),
        };
        match domain {
            super::CredentialDomain::GitHubApp {
                team_name,
                member_name,
            } => {
                assert_eq!(team_name, "my-team");
                assert_eq!(member_name, "superman");
            }
            _ => panic!("Expected GitHubApp variant"),
        }
    }

    #[test]
    fn k8s_namespace_prefix_default() {
        let tmp = tempfile::tempdir().unwrap();
        let content = r#"
name: k8s
description: K8s
type: k8s
k8s:
  context: kind-test
  image: test:latest
"#;
        create_formation(tmp.path(), "k8s", content);

        let config = load(tmp.path(), "k8s").unwrap();
        let k8s = config.k8s.unwrap();
        assert_eq!(k8s.namespace_prefix, "botminter");
    }
}
