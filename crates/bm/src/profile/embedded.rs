use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use include_dir::{Dir, include_dir};

static PROFILES: Dir<'static> = include_dir!("$BM_PROFILES_DIR");

/// Returns the names of all embedded profiles.
pub fn list_embedded_profiles() -> Vec<String> {
    let mut names: Vec<String> = PROFILES
        .dirs()
        .map(|d| d.path().file_name().unwrap().to_string_lossy().to_string())
        .collect();
    names.sort();
    names
}

/// Returns the raw embedded PROFILES directory for advanced access.
#[allow(dead_code)]
pub fn embedded_profiles() -> &'static Dir<'static> {
    &PROFILES
}

/// Extracts all embedded profiles to a target directory, writing files verbatim
/// (no agent tag filtering, no context.md rename). Used by `bm profiles init`.
pub fn extract_embedded_to_disk(target: &Path) -> Result<usize> {
    let mut count = 0;
    for profile_dir in PROFILES.dirs() {
        let profile_name = profile_dir
            .path()
            .file_name()
            .unwrap()
            .to_string_lossy()
            .to_string();
        let profile_target = target.join(&profile_name);
        write_dir_verbatim(profile_dir, &profile_target, profile_dir.path())?;
        count += 1;
    }

    Ok(count)
}

/// Extracts a single embedded profile by name to a target directory, writing
/// files verbatim. Returns an error if the profile doesn't exist.
pub fn extract_single_profile_to_disk(profile_name: &str, target: &Path) -> Result<()> {
    let profile_dir = PROFILES.get_dir(profile_name).with_context(|| {
        let available = list_embedded_profiles().join(", ");
        format!(
            "Profile '{}' not found. Available profiles: {}",
            profile_name, available
        )
    })?;
    let profile_target = target.join(profile_name);
    write_dir_verbatim(profile_dir, &profile_target, profile_dir.path())
}

/// Returns the role names for a given embedded profile.
/// If the profile or its roles/ directory is not found, returns an empty Vec.
pub fn list_embedded_roles(name: &str) -> Vec<String> {
    let roles_path = format!("{}/roles", name);
    let Some(roles_dir) = PROFILES.get_dir(&roles_path) else {
        return Vec::new();
    };
    let mut roles: Vec<String> = roles_dir
        .dirs()
        .filter_map(|d| {
            d.path()
                .file_name()
                .map(|f| f.to_string_lossy().to_string())
        })
        .collect();
    roles.sort();
    roles
}

/// Recursively writes all files from an embedded Dir to disk without any
/// filtering or renaming. Binary files and text files are both written as-is.
fn write_dir_verbatim(dir: &Dir<'_>, base_target: &Path, root_path: &Path) -> Result<()> {
    for file in dir.files() {
        let rel = file
            .path()
            .strip_prefix(root_path)
            .unwrap_or(file.path());
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

    for sub_dir in dir.dirs() {
        write_dir_verbatim(sub_dir, base_target, root_path)?;
    }

    Ok(())
}

/// Embedded Minty config from the binary. Extracted alongside profiles by `bm profiles init`.
pub(crate) mod minty {
    use std::fs;
    use std::path::Path;

    use anyhow::{Context, Result};
    use include_dir::{Dir, include_dir};

    static MINTY: Dir<'static> = include_dir!("$CARGO_MANIFEST_DIR/../../minty");

    /// Extracts the embedded Minty config to a target directory, writing files verbatim.
    /// Target should be `~/.config/botminter/minty/`.
    pub fn extract_minty_to_disk(target: &Path) -> Result<()> {
        write_dir_verbatim(&MINTY, target, MINTY.path())
    }

    /// Recursively writes all files from an embedded Dir to disk without any
    /// filtering or renaming.
    fn write_dir_verbatim(dir: &Dir<'_>, base_target: &Path, root_path: &Path) -> Result<()> {
        for file in dir.files() {
            let rel = file
                .path()
                .strip_prefix(root_path)
                .unwrap_or(file.path());
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

        for sub_dir in dir.dirs() {
            write_dir_verbatim(sub_dir, base_target, root_path)?;
        }

        Ok(())
    }
}

/// Returns the canonical disk path for Minty config.
/// Resolves to `~/.config/botminter/minty/` on Linux/macOS.
pub fn minty_dir() -> Result<PathBuf> {
    Ok(crate::profile::botminter_config_dir()?.join("minty"))
}

/// Reads the version field from an embedded profile's botminter.yml.
pub(super) fn embedded_profile_version(name: &str) -> Option<String> {
    let path = format!("{}/botminter.yml", name);
    let file = embedded_profiles().get_file(&path)?;
    let content = std::str::from_utf8(file.contents()).ok()?;
    let manifest: crate::profile::ProfileManifest = serde_yml::from_str(content).ok()?;
    Some(manifest.version)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::test_support::*;

    #[test]
    fn extract_embedded_to_disk_creates_all_profiles() {
        let tmp = tempfile::tempdir().unwrap();
        let count = extract_embedded_to_disk(tmp.path()).unwrap();

        let expected_profiles = list_embedded_profiles();
        assert_eq!(count, expected_profiles.len());

        for name in &expected_profiles {
            assert!(
                tmp.path().join(name).is_dir(),
                "Profile '{}' should be extracted as a directory", name
            );
        }
    }

    #[test]
    fn extract_embedded_to_disk_preserves_botminter_yml() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        for name in list_embedded_profiles() {
            let manifest_path = tmp.path().join(&name).join("botminter.yml");
            assert!(manifest_path.exists(), "Profile '{}' should have botminter.yml", name);
            let contents = std::fs::read_to_string(&manifest_path).unwrap();
            let manifest: crate::profile::ProfileManifest = serde_yml::from_str(&contents).unwrap();
            assert_eq!(manifest.name, name);
        }
    }

    #[test]
    fn extract_embedded_to_disk_content_matches_embedded() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        let profile = &list_embedded_profiles()[0];
        let embedded_dir = embedded_profiles();
        let file_path = format!("{}/botminter.yml", profile);
        let embedded_bytes = embedded_dir
            .get_file(&file_path)
            .unwrap()
            .contents();
        let disk = std::fs::read(tmp.path().join(profile).join("botminter.yml")).unwrap();
        assert_eq!(embedded_bytes, disk.as_slice(), "Extracted content should be byte-identical");
    }

    #[test]
    fn extract_embedded_to_disk_preserves_context_md_name() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        for name in list_embedded_profiles() {
            let context_path = tmp.path().join(&name).join("context.md");
            assert!(context_path.exists(), "Profile '{}' should keep context.md (not renamed)", name);
            assert!(
                !tmp.path().join(&name).join("CLAUDE.md").exists(),
                "Profile '{}' should not have CLAUDE.md (no rename during init)", name
            );
        }
    }

    #[test]
    fn extract_embedded_to_disk_preserves_agent_tags() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        let profile = &list_embedded_profiles()[0];
        let content = std::fs::read_to_string(tmp.path().join(profile).join("context.md")).unwrap();
        let embedded_dir = embedded_profiles();
        let file_path = format!("{}/context.md", profile);
        let embedded_content = embedded_dir
            .get_file(&file_path)
            .unwrap()
            .contents_utf8()
            .unwrap();
        assert_eq!(content, embedded_content, "Agent tags should be preserved verbatim");
    }

    #[test]
    fn extract_embedded_to_disk_preserves_schema_dir() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        for name in list_embedded_profiles() {
            let schema_dir = tmp.path().join(&name).join(".schema");
            assert!(schema_dir.is_dir(), "Profile '{}' should have .schema/ directory", name);
            assert!(schema_dir.join("v1.yml").exists(), "Profile '{}' should have .schema/v1.yml", name);
        }
    }

    #[test]
    fn extract_embedded_to_disk_preserves_roles_dir() {
        let tmp = tempfile::tempdir().unwrap();
        extract_embedded_to_disk(tmp.path()).unwrap();

        for name in list_embedded_profiles() {
            let roles_dir = tmp.path().join(&name).join("roles");
            assert!(roles_dir.is_dir(), "Profile '{}' should have roles/ directory", name);
        }
    }

    #[test]
    fn extract_embedded_to_disk_creates_parent_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let nested = tmp.path().join("deep").join("nested").join("profiles");
        extract_embedded_to_disk(&nested).unwrap();

        let profile = &list_embedded_profiles()[0];
        assert!(nested.is_dir(), "Nested target should be created");
        assert!(nested.join(profile).join("botminter.yml").exists(), "Profiles should be extracted into nested target");
    }

    #[test]
    fn init_resolves_default_coding_agent_from_manifest() {
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let manifest = crate::profile::read_manifest_from(&name, &base).unwrap();
            assert!(
                !manifest.default_coding_agent.is_empty(),
                "Profile '{}' should declare a default_coding_agent", name
            );
            let agent = manifest.coding_agents.get(&manifest.default_coding_agent);
            assert!(
                agent.is_some(),
                "Profile '{}' default_coding_agent '{}' should exist in coding_agents map",
                name, manifest.default_coding_agent
            );
            let agent = agent.unwrap();
            assert_eq!(
                agent.context_file, "CLAUDE.md",
                "Profile '{}' default agent should have CLAUDE.md context_file", name
            );
        }
    }

    #[test]
    fn workflow_dot_files_cover_all_statuses() {
        let embedded = embedded_profiles();
        for profile_name in list_embedded_profiles() {
            // Read the manifest to get all statuses dynamically
            let manifest_path = format!("{}/botminter.yml", profile_name);
            let manifest_file = embedded.get_file(&manifest_path)
                .unwrap_or_else(|| panic!("Profile '{}' should have botminter.yml", profile_name));
            let manifest: crate::profile::ProfileManifest =
                serde_yml::from_str(manifest_file.contents_utf8().unwrap()).unwrap();

            // Collect all DOT file contents from workflows/
            let workflows_path = format!("{}/workflows", profile_name);
            let workflows_dir = embedded.get_dir(&workflows_path);
            assert!(
                workflows_dir.is_some(),
                "Profile '{}' should have a workflows/ directory", profile_name
            );
            let workflows_dir = workflows_dir.unwrap();

            let dot_files: Vec<_> = workflows_dir.files()
                .filter(|f| f.path().extension().map_or(false, |e| e == "dot"))
                .collect();
            assert!(
                !dot_files.is_empty(),
                "Profile '{}' should have at least one .dot file in workflows/", profile_name
            );

            // Concatenate all DOT file contents
            let all_dot_content: String = dot_files.iter()
                .map(|f| f.contents_utf8().unwrap())
                .collect::<Vec<_>>()
                .join("\n");

            // Verify every status from the manifest appears in at least one DOT file
            for status in &manifest.statuses {
                assert!(
                    all_dot_content.contains(&format!("\"{}\"", status.name)),
                    "Profile '{}': status '{}' not found in any workflow DOT file",
                    profile_name, status.name
                );
            }
        }
    }

    #[test]
    fn workflow_dot_files_are_valid_syntax() {
        let embedded = embedded_profiles();
        for profile_name in list_embedded_profiles() {
            let workflows_path = format!("{}/workflows", profile_name);
            let workflows_dir = embedded.get_dir(&workflows_path)
                .unwrap_or_else(|| panic!("Profile '{}' should have workflows/", profile_name));

            for file in workflows_dir.files() {
                if file.path().extension().map_or(true, |e| e != "dot") {
                    continue;
                }
                let content = file.contents_utf8().unwrap();
                let filename = file.path().file_name().unwrap().to_string_lossy();

                // Basic DOT syntax validation
                assert!(
                    content.contains("digraph "),
                    "Profile '{}' file '{}' should contain a digraph declaration",
                    profile_name, filename
                );
                assert!(
                    content.contains("rankdir=LR"),
                    "Profile '{}' file '{}' should use left-to-right layout",
                    profile_name, filename
                );

                // Balanced braces
                let opens = content.chars().filter(|c| *c == '{').count();
                let closes = content.chars().filter(|c| *c == '}').count();
                assert_eq!(
                    opens, closes,
                    "Profile '{}' file '{}' should have balanced braces (found {} open, {} close)",
                    profile_name, filename, opens, closes
                );
            }
        }
    }
}
