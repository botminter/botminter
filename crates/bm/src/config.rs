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
}

/// A registered team.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct TeamEntry {
    pub name: String,
    pub path: PathBuf,
    pub profile: String,
    pub github_repo: String,
    pub credentials: Credentials,
}

/// Stored credentials for a team (tokens).
#[derive(Debug, Serialize, Deserialize, Clone, Default)]
pub struct Credentials {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub gh_token: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
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

    // Check file permissions and warn if not 0600
    check_permissions(path);

    let contents =
        fs::read_to_string(path).context("Failed to read config file")?;

    let config: BotminterConfig =
        serde_yml::from_str(&contents).context("Failed to parse config file")?;

    Ok(config)
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

/// Checks config file permissions and prints a warning if not 0600.
fn check_permissions(path: &Path) {
    if let Ok(metadata) = fs::metadata(path) {
        let mode = metadata.permissions().mode() & 0o777;
        if mode != CONFIG_PERMISSIONS {
            eprintln!(
                "Warning: Config file {} has permissions {:04o} (expected {:04o}). \
                 This file contains secrets — consider running: chmod 600 {}",
                path.display(),
                mode,
                CONFIG_PERMISSIONS,
                path.display()
            );
        }
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
                    gh_token: Some("ghp_test123".to_string()),
                    telegram_bot_token: None,
                    webhook_secret: None,
                },
            }],
        };

        save_to(&path, &config).unwrap();
        let loaded = load_from(&path).unwrap();

        assert_eq!(loaded.default_team, Some("my-team".to_string()));
        assert_eq!(loaded.teams.len(), 1);
        assert_eq!(loaded.teams[0].name, "my-team");
        assert_eq!(loaded.teams[0].profile, "scrum");
        assert_eq!(
            loaded.teams[0].credentials.gh_token,
            Some("ghp_test123".to_string())
        );
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
                    profile: "scrum-compact".to_string(),
                    github_repo: "".to_string(),
                    credentials: Credentials::default(),
                },
                TeamEntry {
                    name: "other".to_string(),
                    path: PathBuf::from("/tmp/other"),
                    profile: "scrum".to_string(),
                    github_repo: "".to_string(),
                    credentials: Credentials::default(),
                },
            ],
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
            }],
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
        };

        let result = resolve_team(&config, None);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No default team"));
    }

    #[test]
    fn resolve_team_nonexistent_errors() {
        let config = BotminterConfig {
            workzone: PathBuf::from("/tmp"),
            default_team: None,
            teams: vec![TeamEntry {
                name: "exists".to_string(),
                path: PathBuf::from("/tmp/exists"),
                profile: "scrum-compact".to_string(),
                github_repo: "".to_string(),
                credentials: Credentials::default(),
            }],
        };

        let result = resolve_team(&config, Some("nope"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nope"));
        assert!(err.contains("exists"));
    }
}
