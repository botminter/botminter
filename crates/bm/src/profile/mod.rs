mod agent;
pub(crate) mod embedded;
mod extraction;
mod manifest;
mod member;
mod team_repo;

// Re-export public API
pub use agent::{
    ensure_minty_initialized, resolve_agent_from_profiles, resolve_coding_agent, scan_agent_tags,
};
pub use embedded::{
    extract_embedded_to_disk, extract_single_profile_to_disk, list_embedded_profiles,
    list_embedded_roles, minty_dir,
};
pub use embedded::minty::extract_minty_to_disk;
pub use extraction::{extract_member_to, extract_profile_from, extract_profile_to};
pub(crate) use extraction::extract_member_from;
pub use member::{auto_suffix, finalize_member_manifest, hire_member, HireResult};
pub(crate) use member::render_member_placeholders;
pub use manifest::{
    BridgeDef, CodingAgentDef, LabelDef, OperatorDef, ProfileManifest, ProjectDef, RoleDef,
    StatusDef, ViewDef,
};
pub use team_repo::{
    augment_manifest_with_projects, credentials_env, discover_member_dirs, gather_team_summary,
    infer_role_from_dir, list_files_in_dir, list_scope_files, list_subdirs, read_member_role,
    read_team_projects, read_team_repo_manifest, read_team_schema, record_bridge_in_manifest,
    validate_bridge_selection, validate_knowledge_path, TeamSummary,
};

use std::fs;
use std::io::{self, BufRead, IsTerminal, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result, bail};

/// Result of checking/initializing profiles.
#[derive(Debug)]
pub enum ProfileInitResult {
    /// Profiles were already current — no action needed.
    AlreadyCurrent,
    /// Profiles were freshly initialized (first time).
    Initialized { count: usize, path: PathBuf },
    /// Profiles were updated due to version mismatches.
    Updated { count: usize, path: PathBuf, mismatches: Vec<ProfileVersionMismatch> },
    /// User declined the update prompt.
    Declined,
    /// User declined initial setup.
    SetupDeclined,
}

/// A version mismatch between on-disk and embedded profiles.
#[derive(Debug)]
pub struct ProfileVersionMismatch {
    pub name: String,
    pub on_disk_version: String,
    pub embedded_version: String,
    pub is_downgrade: bool,
}

/// Compare two semver-like version strings (e.g. "1.0.0" vs "2.0.0").
/// Returns Ordering::Less if a < b, Equal if a == b, Greater if a > b.
/// Falls back to string comparison if either version isn't valid semver.
fn compare_versions(a: &str, b: &str) -> std::cmp::Ordering {
    let parse = |v: &str| -> Option<(u64, u64, u64)> {
        let parts: Vec<&str> = v.split('.').collect();
        if parts.len() != 3 {
            return None;
        }
        Some((
            parts[0].parse().ok()?,
            parts[1].parse().ok()?,
            parts[2].parse().ok()?,
        ))
    };
    match (parse(a), parse(b)) {
        (Some(av), Some(bv)) => av.cmp(&bv),
        _ => a.cmp(b),
    }
}

/// Returns the profiles directory for a given home path.
/// Computes `{home}/.config/botminter/profiles/` (Linux layout).
pub fn profiles_dir_for(home: &Path) -> PathBuf {
    home.join(".config").join("botminter").join("profiles")
}

/// Returns the names of all profiles installed on disk.
pub fn list_profiles() -> Result<Vec<String>> {
    list_profiles_from(&profiles_dir()?)
}

/// Returns the names of all profiles installed at a given base directory.
pub fn list_profiles_from(base: &Path) -> Result<Vec<String>> {
    if !base.is_dir() {
        bail!(
            "Profiles directory does not exist: {}\n\
             Run `bm profiles init` to extract profiles to disk.",
            base.display()
        );
    }
    let mut names: Vec<String> = fs::read_dir(base)
        .with_context(|| format!("Failed to read profiles directory: {}", base.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| {
            let name = e.file_name().into_string().ok()?;
            // Only include dirs that have botminter.yml (valid profiles)
            if e.path().join("botminter.yml").exists() {
                Some(name)
            } else {
                None
            }
        })
        .collect();
    names.sort();
    Ok(names)
}

/// Reads and parses the botminter.yml manifest for a named profile from disk.
pub fn read_manifest(name: &str) -> Result<ProfileManifest> {
    read_manifest_from(name, &profiles_dir()?)
}

/// Reads and parses the botminter.yml manifest for a named profile from a given base directory.
pub fn read_manifest_from(name: &str, base: &Path) -> Result<ProfileManifest> {
    let path = base.join(name).join("botminter.yml");
    let contents = fs::read_to_string(&path).with_context(|| {
        let available = list_profiles_from(base)
            .unwrap_or_default()
            .join(", ");
        format!(
            "Profile '{}' not found at {}. Available profiles: {}",
            name,
            path.display(),
            available
        )
    })?;

    let manifest: ProfileManifest =
        serde_yml::from_str(&contents).context("Failed to parse profile manifest")?;

    Ok(manifest)
}

/// Lists the role names available in a profile by reading its roles/ subdirectory on disk.
pub fn list_roles(name: &str) -> Result<Vec<String>> {
    list_roles_from(name, &profiles_dir()?)
}

/// Lists role names from a profile at a given base directory.
pub fn list_roles_from(name: &str, base: &Path) -> Result<Vec<String>> {
    let roles_dir = base.join(name).join("roles");
    if !roles_dir.is_dir() {
        bail!("Profile '{}' has no roles/ directory", name);
    }
    let mut roles: Vec<String> = fs::read_dir(&roles_dir)
        .with_context(|| format!("Failed to read roles directory: {}", roles_dir.display()))?
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
        .filter_map(|e| e.file_name().into_string().ok())
        .collect();
    roles.sort();
    Ok(roles)
}

/// Returns the canonical disk path for externalized profiles.
/// Resolves to `~/.config/botminter/profiles/` on Linux/macOS.
pub fn profiles_dir() -> Result<PathBuf> {
    let config = dirs::config_dir().context("Could not determine config directory")?;
    Ok(config.join("botminter").join("profiles"))
}

/// Ensures profiles are available on disk. If missing, prompts the user (TTY) or
/// auto-initializes (non-TTY). Call at the top of any command that reads profiles.
///
/// Returns a `ProfileInitResult` describing what happened. The caller can
/// use this to display appropriate messages.
pub fn ensure_profiles_initialized() -> Result<ProfileInitResult> {
    ensure_profiles_initialized_with(
        &profiles_dir()?,
        io::stdin().is_terminal(),
        false,
        &mut io::stdin().lock(),
        &mut io::stderr(),
    )
}

fn ensure_profiles_initialized_with(
    profiles_path: &Path,
    is_tty: bool,
    force: bool,
    reader: &mut dyn BufRead,
    writer: &mut dyn Write,
) -> Result<ProfileInitResult> {
    // Already initialized — at least one profile subdirectory exists
    if profiles_path.is_dir() {
        let has_profile = fs::read_dir(profiles_path)
            .map(|entries| {
                entries
                    .filter_map(|e| e.ok())
                    .any(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            })
            .unwrap_or(false);
        if has_profile {
            // Check version of each embedded profile against on-disk version
            let mut mismatches: Vec<ProfileVersionMismatch> = Vec::new();
            for name in embedded::list_embedded_profiles() {
                let embedded_ver = match embedded::embedded_profile_version(&name) {
                    Some(v) => v,
                    None => continue, // skip profiles without version
                };
                let on_disk_ver = match read_manifest_from(&name, profiles_path) {
                    Ok(m) => m.version,
                    Err(_) => String::new(), // missing/corrupt → treat as mismatch
                };
                if on_disk_ver != embedded_ver {
                    let is_downgrade = compare_versions(&on_disk_ver, &embedded_ver) == std::cmp::Ordering::Greater;
                    mismatches.push(ProfileVersionMismatch {
                        name,
                        on_disk_version: on_disk_ver,
                        embedded_version: embedded_ver,
                        is_downgrade,
                    });
                }
            }

            if mismatches.is_empty() {
                return Ok(ProfileInitResult::AlreadyCurrent);
            }

            if force || !is_tty {
                // Auto re-extract
                let count = embedded::extract_embedded_to_disk(profiles_path)?;
                return Ok(ProfileInitResult::Updated {
                    count,
                    path: profiles_path.to_path_buf(),
                    mismatches,
                });
            } else {
                // Interactive — display mismatches and prompt
                for m in &mismatches {
                    let on_disk_display = if m.on_disk_version.is_empty() {
                        "unknown".to_string()
                    } else {
                        format!("v{}", m.on_disk_version)
                    };
                    let downgrade_label = if m.is_downgrade { " \u{26a0} this is a downgrade" } else { "" };
                    writeln!(
                        writer,
                        "Profile '{}': found {}, installing v{}{}",
                        m.name, on_disk_display, m.embedded_version, downgrade_label
                    )?;
                }
                write!(writer, "Update profiles? [y/N] ")?;
                writer.flush()?;
                let mut line = String::new();
                reader.read_line(&mut line)?;
                let answer = line.trim().to_lowercase();
                if answer == "y" || answer == "yes" {
                    let count = embedded::extract_embedded_to_disk(profiles_path)?;
                    return Ok(ProfileInitResult::Updated {
                        count,
                        path: profiles_path.to_path_buf(),
                        mismatches,
                    });
                } else {
                    return Ok(ProfileInitResult::Declined);
                }
            }
        }
    }

    if is_tty {
        write!(writer, "Profiles not initialized. Initialize now? [Y/n] ")?;
        writer.flush()?;

        let mut line = String::new();
        let bytes = reader.read_line(&mut line)?;
        let answer = line.trim().to_lowercase();
        if bytes > 0 && (answer == "n" || answer == "no") {
            return Ok(ProfileInitResult::SetupDeclined);
        }
    }

    // Initialize profiles from embedded data
    fs::create_dir_all(profiles_path).with_context(|| {
        format!(
            "Failed to create profiles directory {}",
            profiles_path.display()
        )
    })?;
    let count = embedded::extract_embedded_to_disk(profiles_path)?;
    Ok(ProfileInitResult::Initialized {
        count,
        path: profiles_path.to_path_buf(),
    })
}

/// Checks that the disk profile's schema_version matches the expected value.
/// Returns an error suggesting `bm upgrade` on mismatch.
pub fn check_schema_version(profile_name: &str, team_schema: &str) -> Result<()> {
    check_schema_version_in(profile_name, team_schema, &profiles_dir()?)
}

fn check_schema_version_in(profile_name: &str, team_schema: &str, base: &Path) -> Result<()> {
    let manifest = read_manifest_from(profile_name, base)?;
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

/// Reads a team repo's `botminter.yml` manifest and validates its schema version
/// against the embedded profile. Returns the schema version string and the profile
/// name from the manifest (falling back to the team's configured profile).
///
/// This encapsulates the manifest-read-and-validate pattern used by multiple commands
/// (start, hire, teams sync).
pub fn validate_team_manifest(
    team_repo: &Path,
    team_profile: &str,
) -> Result<TeamManifestInfo> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        bail!(
            "Team repo at {} has no botminter.yml. Is this a valid team repo?",
            team_repo.display()
        );
    }
    let manifest_contents = std::fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml")?;
    let team_manifest: serde_yml::Value =
        serde_yml::from_str(&manifest_contents).context("Failed to parse team botminter.yml")?;
    let schema_version = team_manifest["schema_version"]
        .as_str()
        .unwrap_or("")
        .to_string();
    let profile = team_manifest["profile"]
        .as_str()
        .unwrap_or(team_profile)
        .to_string();

    check_schema_version(&profile, &schema_version)?;

    Ok(TeamManifestInfo {
        schema_version,
        profile,
    })
}

/// Parsed and validated team manifest metadata.
pub struct TeamManifestInfo {
    pub schema_version: String,
    pub profile: String,
}

/// Shared test helpers for profile sub-module tests.
#[cfg(test)]
pub(crate) mod test_support {
    use std::path::{Path, PathBuf};

    use super::manifest::CodingAgentDef;

    /// Returns the default Claude Code agent definition for tests.
    pub fn claude_code_agent() -> CodingAgentDef {
        CodingAgentDef {
            name: "claude-code".into(),
            display_name: "Claude Code".into(),
            context_file: "CLAUDE.md".into(),
            agent_dir: ".claude".into(),
            binary: "claude".into(),
            system_prompt_flag: Some("--append-system-prompt-file".into()),
            skip_permissions_flag: Some("--dangerously-skip-permissions".into()),
        }
    }

    /// Extracts embedded profiles to a tempdir for disk-based testing.
    /// Returns (tempdir_handle, path_to_profiles_dir).
    pub fn setup_disk_profiles() -> (tempfile::TempDir, PathBuf) {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().to_path_buf();
        super::embedded::extract_embedded_to_disk(&profiles_path).unwrap();
        (tmp, profiles_path)
    }

    /// Recursively collect all file paths under a directory.
    pub fn collect_files_recursive_disk(dir: &Path) -> Vec<PathBuf> {
        let mut files = Vec::new();
        if let Ok(entries) = std::fs::read_dir(dir) {
            for entry in entries.flatten() {
                let path = entry.path();
                if path.is_dir() {
                    files.extend(collect_files_recursive_disk(&path));
                } else {
                    files.push(path);
                }
            }
        }
        files
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use test_support::*;

    #[test]
    fn all_embedded_profiles_are_discoverable() {
        let (_tmp, base) = setup_disk_profiles();
        let profiles = list_profiles_from(&base).unwrap();
        let emb = embedded::list_embedded_profiles();
        assert_eq!(
            profiles.len(),
            emb.len(),
            "list_profiles_in should find all embedded profiles"
        );
        for name in &emb {
            assert!(
                profiles.contains(name),
                "Profile '{}' should be discoverable",
                name
            );
        }
    }

    #[test]
    fn list_profiles_is_sorted() {
        let (_tmp, base) = setup_disk_profiles();
        let profiles = list_profiles_from(&base).unwrap();
        let mut sorted = profiles.clone();
        sorted.sort();
        assert_eq!(
            profiles, sorted,
            "list_profiles_in should return profiles in sorted order"
        );
    }

    #[test]
    fn all_profiles_manifests_parse() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base)
                .unwrap_or_else(|e| panic!("Profile '{}' manifest should parse: {}", name, e));
            assert_eq!(
                manifest.name, name,
                "Profile '{}' name should match directory",
                name
            );
            assert!(
                !manifest.display_name.is_empty(),
                "Profile '{}' should have display_name",
                name
            );
            assert!(
                !manifest.version.is_empty(),
                "Profile '{}' should have version",
                name
            );
            assert!(
                !manifest.schema_version.is_empty(),
                "Profile '{}' should have schema_version",
                name
            );
            assert!(
                !manifest.description.is_empty(),
                "Profile '{}' should have description",
                name
            );
            assert!(
                !manifest.roles.is_empty(),
                "Profile '{}' should have at least one role",
                name
            );
        }
    }

    #[test]
    fn read_manifest_roles_have_descriptions() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
        for role in &manifest.roles {
            assert!(!role.name.is_empty());
            assert!(!role.description.is_empty());
        }
    }

    #[test]
    fn read_manifest_labels_have_required_fields() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
        for label in &manifest.labels {
            assert!(!label.name.is_empty());
            assert!(!label.color.is_empty());
            assert!(!label.description.is_empty());
        }
    }

    #[test]
    fn read_manifest_nonexistent_profile_errors() {
        let (_tmp, base) = setup_disk_profiles();
        let result = read_manifest_from("nonexistent", &base);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not found"));
        for name in list_profiles_from(&base).unwrap() {
            assert!(
                err.contains(&name),
                "Error should list available profile '{}': {}",
                name,
                err
            );
        }
    }

    #[test]
    fn all_profiles_list_roles_matches_manifest() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let roles = list_roles_from(&name, &base).unwrap();
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert_eq!(
                roles.len(),
                manifest.roles.len(),
                "Profile '{}': list_roles count should match manifest roles count",
                name
            );
            for role in &manifest.roles {
                assert!(
                    roles.contains(&role.name),
                    "Profile '{}': list_roles should include role '{}'",
                    name,
                    role.name
                );
            }
        }
    }

    #[test]
    fn all_profiles_have_labels() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert!(
                !manifest.labels.is_empty(),
                "Profile '{}' should have at least one label",
                name
            );
        }
    }

    #[test]
    fn all_profiles_have_statuses() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert!(
                !manifest.statuses.is_empty(),
                "Profile '{}' should have at least one status",
                name
            );
        }
    }

    #[test]
    fn read_manifest_statuses_have_required_fields() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
        for status in &manifest.statuses {
            assert!(!status.name.is_empty());
            assert!(!status.description.is_empty());
        }
    }

    #[test]
    fn check_schema_version_match() {
        let (_tmp, base) = setup_disk_profiles();
        assert!(check_schema_version_in("scrum", "1.0", &base).is_ok());
        assert!(check_schema_version_in("agentic-sdlc-minimal", "1.0", &base).is_ok());
    }

    #[test]
    fn check_schema_version_mismatch() {
        let (_tmp, base) = setup_disk_profiles();
        let result = check_schema_version_in("scrum", "99.0", &base);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bm upgrade"));
    }

    #[test]
    fn check_schema_version_old_team_against_current_profile() {
        let (_tmp, base) = setup_disk_profiles();
        let result = check_schema_version_in("scrum", "0.1", &base);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("bm upgrade"));
        assert!(err.contains("0.1"));
        assert!(err.contains("1.0"));
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

    // ── View tests that require disk profiles ────────────────────

    #[test]
    fn rh_scrum_has_views() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
        assert!(!manifest.views.is_empty());
        let names: Vec<&str> = manifest.views.iter().map(|v| v.name.as_str()).collect();
        assert!(names.contains(&"PO"));
        assert!(names.contains(&"Architect"));
        assert!(names.contains(&"Developer"));
    }

    #[test]
    fn scrum_compact_has_views() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("agentic-sdlc-minimal", &base).unwrap();
        assert!(!manifest.views.is_empty());
    }

    #[test]
    fn rh_scrum_po_view_resolves_all_po_statuses() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
        let po_view = manifest.views.iter().find(|v| v.name == "PO").unwrap();
        let resolved = po_view.resolve_statuses(&manifest.statuses);
        assert!(resolved.iter().all(|s| s.starts_with("po:") || s == "done" || s == "error"));
        assert!(resolved.contains(&"po:triage".to_string()));
        assert!(resolved.contains(&"po:merge".to_string()));
        assert!(resolved.contains(&"done".to_string()));
        assert!(resolved.contains(&"error".to_string()));
    }

    #[test]
    fn all_views_cover_done_and_error() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = read_manifest_from("scrum", &base).unwrap();
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

    // ── CodingAgentDef tests ────────────────────────────────────

    #[test]
    fn all_profiles_declare_coding_agents() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert!(
                !manifest.coding_agents.is_empty(),
                "Profile '{}' has no coding_agents", name
            );
            assert!(
                !manifest.default_coding_agent.is_empty(),
                "Profile '{}' has no default_coding_agent", name
            );
        }
    }

    #[test]
    fn all_profiles_have_claude_code_agent() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            let agent = manifest.coding_agents.get("claude-code");
            assert!(agent.is_some(), "Profile '{}' missing claude-code agent", name);
            let agent = agent.unwrap();
            assert_eq!(agent.name, "claude-code");
            assert_eq!(agent.display_name, "Claude Code");
            assert_eq!(agent.context_file, "CLAUDE.md");
            assert_eq!(agent.agent_dir, ".claude");
            assert_eq!(agent.binary, "claude");
        }
    }

    #[test]
    fn all_profiles_default_to_claude_code() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert_eq!(
                manifest.default_coding_agent, "claude-code",
                "Profile '{}' default_coding_agent is not claude-code", name
            );
        }
    }

    #[test]
    fn schema_version_remains_1_0() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            assert_eq!(
                manifest.schema_version, "1.0",
                "Profile '{}' schema_version changed from 1.0", name
            );
        }
    }

    #[test]
    fn profiles_dir_returns_config_path() {
        let dir = profiles_dir().unwrap();
        let path_str = dir.to_string_lossy();
        assert!(
            path_str.contains("botminter") && path_str.contains("profiles"),
            "profiles_dir should contain botminter/profiles: got {}", path_str
        );
    }

    // ── ensure_profiles_initialized tests ────────────────────────

    #[test]
    fn ensure_profiles_initialized_skips_when_profiles_exist() {
        let (_tmp, base) = setup_disk_profiles();
        let mut reader = io::Cursor::new(b"");
        let result = ensure_profiles_initialized_with(&base, true, false, &mut reader, &mut io::sink());
        assert!(result.is_ok(), "Should return Ok when profiles exist");
    }

    #[test]
    fn ensure_profiles_initialized_extracts_on_yes() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        let mut reader = io::Cursor::new(b"y\n");
        let result = ensure_profiles_initialized_with(&profiles_path, true, false, &mut reader, &mut io::sink());
        assert!(result.is_ok(), "Should succeed after yes: {:?}", result.err());

        let names = list_profiles_from(&profiles_path).unwrap();
        assert!(!names.is_empty(), "Profiles should be extracted");
    }

    #[test]
    fn ensure_profiles_initialized_extracts_on_default_enter() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        let mut reader = io::Cursor::new(b"\n");
        let result = ensure_profiles_initialized_with(&profiles_path, true, false, &mut reader, &mut io::sink());
        assert!(result.is_ok(), "Empty enter (default Y) should extract");

        let names = list_profiles_from(&profiles_path).unwrap();
        assert!(!names.is_empty(), "Profiles should be extracted on default");
    }

    #[test]
    fn ensure_profiles_initialized_auto_inits_non_tty() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        let mut reader = io::Cursor::new(b"");
        let result = ensure_profiles_initialized_with(&profiles_path, false, false, &mut reader, &mut io::sink());
        assert!(result.is_ok(), "Non-TTY should auto-init: {:?}", result.err());

        let names = list_profiles_from(&profiles_path).unwrap();
        assert!(!names.is_empty(), "Non-TTY should extract profiles");
    }

    #[test]
    fn ensure_profiles_initialized_empty_dir_triggers_init() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        let mut reader = io::Cursor::new(b"");
        let result = ensure_profiles_initialized_with(&profiles_path, false, false, &mut reader, &mut io::sink());
        assert!(result.is_ok());

        let names = list_profiles_from(&profiles_path).unwrap();
        assert!(!names.is_empty(), "Empty profiles dir should trigger init");
    }

    // ── staleness detection tests ────────────────────────────────

    #[test]
    fn ensure_profiles_initialized_re_extracts_when_version_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let manifest = profiles_path.join(sample).join("botminter.yml");
        fs::remove_file(&manifest).unwrap();

        for name in embedded::list_embedded_profiles() {
            let roles = profiles_path.join(&name).join("roles");
            let members = profiles_path.join(&name).join("members");
            if roles.exists() {
                fs::rename(&roles, &members).unwrap();
            }
        }

        let mut reader = io::Cursor::new(b"");
        ensure_profiles_initialized_with(&profiles_path, false, false, &mut reader, &mut io::sink()).unwrap();

        for name in embedded::list_embedded_profiles() {
            let roles = profiles_path.join(&name).join("roles");
            assert!(
                roles.is_dir(),
                "roles/ should exist after re-extraction for {}",
                name
            );
        }
    }

    #[test]
    fn ensure_profiles_initialized_re_extracts_when_version_mismatch() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let manifest_path = profiles_path.join(sample).join("botminter.yml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let modified = content.replace(
            &format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap()),
            "version: \"0.0.0\"",
        );
        fs::write(&manifest_path, &modified).unwrap();

        let sentinel = profiles_path.join(sample).join("PROCESS.md");
        if sentinel.exists() {
            fs::write(&sentinel, "corrupted-sentinel").unwrap();
        }

        let mut reader = io::Cursor::new(b"");
        ensure_profiles_initialized_with(&profiles_path, false, false, &mut reader, &mut io::sink()).unwrap();

        let restored = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            restored.contains("name:"),
            "botminter.yml should be restored after re-extraction"
        );
    }

    #[test]
    fn ensure_profiles_initialized_skips_when_version_current() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let sentinel = profiles_path.join(sample).join("_sentinel.txt");
        fs::write(&sentinel, "keep-me").unwrap();

        let mut reader = io::Cursor::new(b"");
        ensure_profiles_initialized_with(&profiles_path, false, false, &mut reader, &mut io::sink()).unwrap();

        assert!(
            sentinel.exists(),
            "Sentinel file should be preserved when versions match"
        );
    }

    #[test]
    fn ensure_profiles_initialized_prompts_on_version_mismatch_tty() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let manifest_path = profiles_path.join(sample).join("botminter.yml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let modified = content.replace(
            &format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap()),
            "version: \"0.0.0\"",
        );
        fs::write(&manifest_path, &modified).unwrap();

        let mut reader = io::Cursor::new(b"y\n");
        ensure_profiles_initialized_with(&profiles_path, true, false, &mut reader, &mut io::sink()).unwrap();

        let restored = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            restored.contains(&format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap())),
            "botminter.yml should have embedded version after user confirmed update"
        );
    }

    #[test]
    fn ensure_profiles_initialized_skips_on_version_mismatch_tty_declined() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let manifest_path = profiles_path.join(sample).join("botminter.yml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let modified = content.replace(
            &format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap()),
            "version: \"0.0.0\"",
        );
        fs::write(&manifest_path, &modified).unwrap();

        let mut reader = io::Cursor::new(b"n\n");
        ensure_profiles_initialized_with(&profiles_path, true, false, &mut reader, &mut io::sink()).unwrap();

        let preserved = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            preserved.contains("version: \"0.0.0\""),
            "botminter.yml should keep stale version when user declines update"
        );
    }

    #[test]
    fn ensure_profiles_initialized_force_skips_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_path = tmp.path().join("profiles");
        fs::create_dir_all(&profiles_path).unwrap();
        embedded::extract_embedded_to_disk(&profiles_path).unwrap();

        let sample = &embedded::list_embedded_profiles()[0];
        let manifest_path = profiles_path.join(sample).join("botminter.yml");
        let content = fs::read_to_string(&manifest_path).unwrap();
        let modified = content.replace(
            &format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap()),
            "version: \"0.0.0\"",
        );
        fs::write(&manifest_path, &modified).unwrap();

        let mut reader = io::Cursor::new(b"");
        ensure_profiles_initialized_with(&profiles_path, true, true, &mut reader, &mut io::sink()).unwrap();

        let restored = fs::read_to_string(&manifest_path).unwrap();
        assert!(
            restored.contains(&format!("version: \"{}\"", embedded::embedded_profile_version(sample).unwrap())),
            "botminter.yml should have embedded version after force update"
        );
    }

    // ── BridgeDef profile-based tests ───────────────

    #[test]
    fn embedded_profiles_with_bridges_parse() {
        let (_tmp, base) = setup_disk_profiles();
        for name in list_profiles_from(&base).unwrap() {
            let manifest = read_manifest_from(&name, &base).unwrap();
            if !manifest.bridges.is_empty() {
                for bridge in &manifest.bridges {
                    assert!(!bridge.name.is_empty(), "Bridge name should not be empty in profile '{}'", name);
                    assert!(!bridge.bridge_type.is_empty(), "Bridge type should not be empty in profile '{}'", name);
                }
            }
        }
    }
}
