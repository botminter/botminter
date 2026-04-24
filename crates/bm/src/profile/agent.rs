use std::fs;
use std::path::Path;

use anyhow::{Context, Result, bail};

use super::extraction::should_filter;
use super::manifest::{CodingAgentDef, ProfileManifest};
use super::{list_profiles_from, profiles_dir};
use crate::agent_tags;

/// Scans all files in a profile on disk for agent tags, returning a sorted list of
/// (relative path, agents) pairs for files that contain at least one tag.
pub fn scan_agent_tags(profile_name: &str) -> Result<Vec<(String, Vec<String>)>> {
    scan_agent_tags_in(profile_name, &profiles_dir()?)
}

fn scan_agent_tags_in(profile_name: &str, base: &Path) -> Result<Vec<(String, Vec<String>)>> {
    let profile_dir = base.join(profile_name);
    if !profile_dir.is_dir() {
        let available = list_profiles_from(base).unwrap_or_default().join(", ");
        bail!(
            "Profile '{}' not found. Available profiles: {}",
            profile_name, available
        );
    }

    let mut results = Vec::new();
    scan_dir_for_tags_on_disk(&profile_dir, &profile_dir, &mut results)?;
    results.sort_by(|a, b| a.0.cmp(&b.0));
    Ok(results)
}

/// Recursively scans a disk directory for files containing agent tags.
fn scan_dir_for_tags_on_disk(
    dir: &Path,
    root_path: &Path,
    results: &mut Vec<(String, Vec<String>)>,
) -> Result<()> {
    for entry in fs::read_dir(dir)
        .with_context(|| format!("Failed to read directory {}", dir.display()))?
    {
        let entry = entry?;
        let path = entry.path();
        let rel = path.strip_prefix(root_path).unwrap_or(&path);

        if path.is_dir() {
            scan_dir_for_tags_on_disk(&path, root_path, results)?;
            continue;
        }

        let filename = rel
            .file_name()
            .map(|f| f.to_string_lossy().to_string())
            .unwrap_or_default();

        if !should_filter(&filename) {
            continue;
        }

        if let Ok(content) = fs::read_to_string(&path) {
            let syntax = agent_tags::detect_comment_syntax(&filename);
            let agents = agent_tags::collect_agent_names(&content, syntax);
            if !agents.is_empty() {
                results.push((
                    rel.to_string_lossy().to_string(),
                    agents.into_iter().collect(),
                ));
            }
        }
    }
    Ok(())
}

/// Resolves the effective coding agent for a team.
///
/// Resolution order:
/// 1. Team-level override (`team.coding_agent`) if set
/// 2. Profile default (`manifest.default_coding_agent`)
///
/// Returns an error if the resolved agent name is not found in the manifest's
/// `coding_agents` map.
pub fn resolve_coding_agent<'a>(
    team: &crate::config::TeamEntry,
    manifest: &'a ProfileManifest,
) -> Result<&'a CodingAgentDef> {
    let agent_name = team
        .coding_agent
        .as_deref()
        .unwrap_or(&manifest.default_coding_agent);

    if agent_name.is_empty() {
        bail!(
            "No coding agent configured. Profile '{}' does not declare a default_coding_agent \
             and team '{}' has no coding_agent override.",
            manifest.name,
            team.name
        );
    }

    manifest.coding_agents.get(agent_name).with_context(|| {
        let available: Vec<&str> = manifest.coding_agents.keys().map(|k| k.as_str()).collect();
        format!(
            "Coding agent '{}' not found in profile '{}'. Available agents: {}",
            agent_name,
            manifest.name,
            if available.is_empty() {
                "(none)".to_string()
            } else {
                available.join(", ")
            }
        )
    })
}

/// Resolves the coding agent binary from profiles available on disk.
///
/// Scans profiles in order and returns the binary name from the first one
/// that declares a default coding agent. Used by `bm minty` when no team
/// is specified.
pub fn resolve_agent_from_profiles() -> Result<CodingAgentDef> {
    let profiles = super::list_profiles()?;
    if profiles.is_empty() {
        bail!(
            "No profiles found on disk. Run `bm profiles init` to extract profiles."
        );
    }

    for name in &profiles {
        if let Ok(manifest) = super::read_manifest(name) {
            if !manifest.default_coding_agent.is_empty() {
                if let Some(agent) = manifest.coding_agents.get(&manifest.default_coding_agent) {
                    return Ok(agent.clone());
                }
            }
        }
    }

    bail!(
        "No profile defines a default coding agent. \
         Available profiles: {}",
        profiles.join(", ")
    )
}

/// Ensures Minty config is present on disk at `~/.config/botminter/minty/`.
/// Auto-extracts embedded config if the directory is missing.
pub fn ensure_minty_initialized() -> Result<std::path::PathBuf> {
    let minty_dir = super::minty_dir()?;

    if !minty_dir.join("prompt.md").exists() {
        eprintln!("Initializing Minty config...");
        fs::create_dir_all(&minty_dir).with_context(|| {
            format!(
                "Failed to create minty directory {}",
                minty_dir.display()
            )
        })?;
        super::extract_minty_to_disk(&minty_dir)?;
        eprintln!("Extracted Minty config to {}", minty_dir.display());
    }

    Ok(minty_dir)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::test_support::*;

    #[test]
    fn scan_agent_tags_finds_tagged_files() {
        let (_tmp, base) = setup_disk_profiles();
        let results = scan_agent_tags_in("scrum", &base).unwrap();
        assert!(!results.is_empty(), "scrum profile should have tagged files");
        let has_context = results.iter().any(|(path, _)| path == "context.md");
        assert!(has_context, "scrum should have tagged context.md");
    }

    #[test]
    fn scan_agent_tags_reports_claude_code() {
        let (_tmp, base) = setup_disk_profiles();
        let results = scan_agent_tags_in("scrum", &base).unwrap();
        for (path, agents) in &results {
            assert!(
                agents.contains(&"claude-code".to_string()),
                "File {} should reference claude-code agent, got {:?}", path, agents
            );
        }
    }

    #[test]
    fn scan_agent_tags_finds_ralph_yml_tags() {
        let (_tmp, base) = setup_disk_profiles();
        let results = scan_agent_tags_in("scrum", &base).unwrap();
        let has_ralph_yml = results.iter().any(|(path, _)| path.ends_with("ralph.yml"));
        assert!(has_ralph_yml, "scrum profile should have tagged ralph.yml files");
    }

    #[test]
    fn scan_agent_tags_all_profiles_consistent() {
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let results = scan_agent_tags_in(&name, &base).unwrap();
            assert!(!results.is_empty(), "Profile '{}' should have tagged files", name);
        }
    }

    #[test]
    fn scan_agent_tags_nonexistent_profile_errors() {
        let (_tmp, base) = setup_disk_profiles();
        let result = scan_agent_tags_in("nonexistent", &base);
        assert!(result.is_err());
    }

    #[test]
    fn resolve_coding_agent_uses_profile_default() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = crate::profile::read_manifest_from("scrum", &base).unwrap();
        let team = crate::config::TeamEntry {
            name: "test-team".into(),
            path: "/tmp/test".into(),
            profile: "scrum".into(),
            github_repo: "org/test-team".into(),
            credentials: Default::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };
        let agent = resolve_coding_agent(&team, &manifest).unwrap();
        assert_eq!(agent.name, "claude-code");
        assert_eq!(agent.context_file, "CLAUDE.md");
    }

    #[test]
    fn resolve_coding_agent_team_override() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = crate::profile::read_manifest_from("scrum", &base).unwrap();
        let team = crate::config::TeamEntry {
            name: "test-team".into(),
            path: "/tmp/test".into(),
            profile: "scrum".into(),
            github_repo: "org/test-team".into(),
            credentials: Default::default(),
            coding_agent: Some("claude-code".into()),
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };
        let agent = resolve_coding_agent(&team, &manifest).unwrap();
        assert_eq!(agent.name, "claude-code");
    }

    #[test]
    fn resolve_coding_agent_unknown_agent_errors() {
        let (_tmp, base) = setup_disk_profiles();
        let manifest = crate::profile::read_manifest_from("scrum", &base).unwrap();
        let team = crate::config::TeamEntry {
            name: "test-team".into(),
            path: "/tmp/test".into(),
            profile: "scrum".into(),
            github_repo: "org/test-team".into(),
            credentials: Default::default(),
            coding_agent: Some("nonexistent-agent".into()),
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        };
        let result = resolve_coding_agent(&team, &manifest);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent-agent"));
        assert!(err.contains("not found"));
    }

    // ── Agent tag validation tests (disk-based) ────────────────────

    #[test]
    fn tagged_context_md_files_have_balanced_tags() {
        use crate::agent_tags::{CommentSyntax, tags_are_balanced};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy();
                if path_str.ends_with("context.md") {
                    let content = fs::read_to_string(&file_path).unwrap();
                    assert!(
                        tags_are_balanced(&content, CommentSyntax::Html),
                        "Unbalanced HTML agent tags in {}", path_str
                    );
                }
            }
        }
    }

    #[test]
    fn tagged_ralph_yml_files_have_balanced_tags() {
        use crate::agent_tags::{CommentSyntax, tags_are_balanced};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy();
                if path_str.ends_with("ralph.yml") {
                    let content = fs::read_to_string(&file_path).unwrap();
                    assert!(
                        tags_are_balanced(&content, CommentSyntax::Hash),
                        "Unbalanced hash agent tags in {}", path_str
                    );
                }
            }
        }
    }

    #[test]
    fn filtering_context_md_for_claude_code_strips_only_tag_lines() {
        use crate::agent_tags::{CommentSyntax, filter_agent_tags};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy().to_string();
                if path_str.ends_with("context.md") {
                    let content = fs::read_to_string(&file_path).unwrap();
                    let filtered = filter_agent_tags(&content, "claude-code", CommentSyntax::Html);
                    assert!(
                        !filtered.contains("+agent:"),
                        "Filtered {} still contains +agent: tags", path_str
                    );
                    assert!(
                        !filtered.contains("<!-- -agent -->"),
                        "Filtered {} still contains -agent tags", path_str
                    );
                    for line in content.lines() {
                        if !line.trim().starts_with("<!-- +agent:")
                            && line.trim() != "<!-- -agent -->"
                        {
                            assert!(
                                filtered.contains(line),
                                "Filtered {} is missing line: {}", path_str, line
                            );
                        }
                    }
                }
            }
        }
    }

    #[test]
    fn filtering_ralph_yml_for_claude_code_produces_valid_yaml() {
        use crate::agent_tags::{CommentSyntax, filter_agent_tags};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy().to_string();
                if path_str.ends_with("ralph.yml")
                    && !path_str.contains("formations/")
                {
                    let content = fs::read_to_string(&file_path).unwrap();
                    let filtered = filter_agent_tags(&content, "claude-code", CommentSyntax::Hash);
                    let parsed: Result<serde_yml::Value, _> = serde_yml::from_str(&filtered);
                    assert!(
                        parsed.is_ok(),
                        "Filtered {} is not valid YAML: {}", path_str,
                        parsed.unwrap_err()
                    );
                    let yaml = parsed.unwrap();
                    let backend = yaml.get("cli")
                        .and_then(|c: &serde_yml::Value| c.get("backend"))
                        .and_then(|b: &serde_yml::Value| b.as_str());
                    assert_eq!(
                        backend, Some("claude"),
                        "Filtered {} should have cli.backend: claude", path_str
                    );
                }
            }
        }
    }

    #[test]
    fn filtering_context_md_for_other_agent_excludes_claude_sections() {
        use crate::agent_tags::{CommentSyntax, filter_agent_tags};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy().to_string();
                if path_str.ends_with("context.md") {
                    let content = fs::read_to_string(&file_path).unwrap();
                    if !content.contains("+agent:claude-code") {
                        continue;
                    }
                    let filtered = filter_agent_tags(&content, "gemini-cli", CommentSyntax::Html);
                    assert!(
                        !filtered.contains(".claude/"),
                        "Filtering {} for gemini-cli should exclude .claude/ references", path_str
                    );
                }
            }
        }
    }

    #[test]
    fn filtering_ralph_yml_for_other_agent_excludes_claude_backend() {
        use crate::agent_tags::{CommentSyntax, filter_agent_tags};
        let (_tmp, base) = setup_disk_profiles();
        for name in crate::profile::list_profiles_from(&base).unwrap() {
            let profile_dir = base.join(&name);
            for file_path in collect_files_recursive_disk(&profile_dir) {
                let path_str = file_path.to_string_lossy().to_string();
                if path_str.ends_with("ralph.yml")
                    && !path_str.contains("formations/")
                {
                    let content = fs::read_to_string(&file_path).unwrap();
                    if !content.contains("+agent:claude-code") {
                        continue;
                    }
                    let filtered = filter_agent_tags(&content, "gemini-cli", CommentSyntax::Hash);
                    assert!(
                        !filtered.contains("backend: claude"),
                        "Filtering {} for gemini-cli should exclude backend: claude", path_str
                    );
                }
            }
        }
    }
}
