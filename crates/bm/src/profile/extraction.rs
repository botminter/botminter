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
/// renamed to `coding_agent.context_file` (e.g., `CLAUDE.md` for Claude Code).
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
/// - `context.md` is renamed to `coding_agent.context_file` (e.g., `CLAUDE.md`).
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
            // Replace context.md references in content with the coding agent's
            // context_file (e.g., CLAUDE.md). Profile templates use the agent-neutral
            // name "context.md"; generated team repos need the resolved name.
            let filtered = if coding_agent.context_file != "context.md" {
                filtered.replace("context.md", &coding_agent.context_file)
            } else {
                filtered
            };
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
        assert!(output.path().join("CLAUDE.md").exists());
        assert!(!output.path().join("context.md").exists());
        assert!(output.path().join("botminter.yml").exists());
        assert!(output.path().join("knowledge").is_dir());
        assert!(output.path().join("invariants").is_dir());
        assert!(output.path().join("agreements").is_dir());
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
        assert!(output.path().join("CLAUDE.md").exists());
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
        extract_profile_from(&base, "agentic-sdlc-minimal", output.path(), &claude_code_agent()).unwrap();

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

        let content = std::fs::read_to_string(output.path().join("CLAUDE.md")).unwrap();
        assert!(!content.contains("+agent:"), "Extracted CLAUDE.md should not contain +agent: tags");
        assert!(!content.contains("<!-- -agent -->"), "Extracted CLAUDE.md should not contain -agent close tags");
        assert!(content.len() > 50, "Extracted CLAUDE.md should have substantial content");
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

        let content = std::fs::read_to_string(output.path().join("CLAUDE.md")).unwrap();
        assert!(!content.contains("+agent:"), "Extracted member CLAUDE.md should not contain +agent: tags");
        assert!(!content.contains("<!-- -agent -->"), "Extracted member CLAUDE.md should not contain -agent close tags");
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
    fn extract_profile_copies_agreements_directory() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let output = tempfile::tempdir().unwrap();
            extract_profile_from(&base, &profile, output.path(), &claude_code_agent()).unwrap();

            assert!(output.path().join("agreements").is_dir(), "{profile}: agreements/ should exist");
            assert!(output.path().join("agreements/decisions").is_dir(), "{profile}: agreements/decisions/ should exist");
            assert!(output.path().join("agreements/retros").is_dir(), "{profile}: agreements/retros/ should exist");
            assert!(output.path().join("agreements/norms").is_dir(), "{profile}: agreements/norms/ should exist");
        }
    }

    #[test]
    fn extract_profile_agreements_has_gitkeep() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let output = tempfile::tempdir().unwrap();
            extract_profile_from(&base, &profile, output.path(), &claude_code_agent()).unwrap();

            assert!(output.path().join("agreements/decisions/.gitkeep").exists(), "{profile}: decisions/.gitkeep should exist");
            assert!(output.path().join("agreements/retros/.gitkeep").exists(), "{profile}: retros/.gitkeep should exist");
            assert!(output.path().join("agreements/norms/.gitkeep").exists(), "{profile}: norms/.gitkeep should exist");
        }
    }

    #[test]
    fn extract_profile_includes_agreements_knowledge() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let output = tempfile::tempdir().unwrap();
            extract_profile_from(&base, &profile, output.path(), &claude_code_agent()).unwrap();

            let knowledge_path = output.path().join("knowledge/team-agreements.md");
            assert!(knowledge_path.exists(), "{profile}: knowledge/team-agreements.md should exist");

            let content = std::fs::read_to_string(&knowledge_path).unwrap();
            assert!(content.contains("decisions/"), "{profile}: should document decisions/ subdir");
            assert!(content.contains("retros/"), "{profile}: should document retros/ subdir");
            assert!(content.contains("norms/"), "{profile}: should document norms/ subdir");
        }
    }

    #[test]
    fn knowledge_team_agreements_documents_format() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let output = tempfile::tempdir().unwrap();
            extract_profile_from(&base, &profile, output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(output.path().join("knowledge/team-agreements.md")).unwrap();
            // Verify frontmatter fields are documented
            assert!(content.contains("id"), "{profile}: should document id field");
            assert!(content.contains("type"), "{profile}: should document type field");
            assert!(content.contains("status"), "{profile}: should document status field");
            assert!(content.contains("date"), "{profile}: should document date field");
            assert!(content.contains("participants"), "{profile}: should document participants field");
            // Verify lifecycle states
            assert!(content.contains("proposed"), "{profile}: should document proposed status");
            assert!(content.contains("accepted"), "{profile}: should document accepted status");
            assert!(content.contains("superseded"), "{profile}: should document superseded status");
        }
    }

    #[test]
    fn process_md_references_agreements() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let output = tempfile::tempdir().unwrap();
            extract_profile_from(&base, &profile, output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(output.path().join("PROCESS.md")).unwrap();
            let lower = content.to_lowercase();
            assert!(lower.contains("agreements"), "{profile}: PROCESS.md should reference agreements");
            assert!(lower.contains("team agreements"), "{profile}: PROCESS.md should reference team agreements convention");
        }
    }

    #[test]
    fn extract_profile_mock_agent_produces_different_context_file() {
        let mock_agent = CodingAgentDef {
            name: "gemini-cli".into(),
            display_name: "Gemini CLI".into(),
            context_file: "GEMINI.md".into(),
            agent_dir: ".gemini".into(),
            binary: "gemini".into(),
            system_prompt_flag: None,
            skip_permissions_flag: None,
        };
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &mock_agent).unwrap();

        assert!(output.path().join("GEMINI.md").exists(), "Mock agent should produce GEMINI.md");
        assert!(!output.path().join("CLAUDE.md").exists(), "Mock agent should not produce CLAUDE.md");
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
            system_prompt_flag: None,
            skip_permissions_flag: None,
        };
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &mock_agent).unwrap();

        assert!(output.path().join("GEMINI.md").exists(), "Mock agent should produce GEMINI.md in member dir");
        assert!(!output.path().join("CLAUDE.md").exists(), "Mock agent should not produce CLAUDE.md in member dir");
        assert!(!output.path().join("context.md").exists(), "Mock agent should not produce context.md in member dir");
    }

    #[test]
    fn extract_profile_replaces_context_md_references_in_content() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &claude_code_agent()).unwrap();

        let content = std::fs::read_to_string(output.path().join("CLAUDE.md")).unwrap();
        assert!(
            !content.contains("team/context.md"),
            "Extracted CLAUDE.md should not contain stale 'team/context.md' references"
        );
    }

    #[test]
    fn extract_member_replaces_context_md_references_in_content() {
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_member_from(&base, "scrum", "architect", output.path(), &claude_code_agent()).unwrap();

        let content = std::fs::read_to_string(output.path().join("CLAUDE.md")).unwrap();
        assert!(
            !content.contains("team/context.md"),
            "Extracted member CLAUDE.md should not contain stale 'team/context.md' references"
        );
    }

    #[test]
    fn extract_profile_mock_agent_replaces_context_md_with_agent_context_file() {
        let mock_agent = CodingAgentDef {
            name: "gemini-cli".into(),
            display_name: "Gemini CLI".into(),
            context_file: "GEMINI.md".into(),
            agent_dir: ".gemini".into(),
            binary: "gemini".into(),
            system_prompt_flag: None,
            skip_permissions_flag: None,
        };
        let (_profiles_tmp, base) = setup_disk_profiles();
        let output = tempfile::tempdir().unwrap();
        extract_profile_from(&base, "scrum", output.path(), &mock_agent).unwrap();

        let content = std::fs::read_to_string(output.path().join("GEMINI.md")).unwrap();
        assert!(
            !content.contains("context.md"),
            "Extracted GEMINI.md should not contain any 'context.md' references, got: {}",
            content.lines().filter(|l| l.contains("context.md")).collect::<Vec<_>>().join("\n")
        );
    }

    // --- Retrospective Skill Tests (Task 02) ---

    #[test]
    fn retrospective_skill_exists_in_all_profiles() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let skill_path = output.path().join("coding-agent/skills/retrospective/SKILL.md");
            assert!(skill_path.exists(), "{profile}: retrospective/SKILL.md should exist after member extraction");
        }
    }

    #[test]
    fn retrospective_skill_covered_by_ralph_yml_skill_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let ralph_yml_path = output.path().join("ralph.yml");
            let content = std::fs::read_to_string(&ralph_yml_path)
                .unwrap_or_else(|_| panic!("{profile}: chief-of-staff ralph.yml should exist"));
            let yaml: serde_yml::Value = serde_yml::from_str(&content).unwrap();
            let dirs = yaml.get("skills")
                .and_then(|s| s.get("dirs"))
                .and_then(|d| d.as_sequence())
                .unwrap_or_else(|| panic!("{profile}: skills.dirs should be a sequence"));

            let has_skill_dir = dirs.iter().any(|d| {
                d.as_str().map_or(false, |s| {
                    s.contains("chief-of-staff/coding-agent/skills")
                        || s.contains("{{member_dir}}/coding-agent/skills")
                })
            });
            assert!(has_skill_dir, "{profile}: ralph.yml skills.dirs should cover chief-of-staff skills");
        }
    }

    #[test]
    fn retrospective_skill_has_valid_frontmatter() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/retrospective/SKILL.md")
            ).unwrap();

            // Extract frontmatter between --- delimiters
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            assert!(parts.len() >= 3, "{profile}: SKILL.md should have YAML frontmatter");
            let frontmatter = parts[1];

            let yaml: serde_yml::Value = serde_yml::from_str(frontmatter).unwrap();

            let name = yaml.get("name").and_then(|n| n.as_str());
            assert_eq!(name, Some("retrospective"), "{profile}: name should be 'retrospective'");

            let desc = yaml.get("description").and_then(|d| d.as_str()).unwrap_or("");
            assert!(!desc.is_empty(), "{profile}: description should be non-empty");
            assert!(desc.len() < 1024, "{profile}: description should be under 1024 chars");
            assert!(!desc.contains('<') && !desc.contains('>'), "{profile}: no XML angle brackets");

            let desc_lower = desc.to_lowercase();
            assert!(
                desc_lower.contains("retro") || desc_lower.contains("retrospective"),
                "{profile}: description should contain retro trigger phrase"
            );
            assert!(
                desc_lower.contains("use when") || desc_lower.contains("use for"),
                "{profile}: description should contain trigger phrasing"
            );
        }
    }

    #[test]
    fn retrospective_skill_body_under_word_limit() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/retrospective/SKILL.md")
            ).unwrap();

            // Strip frontmatter
            let parts: Vec<&str> = content.splitn(3, "---").collect();
            let body = if parts.len() >= 3 { parts[2] } else { &content };
            let word_count = body.split_whitespace().count();
            assert!(word_count < 5000, "{profile}: SKILL.md body has {word_count} words, must be under 5000");
        }
    }

    #[test]
    fn retrospective_skill_covers_retro_sections() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/retrospective/SKILL.md")
            ).unwrap();
            let lower = content.to_lowercase();

            assert!(lower.contains("scope"), "{profile}: should cover retro scope");
            assert!(lower.contains("went well"), "{profile}: should cover what went well");
            assert!(
                lower.contains("didn\u{2019}t go well") || lower.contains("didn't go well") || lower.contains("pain point") || lower.contains("improvement"),
                "{profile}: should cover what didn't go well"
            );
            assert!(lower.contains("action item"), "{profile}: should cover action items");
        }
    }

    #[test]
    fn retrospective_skill_documents_action_item_types() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/retrospective/SKILL.md")
            ).unwrap();

            assert!(content.contains("process-change"), "{profile}: should document process-change type");
            assert!(content.contains("role-change"), "{profile}: should document role-change type");
            assert!(content.contains("member-tuning"), "{profile}: should document member-tuning type");
            assert!(content.contains("knowledge-update"), "{profile}: should document knowledge-update type");
            assert!(content.contains("norm"), "{profile}: should document norm type");
        }
    }

    #[test]
    fn retrospective_skill_references_agreements_output() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/retrospective/SKILL.md")
            ).unwrap();

            assert!(content.contains("agreements/retros/"), "{profile}: should reference agreements/retros/ output path");
            assert!(content.contains("agreements/norms/"), "{profile}: should reference agreements/norms/ path");
        }
    }

    #[test]
    fn retrospective_skill_no_readme() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let readme_path = output.path().join("coding-agent/skills/retrospective/README.md");
            assert!(!readme_path.exists(), "{profile}: retrospective/ should NOT have a README.md");
        }
    }

    // --- Role Management Skill Tests (Task 03) ---

    #[test]
    fn role_management_skill_exists_in_all_profiles() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let skill_path = output.path().join("coding-agent/skills/role-management/SKILL.md");
            assert!(skill_path.exists(), "{profile}: role-management/SKILL.md should exist after member extraction");
        }
    }

    #[test]
    fn role_management_skill_covered_by_ralph_yml_skill_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let ralph_yml_path = output.path().join("ralph.yml");
            let content = std::fs::read_to_string(&ralph_yml_path)
                .unwrap_or_else(|_| panic!("{profile}: chief-of-staff ralph.yml should exist"));
            let yaml: serde_yml::Value = serde_yml::from_str(&content).unwrap();
            let dirs = yaml.get("skills")
                .and_then(|s| s.get("dirs"))
                .and_then(|d| d.as_sequence())
                .unwrap_or_else(|| panic!("{profile}: skills.dirs should be a sequence"));

            let has_skill_dir = dirs.iter().any(|d| {
                d.as_str().map_or(false, |s| {
                    s.contains("chief-of-staff/coding-agent/skills")
                        || s.contains("{{member_dir}}/coding-agent/skills")
                })
            });
            assert!(has_skill_dir, "{profile}: ralph.yml skills.dirs should cover chief-of-staff skills");
        }
    }

    #[test]
    fn role_management_skill_has_valid_frontmatter() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/role-management/SKILL.md")
            ).unwrap();

            let parts: Vec<&str> = content.splitn(3, "---").collect();
            assert!(parts.len() >= 3, "{profile}: SKILL.md should have YAML frontmatter");
            let frontmatter = parts[1];

            let yaml: serde_yml::Value = serde_yml::from_str(frontmatter).unwrap();

            let name = yaml.get("name").and_then(|n| n.as_str());
            assert_eq!(name, Some("role-management"), "{profile}: name should be 'role-management'");

            let desc = yaml.get("description").and_then(|d| d.as_str()).unwrap_or("");
            assert!(!desc.is_empty(), "{profile}: description should be non-empty");
            assert!(desc.len() < 1024, "{profile}: description should be under 1024 chars");
            assert!(!desc.contains('<') && !desc.contains('>'), "{profile}: no XML angle brackets");

            let desc_lower = desc.to_lowercase();
            assert!(
                desc_lower.contains("role"),
                "{profile}: description should mention 'role'"
            );
            assert!(
                desc_lower.contains("use when") || desc_lower.contains("use for"),
                "{profile}: description should contain trigger phrasing"
            );
        }
    }

    #[test]
    fn role_management_skill_body_under_word_limit() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/role-management/SKILL.md")
            ).unwrap();

            let parts: Vec<&str> = content.splitn(3, "---").collect();
            let body = if parts.len() >= 3 { parts[2] } else { &content };
            let word_count = body.split_whitespace().count();
            assert!(word_count < 5000, "{profile}: SKILL.md body has {word_count} words, must be under 5000");
        }
    }

    #[test]
    fn role_management_skill_covers_four_operations() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/role-management/SKILL.md")
            ).unwrap();
            let lower = content.to_lowercase();

            assert!(lower.contains("list role"), "{profile}: should cover list roles operation");
            assert!(lower.contains("add role"), "{profile}: should cover add role operation");
            assert!(lower.contains("remove role"), "{profile}: should cover remove role operation");
            assert!(lower.contains("inspect role"), "{profile}: should cover inspect role operation");
        }
    }

    #[test]
    fn role_management_skill_includes_impact_analysis() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/role-management/SKILL.md")
            ).unwrap();
            let lower = content.to_lowercase();

            assert!(lower.contains("impact"), "{profile}: should cover impact analysis");
            assert!(lower.contains("status"), "{profile}: impact analysis should mention statuses");
            assert!(lower.contains("hat"), "{profile}: impact analysis should mention hats");
            assert!(lower.contains("knowledge"), "{profile}: impact analysis should mention knowledge");
        }
    }

    #[test]
    fn role_management_skill_references_agreements() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/role-management/SKILL.md")
            ).unwrap();

            assert!(content.contains("agreements/decisions/"), "{profile}: should reference agreements/decisions/ path");
        }
    }

    #[test]
    fn role_management_skill_no_readme() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let readme_path = output.path().join("coding-agent/skills/role-management/README.md");
            assert!(!readme_path.exists(), "{profile}: role-management/ should NOT have a README.md");
        }
    }

    // ── Member Tuning Skill Tests ──────────────────────────────────────

    #[test]
    fn member_tuning_skill_exists_in_all_profiles() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let skill_path = output.path().join("coding-agent/skills/member-tuning/SKILL.md");
            assert!(skill_path.exists(), "{profile}: member-tuning/SKILL.md should exist after member extraction");
        }
    }

    #[test]
    fn member_tuning_skill_covered_by_ralph_yml_skill_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let ralph_yml_path = output.path().join("ralph.yml");
            let content = std::fs::read_to_string(&ralph_yml_path)
                .unwrap_or_else(|_| panic!("{profile}: chief-of-staff ralph.yml should exist"));
            let yaml: serde_yml::Value = serde_yml::from_str(&content).unwrap();
            let dirs = yaml.get("skills")
                .and_then(|s| s.get("dirs"))
                .and_then(|d| d.as_sequence())
                .unwrap_or_else(|| panic!("{profile}: skills.dirs should be a sequence"));

            let has_skill_dir = dirs.iter().any(|d| {
                d.as_str().map_or(false, |s| {
                    s.contains("chief-of-staff/coding-agent/skills")
                        || s.contains("{{member_dir}}/coding-agent/skills")
                })
            });
            assert!(has_skill_dir, "{profile}: ralph.yml skills.dirs should cover chief-of-staff skills");
        }
    }

    #[test]
    fn member_tuning_skill_has_valid_frontmatter() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/member-tuning/SKILL.md")
            ).unwrap();

            let parts: Vec<&str> = content.splitn(3, "---").collect();
            assert!(parts.len() >= 3, "{profile}: SKILL.md should have YAML frontmatter");
            let frontmatter = parts[1];

            let yaml: serde_yml::Value = serde_yml::from_str(frontmatter).unwrap();

            let name = yaml.get("name").and_then(|n| n.as_str());
            assert_eq!(name, Some("member-tuning"), "{profile}: name should be 'member-tuning'");

            let desc = yaml.get("description").and_then(|d| d.as_str()).unwrap_or("");
            assert!(!desc.is_empty(), "{profile}: description should be non-empty");
            assert!(desc.len() < 1024, "{profile}: description should be under 1024 chars");
            assert!(!desc.contains('<') && !desc.contains('>'), "{profile}: no XML angle brackets");

            let desc_lower = desc.to_lowercase();
            assert!(
                desc_lower.contains("tun") || desc_lower.contains("troubleshoot") || desc_lower.contains("diagnostic"),
                "{profile}: description should mention tuning, troubleshoot, or diagnostic"
            );
            assert!(
                desc_lower.contains("use when"),
                "{profile}: description should contain 'use when' trigger phrasing"
            );
        }
    }

    #[test]
    fn member_tuning_skill_body_under_word_limit() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/member-tuning/SKILL.md")
            ).unwrap();

            let parts: Vec<&str> = content.splitn(3, "---").collect();
            let body = if parts.len() >= 3 { parts[2] } else { &content };
            let word_count = body.split_whitespace().count();
            assert!(word_count < 5000, "{profile}: SKILL.md body has {word_count} words, must be under 5000");
        }
    }

    #[test]
    fn member_tuning_skill_covers_five_artifact_types() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/member-tuning/SKILL.md")
            ).unwrap();

            assert!(content.contains("PROMPT.md"), "{profile}: should cover PROMPT.md artifact");
            assert!(content.contains("CLAUDE.md"), "{profile}: should cover CLAUDE.md artifact");
            assert!(content.contains("ralph.yml") || content.contains("hats"), "{profile}: should cover hats/ralph.yml artifact");
            assert!(content.contains("skills"), "{profile}: should cover skills artifact");
            assert!(content.contains("PROCESS.md"), "{profile}: should cover PROCESS.md artifact");
        }
    }

    #[test]
    fn member_tuning_skill_includes_diagnostic_flow() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/member-tuning/SKILL.md")
            ).unwrap();
            let lower = content.to_lowercase();

            assert!(
                lower.contains("symptom") || lower.contains("diagnos"),
                "{profile}: should contain diagnostic/symptom language"
            );
            assert!(
                lower.contains("inspect"),
                "{profile}: should contain inspection instructions"
            );
        }
    }

    #[test]
    fn member_tuning_skill_includes_propagation_reminder() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/member-tuning/SKILL.md")
            ).unwrap();
            let lower = content.to_lowercase();

            assert!(lower.contains("sync"), "{profile}: should mention sync for propagation");
            assert!(
                lower.contains("restart") || lower.contains("bm stop") || lower.contains("bm start"),
                "{profile}: should mention restart for propagation"
            );
        }
    }

    #[test]
    fn member_tuning_skill_no_readme() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let readme_path = output.path().join("coding-agent/skills/member-tuning/README.md");
            assert!(!readme_path.exists(), "{profile}: member-tuning/ should NOT have a README.md");
        }
    }

    // ── Process Evolution Skill Tests ──────────────────────────────────

    #[test]
    fn process_evolution_skill_exists_in_all_profiles() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let skill_path = output.path().join("coding-agent/skills/process-evolution/SKILL.md");
            assert!(skill_path.exists(), "{profile}: process-evolution/SKILL.md should exist after member extraction");
        }
    }

    #[test]
    fn process_evolution_skill_covered_by_ralph_yml_skill_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let ralph_yml = std::fs::read_to_string(output.path().join("ralph.yml")).unwrap();
            let has_skill_dir = ralph_yml.lines().any(|line| {
                let trimmed = line.trim().trim_start_matches("- ");
                trimmed.contains("skills") && (trimmed.contains("chief-of-staff") || trimmed.contains("coding-agent/skills"))
            });
            assert!(has_skill_dir, "{profile}: ralph.yml skills.dirs should cover chief-of-staff skills");
        }
    }

    #[test]
    fn process_evolution_skill_has_valid_frontmatter() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/process-evolution/SKILL.md"),
            ).unwrap();

            assert!(content.starts_with("---"), "{profile}: SKILL.md must start with YAML frontmatter");
            assert!(content.contains("name: process-evolution"), "{profile}: frontmatter must have name: process-evolution");

            let desc_area = &content[..content.find("\n---\n").unwrap_or(content.len())];
            let desc_lower = desc_area.to_lowercase();
            assert!(
                desc_lower.contains("process") || desc_lower.contains("workflow") || desc_lower.contains("status"),
                "{profile}: description should mention process, workflow, or status triggers"
            );
        }
    }

    #[test]
    fn process_evolution_skill_body_under_word_limit() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/process-evolution/SKILL.md"),
            ).unwrap();
            let body = content.splitn(3, "---").nth(2).unwrap_or("");
            let word_count = body.split_whitespace().count();
            assert!(word_count < 5000, "{profile}: SKILL.md body has {word_count} words, must be under 5000");
        }
    }

    #[test]
    fn process_evolution_skill_covers_status_graph_operations() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/process-evolution/SKILL.md"),
            ).unwrap();
            let lower = content.to_lowercase();
            assert!(lower.contains("show") && lower.contains("current process"), "{profile}: should cover showing current process");
            assert!(lower.contains("adding a status"), "{profile}: should cover adding a status");
            assert!(lower.contains("removing a status"), "{profile}: should cover removing a status");
        }
    }

    #[test]
    fn process_evolution_skill_includes_validation_rules() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/process-evolution/SKILL.md"),
            ).unwrap();
            let lower = content.to_lowercase();
            assert!(lower.contains("orphan"), "{profile}: should include orphan status validation");
            assert!(lower.contains("dead") || lower.contains("dead-end") || lower.contains("dead end"), "{profile}: should include dead-end validation");
            assert!(lower.contains("loop"), "{profile}: should include loop validation");
        }
    }

    #[test]
    fn process_evolution_skill_references_agreements() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/process-evolution/SKILL.md"),
            ).unwrap();
            assert!(content.contains("agreements/decisions"), "{profile}: should reference agreements/decisions for recording changes");
        }
    }

    #[test]
    fn process_evolution_skill_no_readme() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let readme_path = output.path().join("coding-agent/skills/process-evolution/README.md");
            assert!(!readme_path.exists(), "{profile}: process-evolution/ should NOT have a README.md");
        }
    }

    // ── Team Design Hub Skill ──────────────────────────────────────────

    #[test]
    fn team_design_skill_exists_in_all_profiles() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let skill_path = output.path().join("coding-agent/skills/team-design/SKILL.md");
            assert!(skill_path.exists(), "{profile}: team-design/SKILL.md should exist after member extraction");
        }
    }

    #[test]
    fn team_design_skill_covered_by_ralph_yml_skill_dirs() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let ralph_yml = std::fs::read_to_string(output.path().join("ralph.yml")).unwrap();
            let has_skill_dir = ralph_yml.lines().any(|line| {
                let trimmed = line.trim().trim_start_matches("- ");
                trimmed.contains("skills") && (trimmed.contains("chief-of-staff") || trimmed.contains("coding-agent/skills"))
            });
            assert!(has_skill_dir, "{profile}: ralph.yml skills.dirs should cover chief-of-staff skills");
        }
    }

    #[test]
    fn team_design_skill_has_valid_frontmatter() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/team-design/SKILL.md"),
            ).unwrap();

            assert!(content.starts_with("---"), "{profile}: SKILL.md must start with YAML frontmatter");
            assert!(content.contains("name: team-design"), "{profile}: frontmatter must have name: team-design");

            let desc_area = &content[..content.find("\n---\n").unwrap_or(content.len())];
            let desc_lower = desc_area.to_lowercase();
            assert!(
                desc_lower.contains("use when"),
                "{profile}: description must contain trigger phrase 'Use when'"
            );
        }
    }

    #[test]
    fn team_design_skill_no_readme() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let readme_path = output.path().join("coding-agent/skills/team-design/README.md");
            assert!(!readme_path.exists(), "{profile}: team-design/ should NOT have a README.md");
        }
    }

    #[test]
    fn team_design_skill_body_under_word_limit() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/team-design/SKILL.md"),
            ).unwrap();
            let body = content.splitn(3, "---").nth(2).unwrap_or("");
            let word_count = body.split_whitespace().count();
            assert!(word_count < 5000, "{profile}: SKILL.md body has {word_count} words, must be under 5000");
        }
    }

    #[test]
    fn team_design_skill_includes_intent_routing_table() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/team-design/SKILL.md"),
            ).unwrap();
            let lower = content.to_lowercase();
            assert!(lower.contains("retrospective"), "{profile}: routing table should reference retrospective");
            assert!(lower.contains("role-management"), "{profile}: routing table should reference role-management");
            assert!(lower.contains("member-tuning"), "{profile}: routing table should reference member-tuning");
            assert!(lower.contains("process-evolution"), "{profile}: routing table should reference process-evolution");
        }
    }

    #[test]
    fn team_design_skill_references_skill_loading() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/team-design/SKILL.md"),
            ).unwrap();
            assert!(
                content.contains("ralph tools skill load"),
                "{profile}: should reference 'ralph tools skill load' for delegation to sub-skills"
            );
        }
    }

    #[test]
    fn team_design_skill_includes_dashboard_procedure() {
        let (_profiles_tmp, base) = setup_disk_profiles();

        for profile in crate::profile::list_profiles_from(&base).unwrap() {
            let roles = crate::profile::list_roles_from(&profile, &base).unwrap();
            if !roles.contains(&"chief-of-staff".to_string()) {
                continue;
            }
            let output = tempfile::tempdir().unwrap();
            extract_member_from(&base, &profile, "chief-of-staff", output.path(), &claude_code_agent()).unwrap();

            let content = std::fs::read_to_string(
                output.path().join("coding-agent/skills/team-design/SKILL.md"),
            ).unwrap();
            let lower = content.to_lowercase();
            assert!(lower.contains("dashboard"), "{profile}: should include dashboard section");
            assert!(lower.contains("roles") && lower.contains("members"), "{profile}: dashboard should cover roles and members");
            assert!(lower.contains("agreements") || lower.contains("action items"), "{profile}: dashboard should cover agreements or action items");
        }
    }

    // --- Minty Profile Design Skill Tests (Task 07) ---

    fn minty_skill_path() -> std::path::PathBuf {
        let manifest_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"));
        manifest_dir.join("../../minty/.claude/skills/profile-design/SKILL.md")
    }

    #[test]
    fn minty_profile_design_skill_exists() {
        let path = minty_skill_path();
        assert!(path.exists(), "minty/.claude/skills/profile-design/SKILL.md should exist in source tree");
    }

    #[test]
    fn minty_profile_design_skill_has_valid_frontmatter() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();
        assert!(content.starts_with("---"), "SKILL.md should start with YAML frontmatter delimiter");

        let end = content[3..].find("---").expect("SKILL.md should have closing frontmatter delimiter");
        let frontmatter = &content[3..3 + end];

        assert!(frontmatter.contains("name: profile-design"), "frontmatter should have name: profile-design");
        assert!(frontmatter.contains("description:"), "frontmatter should have description field");

        // Extract description content (multi-line YAML scalar)
        let desc_start = frontmatter.find("description:").unwrap();
        let desc_section = &frontmatter[desc_start..];
        // Find next top-level key or end
        let desc_end = desc_section[12..].find("\nmetadata:")
            .or_else(|| desc_section[12..].find("\nlicense:"))
            .unwrap_or(desc_section.len() - 12);
        let description = &desc_section[12..12 + desc_end];
        // Strip YAML block scalar indicators (>-, |-, etc.) before checking content
        let desc_flat: String = description.lines()
            .map(|l| l.trim())
            .filter(|l| *l != ">-" && *l != "|-" && *l != ">" && *l != "|")
            .collect::<Vec<_>>().join(" ");

        assert!(desc_flat.len() < 1024, "description should be under 1024 characters, got {}", desc_flat.len());
        assert!(!desc_flat.contains('<') && !desc_flat.contains('>'), "description must not contain XML angle brackets");
        assert!(
            desc_flat.to_lowercase().contains("use when"),
            "description should include 'Use when' trigger phrase"
        );
    }

    #[test]
    fn minty_profile_design_skill_no_readme() {
        let readme_path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("../../minty/.claude/skills/profile-design/README.md");
        assert!(!readme_path.exists(), "No README.md should exist in the profile-design skill folder");
    }

    #[test]
    fn minty_profile_design_skill_body_under_word_limit() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();
        let end_of_frontmatter = content[3..].find("---").unwrap() + 3 + 3;
        let body = &content[end_of_frontmatter..];
        let word_count = body.split_whitespace().count();
        assert!(word_count < 5000, "SKILL.md body should be under 5000 words, got {word_count}");
    }

    #[test]
    fn minty_profile_design_skill_covers_six_operations() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();
        let lower = content.to_lowercase();

        assert!(lower.contains("browse"), "should cover browse operation");
        assert!(lower.contains("role") && lower.contains("design"), "should cover role design operation");
        assert!(lower.contains("process") && lower.contains("design"), "should cover process design operation");
        assert!(lower.contains("hat") && lower.contains("design"), "should cover hat design operation");
        assert!(lower.contains("fork"), "should cover fork operation");
        assert!(lower.contains("troubleshoot"), "should cover troubleshoot operation");
    }

    #[test]
    fn minty_profile_design_skill_includes_validation_checklist() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();
        let lower = content.to_lowercase();

        assert!(lower.contains("validation"), "should include validation section");
        assert!(lower.contains("statuses") || lower.contains("status"), "validation should check statuses");
        assert!(lower.contains("hat trigger") || lower.contains("triggers"), "validation should check hat triggers");
        assert!(lower.contains("skeleton") || lower.contains("prompt.md"), "validation should check skeleton files");
    }

    #[test]
    fn minty_profile_design_skill_includes_troubleshooting() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();
        let lower = content.to_lowercase();

        assert!(lower.contains("troubleshoot"), "should include troubleshooting section");
        assert!(lower.contains("diagnostic") || lower.contains("diagnose"), "should include diagnostic procedures");
        assert!(lower.contains("symptom") || lower.contains("common issues"), "should describe symptoms or common issues");
    }

    #[test]
    fn minty_profile_design_skill_defers_to_chief_of_staff() {
        let content = std::fs::read_to_string(minty_skill_path()).unwrap();

        assert!(
            content.contains("bm chat chief-of-staff"),
            "should reference `bm chat chief-of-staff` for live team changes"
        );
    }
}
