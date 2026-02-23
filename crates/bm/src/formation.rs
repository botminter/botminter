use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

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
pub fn resolve_formation(
    team_repo: &Path,
    flag: Option<&str>,
) -> Result<Option<String>> {
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
