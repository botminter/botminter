use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::manifest::CodingAgentDef;
use super::{list_profiles_from, list_roles_from, profiles_dir};
use crate::agent_tags;

/// File extensions that should be filtered through the agent tag pipeline.
const FILTERABLE_EXTENSIONS: &[&str] = &["md", "yml", "yaml", "sh"];

/// Returns true if the filename has an extension that should be agent-tag filtered.
pub(super) fn should_filter(filename: &str) -> bool {
    Path::new(filename)
        .extension()
        .and_then(|e| e.to_str())
        .map(|e| FILTERABLE_EXTENSIONS.contains(&e.to_ascii_lowercase().as_str()))
        .unwrap_or(false)
}

/// Extracts a profile's team-repo content to the target directory.
/// Copies everything from the disk profile EXCEPT `roles/` and `.schema/`
/// (role skeletons are extracted on demand via `extract_member_to`; schema is internal).
///
/// Text files (`.md`, `.yml`, `.yaml`, `.sh`) are filtered through the agent tag
/// pipeline to strip non-matching agent sections. `context.md` is additionally
/// renamed to `coding_agent.context_file` (e.g., `AGENTS.md` for Claude Code).
pub fn extract_profile_to(
    profile_name: &str,
    target: &Path,
    coding_agent: &CodingAgentDef,
) -> Result<()> {
    extract_profile_from(&profiles_dir()?, profile_name, target, coding_agent)
}

/// Extracts a profile's team-repo content from a specific profiles base directory.
pub fn extract_profile_from(
    base: &Path,
    profile_name: &str,
    target: &Path,
    coding_agent: &CodingAgentDef,
) -> Result<()> {
    let profile_dir = base.join(profile_name);
    if !profile_dir.is_dir() {
        let available = list_profiles_from(base).unwrap_or_default().join(", ");
        bail!(
            "Profile '{}' not found. Available profiles: {}",
            profile_name, available
        );
    }

    extract_dir_recursive_from_disk(&profile_dir, target, &profile_dir, coding_agent, &|rel_path| {
        let first = rel_path
            .components()
            .next()
            .map(|c| c.as_os_str().to_string_lossy().to_string());
        matches!(first.as_deref(), Some("roles") | Some(".schema"))
    })?;

    Ok(())
}

/// Extracts a member skeleton from the disk profile into the target directory.
/// Copies the contents of `profiles/{profile}/roles/{role}/` to `target/`.
///
/// Text files are filtered through the agent tag pipeline, and `context.md` is
/// renamed to `coding_agent.context_file`.
pub fn extract_member_to(
    profile_name: &str,
    role: &str,
    target: &Path,
    coding_agent: &CodingAgentDef,
) -> Result<()> {
    extract_member_from(&profiles_dir()?, profile_name, role, target, coding_agent)
}

pub(crate) fn extract_member_from(
    base: &Path,
    profile_name: &str,
    role: &str,
    target: &Path,
    coding_agent: &CodingAgentDef,
) -> Result<()> {
    let member_dir = base.join(profile_name).join("roles").join(role);
    if !member_dir.is_dir() {
        let roles = list_roles_from(profile_name, base).unwrap_or_default().join(", ");
        bail!(
            "Role '{}' not available in profile '{}'. Available roles: {}",
            role, profile_name, roles
        );
    }

    extract_dir_recursive_from_disk(&member_dir, target, &member_dir, coding_agent, &|_| false)?;
    Ok(())
}

/// Recursively extracts files from a disk directory to a target path.
/// `root_path` is the path of the root directory being extracted (used to compute
/// relative paths for target files). The `skip` predicate receives the path relative
/// to `root_path` and returns true to skip that entry.
///
/// During extraction:
/// - Text files (`.md`, `.yml`, `.yaml`, `.sh`) are filtered through `filter_file()`
///   to strip non-matching agent tag sections.
/// - `context.md` is renamed to `coding_agent.context_file` (e.g., `AGENTS.md`).
/// - All other files (images, binary) are copied verbatim.
fn extract_dir_recursive_from_disk(
    source_dir: &Path,
    base_target: &Path,
    root_path: &Path,
    coding_agent: &CodingAgentDef,
    skip: &dyn Fn(&Path) -> bool,
) -> Result<()> {
    for entry in fs::read_dir(source_dir)
        .with_context(|| format!("Failed to read directory {}", source_dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root_path).unwrap_or(&path);

        if skip(rel) {
            continue;
        }

        if path.is_dir() {
            extract_dir_recursive_from_disk(&path, base_target, root_path, coding_agent, skip)?;
            continue;
        }

        let filename = rel
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        // Determine output path: rename context.md → coding_agent.context_file
        let target_path = if filename == "context.md" {
            let parent = rel.parent().unwrap_or(Path::new(""));
            base_target.join(parent).join(&coding_agent.context_file)
        } else {
            base_target.join(rel)
        };

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("Failed to create directory {}", parent.display())
            })?;
        }

        // Filter text files through agent tag pipeline; copy others verbatim
        if should_filter(&filename) {
            let content = fs::read_to_string(&path)
                .with_context(|| format!("File {} is not valid UTF-8", rel.display()))?;
            let filtered = agent_tags::filter_file(&content, &filename, &coding_agent.name);
            fs::write(&target_path, filtered.as_bytes()).with_context(|| {
                format!("Failed to write {}", target_path.display())
            })?;
        } else {
            fs::copy(&path, &target_path).with_context(|| {
                format!("Failed to copy {} to {}", path.display(), target_path.display())
            })?;
        }
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::test_support::*;

    #[test]
    fn extract_profile_copies_team_content() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &claude_code_agent()).unwrap();

        assert!(output.path().join("PROCESS.md").exists());
        assert!(output.path().join("AGENTS.md").exists());
        assert!(!output.path().join("context.md").exists());
        assert!(output.path().join("botminter.yml").exists());
        assert!(output.path().join("knowledge").is_dir());
        assert!(output.path().join("invariants").is_dir());
        assert!(output.path().join("coding-agent").is_dir());
        assert!(!output.path().join("roles").exists());
        assert!(!output.path().join(".schema").exists());
    }

    #[test]
    fn extract_member_copies_skeleton() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &claude_code_agent()).unwrap();

        assert!(output.path().join(".botminter.yml").exists());
        assert!(output.path().join("PROMPT.md").exists());
        assert!(output.path().join("AGENTS.md").exists());
        assert!(!output.path().join("context.md").exists());
        assert!(output.path().join("ralph.yml").exists());
    }

    #[test]
    fn extract_member_invalid_role_errors() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        let profiles = crate::profile::list_profiles_from(&base).unwrap();
        let profile_name = &profiles[0];
        let result =
            extract_member_from(&base, profile_name, "nonexistent", output.path(), &claude_code_agent());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(
            err.contains("nonexistent"),
            "Error should mention the invalid role name: {}",
            err
        );
        let manifest = crate::profile::read_manifest_from(profile_name, &base).unwrap();
        for role in &manifest.roles {
            assert!(
                err.contains(&role.name),
                "Error should list available role '{}': {}",
                role.name,
                err
            );
        }
    }

    #[test]
    fn extract_profile_includes_skills_and_formations() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &claude_code_agent()).unwrap();

        assert!(output.path().join("skills").is_dir());
        assert!(output.path().join("formations").is_dir());
        assert!(output.path().join("skills/knowledge-manager/SKILL.md").exists());
        assert!(output.path().join("formations/local/formation.yml").exists());
        assert!(output.path().join("formations/k8s/formation.yml").exists());
        assert!(output.path().join("formations/k8s/ralph.yml").exists());
        assert!(output.path().join("formations/k8s/PROMPT.md").exists());
    }

    #[test]
    fn extract_profile_scrum_compact_includes_expected_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum-compact", output.path(), &claude_code_agent()).unwrap();

        assert!(output.path().join("skills").is_dir());
        assert!(output.path().join("formations").is_dir());
        assert!(output.path().join("skills/knowledge-manager/SKILL.md").exists());
        assert!(output.path().join("formations/local/formation.yml").exists());
    }

    #[test]
    fn extract_profile_claude_md_is_filtered() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &claude_code_agent()).unwrap();

        let content = std::fs::read_to_string(output.path().join("AGENTS.md")).unwrap();
        assert!(!content.contains("+agent:"), "Extracted AGENTS.md should not contain +agent: tags");
        assert!(!content.contains("<!-- -agent -->"), "Extracted AGENTS.md should not contain -agent close tags");
        assert!(content.len() > 50, "Extracted AGENTS.md should have substantial content");
    }

    #[test]
    fn extract_profile_ralph_yml_is_filtered() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &claude_code_agent()).unwrap();

        let formations_dir = output.path().join("formations");
        if formations_dir.is_dir() {
            for entry in std::fs::read_dir(&formations_dir).unwrap() {
                let entry = entry.unwrap();
                if entry.file_type().unwrap().is_dir() {
                    let ralph_yml = entry.path().join("ralph.yml");
                    if ralph_yml.exists() {
                        let content = std::fs::read_to_string(&ralph_yml).unwrap();
                        assert!(!content.contains("+agent:"), "Formation ralph.yml should not contain +agent: tags");
                        let parsed: Result<serde_yml::Value, _> = serde_yml::from_str(&content);
                        assert!(parsed.is_ok(), "Formation ralph.yml should be valid YAML after filtering: {}", parsed.unwrap_err());
                    }
                }
            }
        }
    }

    #[test]
    fn extract_member_claude_md_is_filtered() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &claude_code_agent()).unwrap();

        let content = std::fs::read_to_string(output.path().join("AGENTS.md")).unwrap();
        assert!(!content.contains("+agent:"), "Extracted member AGENTS.md should not contain +agent: tags");
        assert!(!content.contains("<!-- -agent -->"), "Extracted member AGENTS.md should not contain -agent close tags");
    }

    #[test]
    fn extract_member_ralph_yml_is_filtered() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &claude_code_agent()).unwrap();

        let content = std::fs::read_to_string(output.path().join("ralph.yml")).unwrap();
        assert!(!content.contains("+agent:"), "Extracted member ralph.yml should not contain +agent: tags");
        assert!(!content.contains("# -agent"), "Extracted member ralph.yml should not contain -agent close tags");
        let parsed: Result<serde_yml::Value, _> = serde_yml::from_str(&content);
        assert!(parsed.is_ok(), "Extracted ralph.yml should be valid YAML: {}", parsed.unwrap_err());
        let yaml = parsed.unwrap();
        let backend = yaml.get("cli").and_then(|c: &serde_yml::Value| c.get("backend")).and_then(|b: &serde_yml::Value| b.as_str());
        assert_eq!(backend, Some("claude"), "Extracted ralph.yml should have cli.backend: claude");
    }

    #[test]
    fn extract_profile_mock_agent_produces_different_context_file() {
        let mock_agent = CodingAgentDef {
            name: "gemini-cli".into(),
            display_name: "Gemini CLI".into(),
            context_file: "GEMINI.md".into(),
            agent_dir: ".gemini".into(),
            binary: "gemini".into(),
        };
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &mock_agent).unwrap();

        assert!(output.path().join("GEMINI.md").exists(), "Mock agent should produce GEMINI.md");
        assert!(!output.path().join("AGENTS.md").exists(), "Mock agent should not produce AGENTS.md");
        assert!(!output.path().join("context.md").exists(), "Mock agent should not produce context.md");

        let content = std::fs::read_to_string(output.path().join("GEMINI.md")).unwrap();
        assert!(!content.contains("+agent:"), "GEMINI.md should not contain agent tags");
    }

    #[test]
    fn extract_member_mock_agent_produces_different_context_file() {
        let mock_agent = CodingAgentDef {
            name: "gemini-cli".into(),
            display_name: "Gemini CLI".into(),
            context_file: "GEMINI.md".into(),
            agent_dir: ".gemini".into(),
            binary: "gemini".into(),
        };
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &mock_agent).unwrap();

        assert!(output.path().join("GEMINI.md").exists(), "Mock agent should produce GEMINI.md in member dir");
        assert!(!output.path().join("AGENTS.md").exists(), "Mock agent should not produce AGENTS.md in member dir");
        assert!(!output.path().join("context.md").exists(), "Mock agent should not produce context.md in member dir");
    }
}
