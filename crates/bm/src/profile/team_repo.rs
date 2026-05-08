use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use serde::Deserialize;

use super::{BridgeDef, Meeting, ProfileManifest, ProjectDef};

const WORKSPACE_MARKER: &str = ".botminter.workspace";

/// Detects available profile meetings by trying two strategies:
/// 1. Workspace discovery — walk up from CWD looking for `.botminter.workspace`
/// 2. Config fallback — load default team from `~/.botminter/config.yml`
///
/// Returns an empty vec if no meetings are found or no team is configured.
pub fn detect_meetings() -> Vec<Meeting> {
    if let Some(meetings) = detect_meetings_from_workspace() {
        return meetings;
    }
    detect_meetings_from_config().unwrap_or_default()
}

fn detect_meetings_from_workspace() -> Option<Vec<Meeting>> {
    let cwd = std::env::current_dir().ok()?;
    let ws_root = discover_workspace_root(&cwd)?;
    let manifest_path = ws_root.join("team").join("botminter.yml");
    let contents = fs::read_to_string(manifest_path).ok()?;
    let manifest: ProfileManifest = serde_yml::from_str(&contents).ok()?;
    Some(manifest.meetings)
}

fn detect_meetings_from_config() -> Option<Vec<Meeting>> {
    let cfg = crate::config::load().ok()?;
    let team = crate::config::resolve_team(&cfg, None).ok()?;
    let team_repo = team.path.join("team");
    let manifest = read_team_repo_manifest(&team_repo).ok()?;
    if manifest.meetings.is_empty() {
        None
    } else {
        Some(manifest.meetings)
    }
}

fn discover_workspace_root(start: &Path) -> Option<PathBuf> {
    let mut current = start.to_path_buf();
    loop {
        if current.join(WORKSPACE_MARKER).exists() {
            return Some(current);
        }
        if !current.pop() {
            return None;
        }
    }
}

/// Minimal manifest for reading project count.
#[derive(Debug, Deserialize)]
struct ProjectsManifest {
    #[serde(default)]
    projects: Vec<ProjectDef>,
}

/// Reads project definitions from a team repo's botminter.yml.
pub fn read_team_projects(team_repo: &Path) -> Vec<ProjectDef> {
    let manifest_path = team_repo.join("botminter.yml");
    if let Ok(contents) = fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_yml::from_str::<ProjectsManifest>(&contents) {
            return manifest.projects;
        }
    }
    Vec::new()
}

/// Summary of a team's members and projects, gathered from the team repo.
#[derive(Debug)]
pub struct TeamSummary {
    /// (member_dir_name, role) pairs, sorted by name.
    pub members: Vec<(String, String)>,
    /// Project definitions from botminter.yml.
    pub projects: Vec<ProjectDef>,
}

/// Gathers a summary of members and projects from a team repo directory.
pub fn gather_team_summary(team_repo: &Path) -> TeamSummary {
    let members_dir = team_repo.join("members");
    let mut members: Vec<(String, String)> = Vec::new();
    if members_dir.is_dir() {
        if let Ok(entries) = fs::read_dir(&members_dir) {
            for entry in entries.flatten() {
                if !entry.file_type().map(|ft| ft.is_dir()).unwrap_or(false) {
                    continue;
                }
                let name = entry.file_name().to_string_lossy().to_string();
                if name.starts_with('.') {
                    continue;
                }
                let role = read_member_role(&members_dir, &name);
                members.push((name, role));
            }
        }
    }
    members.sort_by(|a, b| a.0.cmp(&b.0));

    let projects = read_team_projects(team_repo);

    TeamSummary { members, projects }
}

/// Minimal member manifest for reading role.
#[derive(Debug, Deserialize)]
struct MemberManifest {
    #[serde(default)]
    role: Option<String>,
}

/// Reads and parses the botminter.yml manifest from a team repo directory.
pub fn read_team_repo_manifest(team_repo: &Path) -> Result<ProfileManifest> {
    let manifest_path = team_repo.join("botminter.yml");
    let contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team repo's botminter.yml")?;
    serde_yml::from_str(&contents).context("Failed to parse botminter.yml")
}

/// Reads the schema_version from a team repo's botminter.yml.
pub fn read_team_schema(team_repo: &Path) -> Result<String> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        bail!(
            "Team repo at {} has no botminter.yml",
            team_repo.display()
        );
    }
    let contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml")?;
    let val: serde_yml::Value =
        serde_yml::from_str(&contents).context("Failed to parse team botminter.yml")?;
    Ok(val["schema_version"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// Lists non-hidden files in a directory, returning their names sorted.
pub fn list_files_in_dir(dir: &Path) -> Vec<String> {
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<String> = fs::read_dir(dir)
        .into_iter()
        .flatten()
        .flatten()
        .filter(|e| {
            e.file_type().map(|ft| ft.is_file()).unwrap_or(false)
                && !e.file_name().to_string_lossy().starts_with('.')
        })
        .map(|e| e.file_name().to_string_lossy().to_string())
        .collect();
    files.sort();
    files
}

/// Lists non-hidden subdirectory names under a directory, sorted.
pub fn list_subdirs(dir: &Path) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    dirs.push(name);
                }
            }
        }
    }
    dirs.sort();
    dirs
}

/// Discovers sorted, non-hidden member directories in the team repo.
pub fn discover_member_dirs(team_repo: &Path) -> Vec<String> {
    list_subdirs(&team_repo.join("members"))
}

/// Reads the role from a member's botminter.yml, falling back to dir-name inference.
pub fn read_member_role(members_dir: &Path, member_dir_name: &str) -> String {
    let manifest_path = members_dir.join(member_dir_name).join("botminter.yml");
    if let Ok(contents) = fs::read_to_string(&manifest_path) {
        if let Ok(manifest) = serde_yml::from_str::<MemberManifest>(&contents) {
            if let Some(role) = manifest.role {
                return role;
            }
        }
    }
    infer_role_from_dir(member_dir_name)
}

/// Infers the role from a member dir name by stripping the last `-segment`
/// (the member name given at hire time). The dir format is `{role}-{name}`,
/// where the role itself may contain hyphens (e.g., `chief-of-staff-bob`
/// → role `chief-of-staff`, name `bob`).
///
/// Falls back to the first segment if the dir has only one hyphen or none.
pub fn infer_role_from_dir(dir_name: &str) -> String {
    match dir_name.rsplit_once('-') {
        Some((role, _name)) if !role.is_empty() => role.to_string(),
        _ => dir_name.to_string(),
    }
}

/// Validates that a path points to a knowledge or invariant file within the team repo.
pub fn validate_knowledge_path(path: &str) -> Result<()> {
    let parts: Vec<&str> = path.split('/').collect();

    let is_knowledge_or_invariant = |segment: &str| -> bool {
        segment == "knowledge" || segment == "invariants"
    };

    let valid = match parts.first() {
        Some(&"knowledge") | Some(&"invariants") => true,
        Some(&"projects") => parts.len() >= 3 && is_knowledge_or_invariant(parts[2]),
        Some(&"members") => {
            (parts.len() >= 3 && is_knowledge_or_invariant(parts[2]))
                || (parts.len() >= 5
                    && parts[2] == "projects"
                    && is_knowledge_or_invariant(parts[4]))
        }
        _ => false,
    };

    if !valid {
        bail!("Path is not within a knowledge or invariant directory");
    }
    Ok(())
}

/// Builds env vars from team credentials for interactive sessions.
pub fn credentials_env(_team: &crate::config::TeamEntry) -> Vec<(String, String)> {
    // GH_TOKEN is no longer stored in config — members use GH_CONFIG_DIR
    // (daemon-managed) and operators use `gh auth login`.
    Vec::new()
}

/// Augments the botminter.yml in the team repo with a projects section.
pub fn augment_manifest_with_projects(
    team_repo: &Path,
    projects: &[(String, String)],
) -> Result<()> {
    let manifest_path = team_repo.join("botminter.yml");
    let mut manifest: ProfileManifest = {
        let contents = fs::read_to_string(&manifest_path)
            .context("Failed to read botminter.yml from team repo")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };

    manifest.projects = projects
        .iter()
        .map(|(name, url)| ProjectDef {
            name: name.clone(),
            fork_url: url.clone(),
        })
        .collect();

    let contents = serde_yml::to_string(&manifest)
        .context("Failed to serialize augmented botminter.yml")?;
    fs::write(&manifest_path, contents)
        .context("Failed to write augmented botminter.yml")?;

    Ok(())
}

/// Validates that a bridge name exists in the profile's bridges list.
pub fn validate_bridge_selection(bridge_name: &str, bridges: &[BridgeDef]) -> Result<()> {
    if bridges.is_empty() {
        bail!("Profile has no bridges available. Remove the --bridge flag.");
    }
    if !bridges.iter().any(|b| b.name == bridge_name) {
        let available: Vec<&str> = bridges.iter().map(|b| b.name.as_str()).collect();
        bail!(
            "Bridge '{}' not found in profile. Available bridges: {}",
            bridge_name,
            available.join(", ")
        );
    }
    Ok(())
}

/// Records the selected bridge in the team's botminter.yml.
///
/// For local bridges, also records the operator identity with the default
/// admin username (`bmadmin`). External bridges don't have managed admin
/// accounts, so the operator section is skipped.
pub fn record_bridge_in_manifest(
    team_repo: &Path,
    bridge_name: &str,
    bridges: &[BridgeDef],
) -> Result<()> {
    let manifest_path = team_repo.join("botminter.yml");
    let contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml for bridge recording")?;
    let mut doc: serde_yml::Value =
        serde_yml::from_str(&contents).context("Failed to parse team botminter.yml")?;

    if let serde_yml::Value::Mapping(ref mut map) = doc {
        map.insert(
            serde_yml::Value::String("bridge".to_string()),
            serde_yml::Value::String(bridge_name.to_string()),
        );

        let is_local = bridges
            .iter()
            .any(|b| b.name == bridge_name && b.bridge_type == "local");
        if is_local {
            let mut op_map = serde_yml::Mapping::new();
            op_map.insert(
                serde_yml::Value::String("bridge_username".to_string()),
                serde_yml::Value::String("bmadmin".to_string()),
            );
            map.insert(
                serde_yml::Value::String("operator".to_string()),
                serde_yml::Value::Mapping(op_map),
            );
        }
    }

    let updated = serde_yml::to_string(&doc)
        .context("Failed to serialize team botminter.yml with bridge")?;
    fs::write(&manifest_path, updated)
        .context("Failed to write team botminter.yml with bridge")?;

    Ok(())
}

/// Lists .md files in a scope subdirectory (e.g., knowledge/ or invariants/).
pub fn list_scope_files(base: &Path, subdir: &str) -> Vec<String> {
    let dir = base.join(subdir);
    if !dir.is_dir() {
        return Vec::new();
    }
    let mut files: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") {
                files.push(name);
            }
        }
    }
    files.sort();
    files
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── read_member_role ──────────────────────────────────────────

    #[test]
    fn read_member_role_from_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("architect-alice");
        fs::create_dir(&member_dir).unwrap();
        fs::write(
            member_dir.join("botminter.yml"),
            "role: architect\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "architect-alice");
        assert_eq!(role, "architect");
    }

    #[test]
    fn read_member_role_yaml_with_extra_fields() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("po-bob");
        fs::create_dir(&member_dir).unwrap();
        fs::write(
            member_dir.join("botminter.yml"),
            "role: product-owner\nschema_version: '0.3'\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "po-bob");
        assert_eq!(role, "product-owner");
    }

    #[test]
    fn read_member_role_fallback_no_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("architect-alice")).unwrap();

        let role = read_member_role(tmp.path(), "architect-alice");
        assert_eq!(role, "architect");
    }

    #[test]
    fn read_member_role_fallback_no_role_field() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("po-bob");
        fs::create_dir(&member_dir).unwrap();
        fs::write(
            member_dir.join("botminter.yml"),
            "schema_version: '0.3'\n",
        )
        .unwrap();

        let role = read_member_role(tmp.path(), "po-bob");
        assert_eq!(role, "po");
    }

    // ── infer_role_from_dir ──────────────────────────────────────

    #[test]
    fn infer_role_standard_pattern() {
        assert_eq!(infer_role_from_dir("architect-alice"), "architect");
    }

    #[test]
    fn infer_role_multiple_hyphens() {
        assert_eq!(infer_role_from_dir("po-bob-senior"), "po-bob");
    }

    #[test]
    fn infer_role_hyphenated_role() {
        assert_eq!(infer_role_from_dir("chief-of-staff-bob"), "chief-of-staff");
    }

    #[test]
    fn infer_role_no_hyphen() {
        assert_eq!(infer_role_from_dir("superman"), "superman");
    }

    #[test]
    fn infer_role_empty_string() {
        assert_eq!(infer_role_from_dir(""), "");
    }

    // ── validate_knowledge_path ──────────────────────────────────

    #[test]
    fn valid_team_knowledge_path() {
        assert!(validate_knowledge_path("knowledge/commit-convention.md").is_ok());
    }

    #[test]
    fn valid_team_invariant_path() {
        assert!(validate_knowledge_path("invariants/code-review.md").is_ok());
    }

    #[test]
    fn valid_project_knowledge_path() {
        assert!(validate_knowledge_path("projects/my-project/knowledge/api.md").is_ok());
    }

    #[test]
    fn valid_project_invariant_path() {
        assert!(validate_knowledge_path("projects/my-project/invariants/test.md").is_ok());
    }

    #[test]
    fn valid_member_knowledge_path() {
        assert!(validate_knowledge_path("members/architect-alice/knowledge/patterns.md").is_ok());
    }

    #[test]
    fn valid_member_invariant_path() {
        assert!(validate_knowledge_path("members/architect-alice/invariants/quality.md").is_ok());
    }

    #[test]
    fn valid_member_project_knowledge_path() {
        assert!(validate_knowledge_path(
            "members/architect-alice/projects/my-project/knowledge/notes.md"
        )
        .is_ok());
    }

    #[test]
    fn invalid_path_botminter_yml() {
        let result = validate_knowledge_path("botminter.yml");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not within a knowledge or invariant directory"));
    }

    #[test]
    fn invalid_path_random_file() {
        assert!(validate_knowledge_path("PROCESS.md").is_err());
    }

    #[test]
    fn invalid_path_member_no_scope() {
        assert!(validate_knowledge_path("members/architect-alice/PROMPT.md").is_err());
    }

    #[test]
    fn invalid_path_project_no_scope() {
        assert!(validate_knowledge_path("projects/my-project/README.md").is_err());
    }

    // ── list_files_in_dir ─────────────────────────────────────────

    #[test]
    fn list_files_in_dir_returns_sorted_non_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("beta.md"), "").unwrap();
        fs::write(tmp.path().join("alpha.md"), "").unwrap();
        fs::write(tmp.path().join(".hidden"), "").unwrap();

        let files = list_files_in_dir(tmp.path());
        assert_eq!(files, vec!["alpha.md", "beta.md"]);
    }

    #[test]
    fn list_files_in_dir_nonexistent() {
        let files = list_files_in_dir(std::path::Path::new("/nonexistent/path"));
        assert!(files.is_empty());
    }

    // ── list_subdirs ──────────────────────────────────────────────

    #[test]
    fn list_subdirs_returns_sorted_non_hidden() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("beta")).unwrap();
        fs::create_dir(tmp.path().join("alpha")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let dirs = list_subdirs(tmp.path());
        assert_eq!(dirs, vec!["alpha", "beta"]);
    }

    // ── discover_member_dirs ─────────────────────────────────────

    #[test]
    fn discover_member_dirs_from_team_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let members = tmp.path().join("members");
        fs::create_dir_all(&members).unwrap();
        fs::create_dir(members.join("architect-alice")).unwrap();
        fs::create_dir(members.join("po-bob")).unwrap();
        fs::create_dir(members.join(".hidden")).unwrap();

        let dirs = discover_member_dirs(tmp.path());
        assert_eq!(dirs, vec!["architect-alice", "po-bob"]);
    }

    #[test]
    fn discover_member_dirs_no_members_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = discover_member_dirs(tmp.path());
        assert!(dirs.is_empty());
    }

    // ── read_team_repo_manifest ──────────────────────────────────

    #[test]
    fn read_team_repo_manifest_parses() {
        let tmp = tempfile::tempdir().unwrap();
        let yaml = "name: test\ndisplay_name: Test\ndescription: Test\nversion: '1.0.0'\nschema_version: '1.0'\n";
        fs::write(tmp.path().join("botminter.yml"), yaml).unwrap();

        let manifest = read_team_repo_manifest(tmp.path()).unwrap();
        assert_eq!(manifest.name, "test");
    }

    #[test]
    fn read_team_repo_manifest_missing_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_team_repo_manifest(tmp.path()).is_err());
    }

    // ── read_team_schema ─────────────────────────────────────────

    #[test]
    fn read_team_schema_parses_version() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("botminter.yml"),
            "schema_version: '1.0'\nname: test\n",
        )
        .unwrap();

        let schema = read_team_schema(tmp.path()).unwrap();
        assert_eq!(schema, "1.0");
    }

    #[test]
    fn read_team_schema_missing_errors() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(read_team_schema(tmp.path()).is_err());
    }

    // ── list_scope_files ─────────────────────────────────────────

    #[test]
    fn list_scope_files_returns_md_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        let knowledge = tmp.path().join("knowledge");
        fs::create_dir(&knowledge).unwrap();
        fs::write(knowledge.join("beta.md"), "").unwrap();
        fs::write(knowledge.join("alpha.md"), "").unwrap();
        fs::write(knowledge.join("readme.txt"), "").unwrap();

        let files = list_scope_files(tmp.path(), "knowledge");
        assert_eq!(files, vec!["alpha.md", "beta.md"]);
    }

    #[test]
    fn list_scope_files_nonexistent() {
        let tmp = tempfile::tempdir().unwrap();
        let files = list_scope_files(tmp.path(), "knowledge");
        assert!(files.is_empty());
    }

    // ── validate_bridge_selection ─────────────────────────────

    #[test]
    fn bridge_valid_name_accepted() {
        let bridges = vec![BridgeDef {
            name: "telegram".to_string(),
            display_name: "Telegram".to_string(),
            description: "Telegram bot".to_string(),
            bridge_type: "external".to_string(),
        }];
        assert!(validate_bridge_selection("telegram", &bridges).is_ok());
    }

    #[test]
    fn bridge_invalid_name_rejected() {
        let bridges = vec![BridgeDef {
            name: "telegram".to_string(),
            display_name: "Telegram".to_string(),
            description: "Telegram bot".to_string(),
            bridge_type: "external".to_string(),
        }];
        let result = validate_bridge_selection("slack", &bridges);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("telegram"), "Error should list available bridges: {}", err);
    }

    #[test]
    fn bridge_empty_bridges_rejects() {
        let bridges: Vec<BridgeDef> = Vec::new();
        let result = validate_bridge_selection("telegram", &bridges);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("no bridges"), "Error should say no bridges available: {}", err);
    }

    // ── record_bridge_in_manifest ───────────────────────────────

    #[test]
    fn record_bridge_adds_bridge_field() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("botminter.yml"),
            "name: test\nschema_version: '1.0'\n",
        ).unwrap();

        let bridges = vec![BridgeDef {
            name: "telegram".to_string(),
            display_name: "Telegram".to_string(),
            description: "".to_string(),
            bridge_type: "external".to_string(),
        }];
        record_bridge_in_manifest(tmp.path(), "telegram", &bridges).unwrap();

        let contents = fs::read_to_string(tmp.path().join("botminter.yml")).unwrap();
        assert!(contents.contains("bridge: telegram"), "Should contain bridge field: {}", contents);
        assert!(!contents.contains("operator"), "External bridge should not set operator");
    }

    #[test]
    fn record_bridge_local_sets_operator() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("botminter.yml"),
            "name: test\nschema_version: '1.0'\n",
        ).unwrap();

        let bridges = vec![BridgeDef {
            name: "tuwunel".to_string(),
            display_name: "Tuwunel".to_string(),
            description: "".to_string(),
            bridge_type: "local".to_string(),
        }];
        record_bridge_in_manifest(tmp.path(), "tuwunel", &bridges).unwrap();

        let contents = fs::read_to_string(tmp.path().join("botminter.yml")).unwrap();
        assert!(contents.contains("bridge: tuwunel"));
        assert!(contents.contains("bridge_username: bmadmin"), "Local bridge should set operator");
    }

    // ── augment_manifest_with_projects ────────────────────────────

    #[test]
    fn augment_manifest_adds_projects() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(
            tmp.path().join("botminter.yml"),
            "name: test\nversion: '1.0.0'\nschema_version: '1.0'\ndescription: Test\ndisplay_name: Test\n",
        ).unwrap();

        let projects = vec![
            ("my-app".to_string(), "https://github.com/org/my-app.git".to_string()),
        ];
        augment_manifest_with_projects(tmp.path(), &projects).unwrap();

        let contents = fs::read_to_string(tmp.path().join("botminter.yml")).unwrap();
        assert!(contents.contains("my-app"), "Should contain project name: {}", contents);
        assert!(contents.contains("https://github.com/org/my-app.git"), "Should contain project URL");
    }
}
