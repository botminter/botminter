use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

const CONFIG_DIR: &str = ".botminter";
const CONFIG_FILE: &str = "config.yml";
const CONFIG_PERMISSIONS: u32 = 0o600;

/// Top-level botminter configuration stored at ~/.botminter/config.yml.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BotminterConfig {
    pub workzone: PathBuf,
    pub default_team: Option<String>,
    #[serde(default)]
    pub teams: Vec<TeamEntry>,
    /// Lima VMs provisioned by `bm env create` (or legacy `bm runtime create`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub vms: Vec<VmEntry>,
    /// Override the Secret Service collection used for credential storage.
    /// Default (None) uses the `login` collection.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub keyring_collection: Option<String>,
}

/// A provisioned Lima VM.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct VmEntry {
    pub name: String,
}

/// A registered team.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamEntry {
    pub name: String,
    pub path: PathBuf,
    pub profile: String,
    pub github_repo: String,
    pub credentials: Credentials,
    /// Override the profile's default coding agent (e.g., "gemini-cli").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub coding_agent: Option<String>,
    /// GitHub Project board number (stored during init).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub project_number: Option<u64>,
    /// Bridge lifecycle configuration.
    #[serde(default, skip_serializing_if = "BridgeLifecycle::is_default")]
    pub bridge_lifecycle: BridgeLifecycle,
    /// Daemon event configuration (polling/webhook).
    #[serde(default, skip_serializing_if = "DaemonSettings::is_default")]
    pub daemon: DaemonSettings,
    /// Optional Lima VM name this team is linked to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub vm: Option<String>,
}

/// Controls how the daemon listens for GitHub events.
///
/// By default, neither polling nor webhook is enabled — the daemon starts
/// as a pure supervisor (member lifecycle + token refresh) and stops
/// automatically when all members stop.
///
/// When polling or webhook is enabled, the daemon stays alive after `bm stop`
/// and may restart members on GitHub events. Members are marked "suspended"
/// to prevent unwanted restarts.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaemonSettings {
    /// Poll GitHub Events API for activity. Default: false (opt-in).
    #[serde(default)]
    pub polling: bool,
    /// Accept GitHub webhook pushes. Default: false (opt-in).
    #[serde(default)]
    pub webhook: bool,
    /// Poll interval in seconds (only used when polling=true). Default: 60.
    #[serde(default = "DaemonSettings::default_interval")]
    pub interval: u64,
}

impl Default for DaemonSettings {
    fn default() -> Self {
        Self {
            polling: false,
            webhook: false,
            interval: 60,
        }
    }
}

impl DaemonSettings {
    fn default_interval() -> u64 {
        60
    }

    fn is_default(&self) -> bool {
        !self.polling && !self.webhook && self.interval == 60
    }

    /// Returns true if any event source is configured.
    pub fn has_event_source(&self) -> bool {
        self.polling || self.webhook
    }
}

/// Controls bridge lifecycle relative to member start/stop.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct BridgeLifecycle {
    /// Start bridge automatically during `bm start`. Default: true.
    pub start_on_up: bool,
    /// Stop bridge automatically during `bm stop`. Default: false.
    pub stop_on_down: bool,
}

impl Default for BridgeLifecycle {
    fn default() -> Self {
        Self {
            start_on_up: true,
            stop_on_down: false,
        }
    }
}

impl BridgeLifecycle {
    fn is_default(&self) -> bool {
        self.start_on_up && !self.stop_on_down
    }
}

/// Stored credentials for a team (tokens).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Credentials {
    /// Legacy field: kept for backward compat with existing config.yml files.
    /// New code uses the CredentialStore (system keyring) for bridge tokens.
    /// Read from old configs but never written to new ones.
    #[serde(default, skip_serializing)]
    pub telegram_bot_token: Option<String>,
    /// Webhook secret for daemon webhook verification (optional).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub webhook_secret: Option<String>,
}

/// Returns the path to the config directory (~/.botminter/).
pub fn config_dir() -> Result<PathBuf> {
    let home = dirs::home_dir().context("Could not determine home directory")?;
    Ok(home.join(CONFIG_DIR))
}

/// Returns the path to the config file (~/.botminter/config.yml).
pub fn config_path() -> Result<PathBuf> {
    Ok(config_dir()?.join(CONFIG_FILE))
}

/// Loads the config from disk. Returns an error with guidance if the file doesn't exist.
pub fn load() -> Result<BotminterConfig> {
    load_from(&config_path()?)
}

/// Loads the config from a specific path.
pub fn load_from(path: &Path) -> Result<BotminterConfig> {
    if !path.exists() {
        bail!("No teams configured. Run `bm init` first.");
    }

    let contents =
        fs::read_to_string(path).context("Failed to read config file")?;

    let config: BotminterConfig =
        serde_yml::from_str(&contents).context("Failed to parse config file")?;

    Ok(config)
}

/// Checks config file permissions and returns a warning message if not 0600.
pub fn check_permissions_warning(path: &Path) -> Option<String> {
    if let Ok(metadata) = fs::metadata(path) {
        let mode = metadata.permissions().mode() & 0o777;
        if mode != CONFIG_PERMISSIONS {
            return Some(format!(
                "Config file {} has permissions {:04o} (expected {:04o}). \
                 This file contains secrets — consider running: chmod 600 {}",
                path.display(),
                mode,
                CONFIG_PERMISSIONS,
                path.display()
            ));
        }
    }
    None
}

/// Saves the config to disk with 0600 permissions.
pub fn save(config: &BotminterConfig) -> Result<()> {
    save_to(&config_path()?, config)
}

/// Saves the config to a specific path with 0600 permissions.
pub fn save_to(path: &Path, config: &BotminterConfig) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create config directory at {}", dir.display()))?;
    }

    let contents = serde_yml::to_string(config).context("Failed to serialize config")?;
    fs::write(path, contents).context("Failed to write config file")?;

    // Set file permissions to 0600 (owner read/write only)
    let perms = fs::Permissions::from_mode(CONFIG_PERMISSIONS);
    fs::set_permissions(path, perms)
        .context("Failed to set config file permissions to 0600")?;

    Ok(())
}


/// Loads the existing config or returns a fresh default.
pub fn load_or_default() -> BotminterConfig {
    load().unwrap_or_else(|_| BotminterConfig {
        workzone: default_workzone_path(),
        default_team: None,
        teams: Vec::new(),
        vms: Vec::new(),
        keyring_collection: None,
    })
}

/// Default workzone path: ~/.botminter/workspaces
pub fn default_workzone_path() -> PathBuf {
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".botminter")
        .join("workspaces")
}

/// Expands `~` at the start of a path to the home directory.
pub fn expand_tilde(path: &str) -> PathBuf {
    if path.starts_with("~/") || path == "~" {
        if let Some(home) = dirs::home_dir() {
            return home.join(&path[2..]);
        }
    }
    PathBuf::from(path)
}

/// Checks that `git` and `gh` CLI tools are available. Errors if either is missing.
pub fn check_prerequisites() -> Result<()> {
    let mut missing = Vec::new();
    if which::which("git").is_err() {
        missing.push("git — https://git-scm.com/");
    }
    if which::which("gh").is_err() {
        missing.push("gh — https://cli.github.com/");
    }
    if !missing.is_empty() {
        bail!(
            "Missing required tools:\n  {}\n\nInstall them and try again.",
            missing.join("\n  "),
        );
    }
    Ok(())
}

/// Resolves which team to operate on: explicit flag > default_team > error.
pub fn resolve_team<'a>(
    config: &'a BotminterConfig,
    flag: Option<&str>,
) -> Result<&'a TeamEntry> {
    let team_name = match flag {
        Some(name) => name.to_string(),
        None => match &config.default_team {
            Some(name) => name.clone(),
            None => bail!(
                "No default team set. Use `-t <team>` or run `bm init` to create a team."
            ),
        },
    };

    config
        .teams
        .iter()
        .find(|t| t.name == team_name)
        .with_context(|| {
            let available: Vec<&str> = config.teams.iter().map(|t| t.name.as_str()).collect();
            format!(
                "Team '{}' not found. Available teams: {}",
                team_name,
                available.join(", ")
            )
        })
}

/// Resolves which VM to use via 3-step resolution:
/// 1. If a team flag (or default team) has `vm` set, use it
/// 2. If exactly one VM is registered, use it
/// 3. If zero or multiple, return an error
pub fn resolve_vm(config: &BotminterConfig, team_flag: Option<&str>) -> Result<String> {
    // Step 1: team's VM binding
    if let Some(flag) = team_flag {
        if let Ok(team) = resolve_team(config, Some(flag)) {
            if let Some(ref vm) = team.vm {
                return Ok(vm.clone());
            }
        }
    } else if let Ok(team) = resolve_team(config, None) {
        if let Some(ref vm) = team.vm {
            return Ok(vm.clone());
        }
    }

    // Step 2: single VM auto-select
    match config.vms.len() {
        0 => bail!("No VM found. Run `bm env create` first."),
        1 => Ok(config.vms[0].name.clone()),
        _ => bail!(
            "Multiple VMs configured. Use `-t <team>` to select one, \
             or set `vm` on a team entry in ~/.botminter/config.yml."
        ),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Creates a config file path inside a temp directory (no env var mutation).
    fn test_config_path(tmp: &Path) -> PathBuf {
        tmp.join(".botminter").join("config.yml")
    }

    #[test]
    fn save_and_load_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = test_config_path(tmp.path());

        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp/workspaces"),
            default_team: Some("my-team".to_string()),
            teams: vec![TeamEntry {
                name: "my-team".to_string(),
                path: PathBuf::from("/tmp/workspaces/my-team"),
                profile: "scrum".to_string(),
                github_repo: "org/my-team".to_string(),
                credentials: Credentials {
                    telegram_bot_token: None,
                    webhook_secret: None,
                },
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }],
            vms: Vec::new(),
            keyring_collection: None,
        };

        save_to(&path, &config).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.default_team, Some("my-team".to_string()));
        assert_eq!(loaded.teams.len(), 1);
        assert_eq!(loaded.teams[0].name, "my-team");
        assert_eq!(loaded.teams[0].profile, "scrum");
        assert!(loaded.teams[0].credentials.telegram_bot_token.is_none());
    }

    #[test]
    fn load_missing_config_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let path = test_config_path(tmp.path());

        let result = load_from(&path);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bm init"));
    }

    #[test]
    fn config_file_has_0600_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let path = test_config_path(tmp.path());

        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp/ws"),
            default_team: None,
            teams: vec![],
            vms: Vec::new(),
            keyring_collection: None,
        };
        save_to(&path, &config).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Config file should have 0600 permissions");
    }

    #[test]
    fn resolve_team_with_flag() {
        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp"),
            default_team: Some("default".to_string()),
            teams: vec![
                TeamEntry {
                    name: "default".to_string(),
                    path: PathBuf::from("/tmp/default"),
                    profile: "agentic-sdlc-minimal".to_string(),
                    github_repo: "".to_string(),
                    credentials: Credentials::default(),
                    coding_agent: None,
                    project_number: None,
                    bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
                },
                TeamEntry {
                    name: "other".to_string(),
                    path: PathBuf::from("/tmp/other"),
                    profile: "scrum".to_string(),
                    github_repo: "".to_string(),
                    credentials: Credentials::default(),
                    coding_agent: None,
                    project_number: None,
                    bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
                },
            ],
            vms: Vec::new(),
            keyring_collection: None,
        };

        // Flag overrides default
        let team = resolve_team(&config, Some("other")).unwrap();
        assert_eq!(team.name, "other");
        assert_eq!(team.profile, "scrum");
    }

    #[test]
    fn resolve_team_uses_default() {
        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp"),
            default_team: Some("my-team".to_string()),
            teams: vec![TeamEntry {
                name: "my-team".to_string(),
                path: PathBuf::from("/tmp/my-team"),
                profile: "scrum".to_string(),
                github_repo: "".to_string(),
                credentials: Credentials::default(),
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }],
            vms: Vec::new(),
            keyring_collection: None,
        };

        let team = resolve_team(&config, None).unwrap();
        assert_eq!(team.name, "my-team");
    }

    #[test]
    fn resolve_team_no_default_no_flag_errors() {
        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp"),
            default_team: None,
            teams: vec![],
            vms: Vec::new(),
            keyring_collection: None,
        };

        let result = resolve_team(&config, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No default team"));
    }

    #[test]
    fn default_workzone_path_under_home() {
        let path = default_workzone_path();
        let path_str = path.to_string_lossy();
        assert!(
            path_str.contains(".botminter") && path_str.contains("workspaces"),
            "default_workzone_path should be under .botminter/workspaces: {}",
            path_str
        );
    }

    #[test]
    fn expand_tilde_home_prefix() {
        let result = expand_tilde("~/projects");
        let result_str = result.to_string_lossy();
        assert!(!result_str.starts_with("~"), "Should expand ~: {}", result_str);
        assert!(result_str.ends_with("projects"), "Should keep suffix: {}", result_str);
    }

    #[test]
    fn expand_tilde_no_tilde() {
        let result = expand_tilde("/absolute/path");
        assert_eq!(result, PathBuf::from("/absolute/path"));
    }

    #[test]
    fn check_prerequisites_passes_when_tools_present() {
        // Both git and gh are available in the dev environment
        assert!(check_prerequisites().is_ok());
    }

    #[test]
    fn resolve_team_nonexistent_errors() {
        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp"),
            default_team: None,
            teams: vec![TeamEntry {
                name: "exists".to_string(),
                path: PathBuf::from("/tmp/exists"),
                profile: "agentic-sdlc-minimal".to_string(),
                github_repo: "".to_string(),
                credentials: Credentials::default(),
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }],
            vms: Vec::new(),
            keyring_collection: None,
        };

        let result = resolve_team(&config, Some("nope"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nope"));
        assert!(err.contains("exists"));
    }
}
