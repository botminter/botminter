use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};
use include_dir::{Dir, include_dir};
use serde::{Deserialize, Serialize};

static PROFILES: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../profiles");

/// Profile manifest parsed from botminter.yml
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProfileManifest {
    pub name: String,
    pub display_name: String,
    pub description: String,
    pub version: String,
    pub schema_version: String,
    #[serde(default)]
    pub roles: Vec<RoleDef>,
    #[serde(default)]
    pub labels: Vec<LabelDef>,
    #[serde(default)]
    pub statuses: Vec<StatusDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub projects: Vec<ProjectDef>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub views: Vec<ViewDef>,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct RoleDef {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct LabelDef {
    pub name: String,
    pub color: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct StatusDef {
    pub name: String,
    pub description: String,
}

#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ProjectDef {
    pub name: String,
    pub fork_url: String,
}

/// Defines a role-based view for the GitHub Project board.
/// Each view maps to a subset of statuses via prefix matching.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct ViewDef {
    pub name: String,
    /// Status name prefixes to include (e.g., ["po"] matches "po:triage", "po:backlog", etc.)
    pub prefixes: Vec<String>,
    /// Extra statuses always included regardless of prefix (e.g., ["done", "error"])
    #[serde(default)]
    pub also_include: Vec<String>,
}

impl ViewDef {
    /// Expands prefixes against the full status list, returning matching status names
    /// plus any `also_include` entries.
    pub fn resolve_statuses(&self, all_statuses: &[StatusDef]) -> Vec<String> {
        let mut result: Vec<String> = all_statuses
            .iter()
            .filter(|s| {
                self.prefixes
                    .iter()
                    .any(|p| s.name.starts_with(&format!("{}:", p)))
            })
            .map(|s| s.name.clone())
            .collect();
        for extra in &self.also_include {
            if !result.contains(extra) {
                result.push(extra.clone());
            }
        }
        result
    }

    /// Builds a GitHub Projects filter string for this view.
    /// Example: `status:po:triage,po:backlog,po:ready,done,error`
    pub fn filter_string(&self, all_statuses: &[StatusDef]) -> String {
        let statuses = self.resolve_statuses(all_statuses);
        format!("status:{}", statuses.join(","))
    }
}

/// Returns the names of all embedded profiles.
pub fn list_profiles() -> Vec<String> {
    let mut names: Vec<String> = PROFILES
        .dirs()
        .map(|d| d.path().file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Reads and parses the botminter.yml manifest for a named profile.
pub fn read_manifest(name: &str) -> Result<ProfileManifest> {
    let path = format!("{}/botminter.yml", name);
    let file = PROFILES
        .get_file(&path)
        .with_context(|| {
            let available = list_profiles().join(", ");
            format!(
                "Profile '{}' not found. Available profiles: {}",
                name, available
            )
        })?;

    let contents = file
        .contents_utf8()
        .context("Profile manifest is not valid UTF-8")?;

    let manifest: ProfileManifest =
        serde_yml::from_str(contents).context("Failed to parse profile manifest")?;

    Ok(manifest)
}

/// Lists the role names available in a profile by reading its members/ subdirectory.
pub fn list_roles(name: &str) -> Result<Vec<String>> {
    let members_path = format!("{}/members", name);
    let members_dir = PROFILES.get_dir(&members_path).with_context(|| {
        format!(
            "Profile '{}' has no members/ directory",
            name
        )
    })?;

    let mut roles: Vec<String> = members_dir
        .dirs()
        .map(|d| d.path().file_name().unwrap().to_string_lossy().to_string())
        .collect();
    roles.sort();
    Ok(roles)
}

/// Extracts a profile's team-repo content to the target directory.
/// Copies everything from the embedded profile EXCEPT `members/` and `.schema/`
/// (members are extracted on demand via `extract_member_to`; schema is internal).
pub fn extract_profile_to(profile_name: &str, target: &Path) -> Result<()> {
    let profile_dir = PROFILES
        .get_dir(profile_name)
        .with_context(|| {
            let available = list_profiles().join(", ");
            format!(
                "Profile '{}' not found. Available profiles: {}",
                profile_name, available
            )
        })?;

    let root_path = profile_dir.path().to_path_buf();
    extract_dir_recursive(profile_dir, target, &root_path, &|rel_path| {
        // rel_path is relative to the profile root, e.g. "members/architect/..."
        let first = rel_path
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string());
        matches!(first.as_deref(), Some("members") | Some(".schema"))
    })?;

    Ok(())
}

/// Extracts a member skeleton from the embedded profile into the target directory.
/// Copies the contents of `profiles/{profile}/members/{role}/` to `target/`.
pub fn extract_member_to(profile_name: &str, role: &str, target: &Path) -> Result<()> {
    let member_path = format!("{}/members/{}", profile_name, role);
    let member_dir = PROFILES.get_dir(&member_path).with_context(|| {
        let roles = list_roles(profile_name).unwrap_or_default().join(", ");
        format!(
            "Role '{}' not available in profile '{}'. Available roles: {}",
            role, profile_name, roles
        )
    })?;

    let root_path = member_dir.path().to_path_buf();
    extract_dir_recursive(member_dir, target, &root_path, &|_| false)?;
    Ok(())
}

/// Recursively extracts files from an embedded Dir to a filesystem path.
/// `root_path` is the path of the root directory being extracted (used to compute
/// relative paths for target files). The `skip` predicate receives the path relative
/// to `root_path` and returns true to skip that entry.
fn extract_dir_recursive(
    dir: &Dir<'_>,
    base_target: &Path,
    root_path: &Path,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<()> {
    // Extract files directly in this directory
    for file in dir.files() {
        // Compute path relative to the root being extracted
        let rel = file
            .path()
            .strip_prefix(root_path)
            .unwrap_or(file.path());

        if skip(rel) {
            continue;
        }

        let target_path = base_target.join(rel);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory {}", parent.display())
            })?;
        }

        fs::write(&target_path, file.contents()).with_context(|| {
            format!("Failed to write {}", target_path.display())
        })?;
    }

    // Recurse into subdirectories
    for sub_dir in dir.dirs() {
        let rel = sub_dir
            .path()
            .strip_prefix(root_path)
            .unwrap_or(sub_dir.path());

        if skip(rel) {
            continue;
        }

        extract_dir_recursive(sub_dir, base_target, root_path, skip)?;
    }

    Ok(())
}

/// Returns the raw embedded PROFILES directory for advanced access.
#[allow(dead_code)] // used in Step 5 (bm teams sync)
pub fn embedded_profiles() -> &'static Dir<'static> {
    &PROFILES
}

/// Checks that the embedded profile's schema_version matches the expected value.
/// Returns an error suggesting `bm upgrade` on mismatch.
pub fn check_schema_version(profile_name: &str, team_schema: &str) -> Result<()> {
    let manifest = read_manifest(profile_name)?;
    if manifest.schema_version != team_schema {
        bail!(
            "Team uses schema {} but this version of `bm` carries schema {} for profile '{}'. \
             Run `bm upgrade` to migrate the team first.",
            team_schema,
            manifest.schema_version,
            profile_name
        );
    }
    Ok(())
}

/// Gate for commands that require the current schema version (1.0).
/// Reads the team's botminter.yml and checks that schema_version matches.
/// Returns a clear error directing the user to upgrade or re-init.
pub fn require_current_schema(team_name: &str, team_schema: &str) -> Result<()> {
    if team_schema != "1.0" {
        bail!(
            "This feature requires schema 1.0, but team '{}' uses schema {}.\n\
             Run `bm upgrade` to migrate the team, or re-init with a current profile.",
            team_name,
            team_schema
        );
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn list_profiles_returns_expected() {
        let profiles = list_profiles();
        assert!(profiles.contains(&"scrum".to_string()));
        assert!(profiles.contains(&"scrum-compact".to_string()));
        assert!(profiles.contains(&"scrum-compact-telegram".to_string()));
        assert_eq!(profiles.len(), 3);
    }

    #[test]
    fn list_profiles_is_sorted() {
        let profiles = list_profiles();
        assert_eq!(profiles[0], "scrum");
        assert_eq!(profiles[1], "scrum-compact");
        assert_eq!(profiles[2], "scrum-compact-telegram");
    }

    #[test]
    fn read_manifest_parses_rh_scrum() {
        let manifest = read_manifest("scrum").unwrap();
        assert_eq!(manifest.name, "scrum");
        assert_eq!(manifest.display_name, "Scrum Team");
        assert_eq!(manifest.version, "1.0.0");
        assert_eq!(manifest.schema_version, "1.0");
        assert!(!manifest.description.is_empty());
        assert_eq!(manifest.roles.len(), 2);
        assert!(manifest.labels.len() > 0);
    }

    #[test]
    fn read_manifest_parses_compact() {
        let manifest = read_manifest("scrum-compact").unwrap();
        assert_eq!(manifest.name, "scrum-compact");
        assert_eq!(manifest.display_name, "Scrum Compact Solo Team");
        assert_eq!(manifest.roles.len(), 1);
        assert_eq!(manifest.roles[0].name, "superman");
    }

    #[test]
    fn read_manifest_roles_have_descriptions() {
        let manifest = read_manifest("scrum").unwrap();
        for role in &manifest.roles {
            assert!(!role.name.is_empty());
            assert!(!role.description.is_empty());
        }
    }

    #[test]
    fn read_manifest_labels_have_required_fields() {
        let manifest = read_manifest("scrum").unwrap();
        for label in &manifest.labels {
            assert!(!label.name.is_empty());
            assert!(!label.color.is_empty());
            assert!(!label.description.is_empty());
        }
    }

    #[test]
    fn read_manifest_nonexistent_profile_errors() {
        let result = read_manifest("nonexistent");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
        assert!(err.contains("scrum"));
        assert!(err.contains("scrum-compact"));
        assert!(err.contains("scrum-compact-telegram"));
    }

    #[test]
    fn list_roles_returns_expected_for_rh_scrum() {
        let roles = list_roles("scrum").unwrap();
        assert!(roles.contains(&"architect".to_string()));
        assert!(roles.contains(&"human-assistant".to_string()));
        assert_eq!(roles.len(), 2);
    }

    #[test]
    fn list_roles_returns_expected_for_compact() {
        let roles = list_roles("scrum-compact").unwrap();
        assert_eq!(roles, vec!["superman".to_string()]);
    }

    #[test]
    fn rh_scrum_label_count() {
        let manifest = read_manifest("scrum").unwrap();
        assert_eq!(manifest.labels.len(), 2); // kind/epic, kind/story
    }

    #[test]
    fn scrum_compact_label_count() {
        let manifest = read_manifest("scrum-compact").unwrap();
        assert_eq!(manifest.labels.len(), 3); // kind/epic, kind/story, kind/docs
    }

    #[test]
    fn rh_scrum_status_count() {
        let manifest = read_manifest("scrum").unwrap();
        assert_eq!(manifest.statuses.len(), 25);
    }

    #[test]
    fn scrum_compact_status_count() {
        let manifest = read_manifest("scrum-compact").unwrap();
        assert_eq!(manifest.statuses.len(), 25);
    }

    #[test]
    fn read_manifest_parses_scrum_compact_telegram() {
        let manifest = read_manifest("scrum-compact-telegram").unwrap();
        assert_eq!(manifest.name, "scrum-compact-telegram");
        assert_eq!(manifest.display_name, "Scrum Compact Solo Team (Telegram HIL)");
        assert_eq!(manifest.roles.len(), 1);
        assert_eq!(manifest.roles[0].name, "superman");
    }

    #[test]
    fn scrum_compact_telegram_status_count() {
        let manifest = read_manifest("scrum-compact-telegram").unwrap();
        assert_eq!(manifest.statuses.len(), 25);
    }

    #[test]
    fn scrum_compact_telegram_has_views() {
        let manifest = read_manifest("scrum-compact-telegram").unwrap();
        assert!(!manifest.views.is_empty());
    }

    #[test]
    fn read_manifest_statuses_have_required_fields() {
        let manifest = read_manifest("scrum").unwrap();
        for status in &manifest.statuses {
            assert!(!status.name.is_empty());
            assert!(!status.description.is_empty());
        }
    }

    #[test]
    fn extract_profile_copies_team_content() {
        let tmp = tempfile::tempdir().unwrap();
        extract_profile_to("scrum", tmp.path()).unwrap();

        // Should have PROCESS.md, CLAUDE.md, botminter.yml
        assert!(tmp.path().join("PROCESS.md").exists());
        assert!(tmp.path().join("CLAUDE.md").exists());
        assert!(tmp.path().join("botminter.yml").exists());

        // Should have knowledge/ and invariants/
        assert!(tmp.path().join("knowledge").is_dir());
        assert!(tmp.path().join("invariants").is_dir());

        // Should have agent/
        assert!(tmp.path().join("agent").is_dir());

        // Should NOT have members/ or .schema/ (those are excluded)
        assert!(!tmp.path().join("members").exists());
        assert!(!tmp.path().join(".schema").exists());
    }

    #[test]
    fn extract_member_copies_skeleton() {
        let tmp = tempfile::tempdir().unwrap();
        extract_member_to("scrum", "architect", tmp.path()).unwrap();

        // Should have the member skeleton files
        assert!(tmp.path().join(".botminter.yml").exists());
        assert!(tmp.path().join("PROMPT.md").exists());
        assert!(tmp.path().join("CLAUDE.md").exists());
        assert!(tmp.path().join("ralph.yml").exists());
    }

    #[test]
    fn extract_member_invalid_role_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let result = extract_member_to("scrum", "nonexistent", tmp.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("architect"));
    }

    #[test]
    fn check_schema_version_match() {
        assert!(check_schema_version("scrum", "1.0").is_ok());
        assert!(check_schema_version("scrum-compact", "1.0").is_ok());
    }

    #[test]
    fn check_schema_version_mismatch() {
        let result = check_schema_version("scrum", "99.0");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bm upgrade"));
    }

    #[test]
    fn check_schema_version_old_team_against_current_profile() {
        let result = check_schema_version("scrum", "0.1");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bm upgrade"));
        assert!(err.contains("0.1"));
        assert!(err.contains("1.0"));
    }

    #[test]
    fn extract_profile_includes_skills_and_formations() {
        let tmp = tempfile::tempdir().unwrap();
        extract_profile_to("scrum", tmp.path()).unwrap();

        // skills/ and formations/ directories
        assert!(tmp.path().join("skills").is_dir());
        assert!(tmp.path().join("formations").is_dir());

        // skills/knowledge-manager/SKILL.md should exist
        assert!(tmp.path().join("skills/knowledge-manager/SKILL.md").exists());

        // formations/local/formation.yml should exist
        assert!(tmp.path().join("formations/local/formation.yml").exists());

        // formations/k8s/formation.yml should exist
        assert!(tmp.path().join("formations/k8s/formation.yml").exists());
        assert!(tmp.path().join("formations/k8s/ralph.yml").exists());
        assert!(tmp.path().join("formations/k8s/PROMPT.md").exists());
    }

    #[test]
    fn extract_profile_scrum_compact_includes_expected_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        extract_profile_to("scrum-compact", tmp.path()).unwrap();

        assert!(tmp.path().join("skills").is_dir());
        assert!(tmp.path().join("formations").is_dir());
        assert!(tmp.path().join("skills/knowledge-manager/SKILL.md").exists());
        assert!(tmp.path().join("formations/local/formation.yml").exists());
    }

    #[test]
    fn require_current_schema_passes() {
        assert!(require_current_schema("my-team", "1.0").is_ok());
    }

    #[test]
    fn require_current_schema_fails_for_old() {
        let result = require_current_schema("my-team", "0.1");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires schema 1.0"));
        assert!(err.contains("my-team"));
        assert!(err.contains("0.1"));
        assert!(err.contains("bm upgrade"));
    }

    #[test]
    fn require_current_schema_fails_for_empty() {
        let result = require_current_schema("test-team", "");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("requires schema 1.0"));
    }

    // ── ViewDef tests ────────────────────────────────────────

    fn sample_statuses() -> Vec<StatusDef> {
        vec![
            StatusDef { name: "po:triage".into(), description: "".into() },
            StatusDef { name: "po:backlog".into(), description: "".into() },
            StatusDef { name: "arch:design".into(), description: "".into() },
            StatusDef { name: "arch:plan".into(), description: "".into() },
            StatusDef { name: "dev:implement".into(), description: "".into() },
            StatusDef { name: "done".into(), description: "".into() },
            StatusDef { name: "error".into(), description: "".into() },
        ]
    }

    #[test]
    fn view_resolve_single_prefix() {
        let view = ViewDef {
            name: "PO".into(),
            prefixes: vec!["po".into()],
            also_include: vec!["done".into(), "error".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["po:triage", "po:backlog", "done", "error"]);
    }

    #[test]
    fn view_resolve_multiple_prefixes() {
        let view = ViewDef {
            name: "Mixed".into(),
            prefixes: vec!["po".into(), "arch".into()],
            also_include: vec![],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["po:triage", "po:backlog", "arch:design", "arch:plan"]);
    }

    #[test]
    fn view_resolve_no_duplicates_in_also_include() {
        let view = ViewDef {
            name: "Dev".into(),
            prefixes: vec!["dev".into()],
            also_include: vec!["done".into(), "dev:implement".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        // dev:implement matched by prefix, should not appear twice from also_include
        assert_eq!(resolved, vec!["dev:implement", "done"]);
    }

    #[test]
    fn view_resolve_empty_prefixes_returns_only_also_include() {
        let view = ViewDef {
            name: "Bare".into(),
            prefixes: vec![],
            also_include: vec!["done".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["done"]);
    }

    #[test]
    fn view_resolve_no_match_returns_only_also_include() {
        let view = ViewDef {
            name: "NoMatch".into(),
            prefixes: vec!["nonexistent".into()],
            also_include: vec!["error".into()],
        };
        let resolved = view.resolve_statuses(&sample_statuses());
        assert_eq!(resolved, vec!["error"]);
    }

    #[test]
    fn view_filter_string_format() {
        let view = ViewDef {
            name: "Arch".into(),
            prefixes: vec!["arch".into()],
            also_include: vec!["done".into()],
        };
        let filter = view.filter_string(&sample_statuses());
        assert_eq!(filter, "status:arch:design,arch:plan,done");
    }

    #[test]
    fn rh_scrum_has_views() {
        let manifest = read_manifest("scrum").unwrap();
        assert!(!manifest.views.is_empty());
        // Should have at least PO, Architect, Developer views
        let names: Vec<&str> = manifest.views.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"PO"));
        assert!(names.contains(&"Architect"));
        assert!(names.contains(&"Developer"));
    }

    #[test]
    fn scrum_compact_has_views() {
        let manifest = read_manifest("scrum-compact").unwrap();
        assert!(!manifest.views.is_empty());
    }

    #[test]
    fn rh_scrum_po_view_resolves_all_po_statuses() {
        let manifest = read_manifest("scrum").unwrap();
        let po_view = manifest.views.iter().find(|v| v.name == "PO").unwrap();
        let resolved = po_view.resolve_statuses(&manifest.statuses);
        // Should include all po:* statuses plus done and error
        assert!(resolved.iter().all(|s| s.starts_with("po:") || s == "done" || s == "error"));
        assert!(resolved.contains(&"po:triage".to_string()));
        assert!(resolved.contains(&"po:merge".to_string()));
        assert!(resolved.contains(&"done".to_string()));
        assert!(resolved.contains(&"error".to_string()));
    }

    #[test]
    fn all_views_cover_done_and_error() {
        let manifest = read_manifest("scrum").unwrap();
        for view in &manifest.views {
            let resolved = view.resolve_statuses(&manifest.statuses);
            assert!(
                resolved.contains(&"done".to_string()),
                "View '{}' missing 'done'", view.name
            );
            assert!(
                resolved.contains(&"error".to_string()),
                "View '{}' missing 'error'", view.name
            );
        }
    }
}
