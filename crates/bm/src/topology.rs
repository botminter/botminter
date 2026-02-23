use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Topology file describing where team members are running.
/// Lives at `{workzone}/{team_name}/topology.json`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct Topology {
    pub formation: String,
    pub created_at: String, // ISO 8601
    pub members: HashMap<String, MemberTopology>,
}

/// Topology entry for a single member.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct MemberTopology {
    pub status: String,
    pub endpoint: Endpoint,
}

/// Where a member is running — structured data, not shell commands.
#[derive(Debug, Serialize, Deserialize, Clone)]
#[serde(tag = "type")]
pub enum Endpoint {
    #[serde(rename = "local")]
    Local { pid: u32, workspace: PathBuf },

    #[serde(rename = "k8s")]
    K8s {
        namespace: String,
        pod: String,
        container: String,
        context: String,
    },
}

/// Returns the topology file path for a team.
pub fn topology_path(workzone: &Path, team_name: &str) -> PathBuf {
    workzone.join(team_name).join("topology.json")
}

/// Loads a topology file. Returns None if the file doesn't exist.
pub fn load(path: &Path) -> Result<Option<Topology>> {
    if !path.exists() {
        return Ok(None);
    }
    let contents = fs::read_to_string(path).context("Failed to read topology file")?;
    let topo: Topology =
        serde_json::from_str(&contents).context("Failed to parse topology file")?;
    Ok(Some(topo))
}

/// Saves a topology file atomically with 0600 permissions.
pub fn save(path: &Path, topo: &Topology) -> Result<()> {
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)
            .with_context(|| format!("Failed to create topology dir {}", dir.display()))?;
    }

    let contents = serde_json::to_string_pretty(topo).context("Failed to serialize topology")?;

    // Atomic write: temp file → rename
    let tmp_path = path.with_extension("json.tmp");
    fs::write(&tmp_path, contents).context("Failed to write temp topology file")?;

    // Set permissions before rename (0600 — contains PIDs, paths)
    let perms = fs::Permissions::from_mode(0o600);
    fs::set_permissions(&tmp_path, perms).context("Failed to set topology file permissions")?;

    fs::rename(&tmp_path, path).context("Failed to rename temp topology file")?;

    Ok(())
}

/// Removes the topology file if it exists.
pub fn remove(path: &Path) -> Result<()> {
    if path.exists() {
        fs::remove_file(path).context("Failed to remove topology file")?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_local_topology() -> Topology {
        let mut members = HashMap::new();
        members.insert(
            "architect-alice".to_string(),
            MemberTopology {
                status: "running".to_string(),
                endpoint: Endpoint::Local {
                    pid: 12345,
                    workspace: PathBuf::from("/tmp/ws/architect-alice/my-project"),
                },
            },
        );
        Topology {
            formation: "local".to_string(),
            created_at: "2026-02-21T10:00:00Z".to_string(),
            members,
        }
    }

    fn sample_k8s_topology() -> Topology {
        let mut members = HashMap::new();
        members.insert(
            "dev-bob".to_string(),
            MemberTopology {
                status: "running".to_string(),
                endpoint: Endpoint::K8s {
                    namespace: "botminter-my-team".to_string(),
                    pod: "dev-bob-7d8f9".to_string(),
                    container: "ralph".to_string(),
                    context: "kind-botminter".to_string(),
                },
            },
        );
        Topology {
            formation: "k8s".to_string(),
            created_at: "2026-02-21T10:00:00Z".to_string(),
            members,
        }
    }

    #[test]
    fn save_and_load_local_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");

        let topo = sample_local_topology();
        save(&path, &topo).unwrap();
        let loaded = load(&path).unwrap().unwrap();

        assert_eq!(loaded.formation, "local");
        assert_eq!(loaded.members.len(), 1);
        let member = loaded.members.get("architect-alice").unwrap();
        assert_eq!(member.status, "running");
        match &member.endpoint {
            Endpoint::Local { pid, workspace } => {
                assert_eq!(*pid, 12345);
                assert_eq!(workspace, &PathBuf::from("/tmp/ws/architect-alice/my-project"));
            }
            other => panic!("Expected Local endpoint, got {:?}", other),
        }
    }

    #[test]
    fn save_and_load_k8s_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");

        let topo = sample_k8s_topology();
        save(&path, &topo).unwrap();
        let loaded = load(&path).unwrap().unwrap();

        assert_eq!(loaded.formation, "k8s");
        let member = loaded.members.get("dev-bob").unwrap();
        match &member.endpoint {
            Endpoint::K8s {
                namespace,
                pod,
                container,
                context,
            } => {
                assert_eq!(namespace, "botminter-my-team");
                assert_eq!(pod, "dev-bob-7d8f9");
                assert_eq!(container, "ralph");
                assert_eq!(context, "kind-botminter");
            }
            other => panic!("Expected K8s endpoint, got {:?}", other),
        }
    }

    #[test]
    fn load_missing_returns_none() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let result = load(&path).unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn topology_file_has_0600_permissions() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");

        save(&path, &sample_local_topology()).unwrap();

        let metadata = fs::metadata(&path).unwrap();
        let mode = metadata.permissions().mode() & 0o777;
        assert_eq!(mode, 0o600, "Topology file should have 0600 permissions");
    }

    #[test]
    fn atomic_write_leaves_no_tmp() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");
        let tmp_path = tmp.path().join("topology.json.tmp");

        save(&path, &sample_local_topology()).unwrap();

        assert!(path.exists());
        assert!(!tmp_path.exists(), "Temp file should be renamed away");
    }

    #[test]
    fn remove_existing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");

        save(&path, &sample_local_topology()).unwrap();
        assert!(path.exists());

        remove(&path).unwrap();
        assert!(!path.exists());
    }

    #[test]
    fn remove_nonexistent_is_ok() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        // Should not error
        remove(&path).unwrap();
    }

    #[test]
    fn topology_path_construction() {
        let workzone = Path::new("/home/user/workzone");
        let result = topology_path(workzone, "my-team");
        assert_eq!(result, PathBuf::from("/home/user/workzone/my-team/topology.json"));
    }

    #[test]
    fn mixed_endpoint_topology() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("topology.json");

        let mut members = HashMap::new();
        members.insert(
            "local-member".to_string(),
            MemberTopology {
                status: "running".to_string(),
                endpoint: Endpoint::Local {
                    pid: 111,
                    workspace: PathBuf::from("/tmp/local"),
                },
            },
        );
        members.insert(
            "k8s-member".to_string(),
            MemberTopology {
                status: "running".to_string(),
                endpoint: Endpoint::K8s {
                    namespace: "ns".to_string(),
                    pod: "pod-1".to_string(),
                    container: "ralph".to_string(),
                    context: "ctx".to_string(),
                },
            },
        );

        let topo = Topology {
            formation: "mixed".to_string(),
            created_at: "2026-02-21T10:00:00Z".to_string(),
            members,
        };

        save(&path, &topo).unwrap();
        let loaded = load(&path).unwrap().unwrap();

        assert_eq!(loaded.members.len(), 2);
        assert!(matches!(
            loaded.members.get("local-member").unwrap().endpoint,
            Endpoint::Local { .. }
        ));
        assert!(matches!(
            loaded.members.get("k8s-member").unwrap().endpoint,
            Endpoint::K8s { .. }
        ));
    }
}
