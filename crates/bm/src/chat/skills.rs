use std::collections::HashSet;
use std::fs;
use std::path::Path;

use serde::Deserialize;

use super::SkillInfo;

/// Frontmatter fields extracted from SKILL.md files.
#[derive(Deserialize)]
struct SkillFrontmatter {
    name: String,
    description: String,
}

/// Scans skill directories for SKILL.md files and extracts skill metadata.
///
/// For each dir in `dirs`, resolves it relative to `ws_path`, lists subdirectories,
/// reads `SKILL.md` frontmatter, and collects skill entries. Skips template paths
/// containing `<project>` placeholders. Results are sorted by name and deduplicated.
pub fn scan_skills(ws_path: &Path, dirs: &[String]) -> Vec<SkillInfo> {
    let mut skills: Vec<SkillInfo> = Vec::new();
    let mut seen_names: HashSet<String> = HashSet::new();

    for dir in dirs {
        let skill_dir = ws_path.join(dir);
        let entries = match fs::read_dir(&skill_dir) {
            Ok(entries) => entries,
            Err(_) => continue, // Skip non-existent dirs silently
        };

        let mut subdirs: Vec<_> = entries
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .collect();
        subdirs.sort_by_key(|e| e.file_name());

        for entry in subdirs {
            let skill_md_path = entry.path().join("SKILL.md");
            if !skill_md_path.exists() {
                continue;
            }

            let content = match fs::read_to_string(&skill_md_path) {
                Ok(c) => c,
                Err(_) => continue,
            };

            // Extract YAML frontmatter between --- markers
            if let Some(frontmatter) = extract_frontmatter(&content) {
                if let Ok(meta) = serde_yml::from_str::<SkillFrontmatter>(&frontmatter) {
                    if seen_names.contains(&meta.name) {
                        continue; // Deduplicate by name
                    }

                    let description = truncate_description(&meta.description);
                    let relative_path = format!(
                        "{}/{}/SKILL.md",
                        dir,
                        entry.file_name().to_string_lossy()
                    );

                    seen_names.insert(meta.name.clone());
                    skills.push(SkillInfo {
                        name: meta.name,
                        description,
                        load_command: relative_path,
                    });
                }
            }
        }
    }

    skills.sort_by(|a, b| a.name.cmp(&b.name));
    skills
}

/// Extracts YAML frontmatter from between `---` markers.
pub fn extract_frontmatter(content: &str) -> Option<String> {
    let trimmed = content.trim_start();
    if !trimmed.starts_with("---") {
        return None;
    }
    let after_first = &trimmed[3..];
    let end = after_first.find("\n---")?;
    Some(after_first[..end].to_string())
}

/// Truncates a description to the first sentence or 120 chars, whichever is shorter.
pub fn truncate_description(desc: &str) -> String {
    // Normalize whitespace (multiline YAML descriptions may have newlines)
    let normalized: String = desc.split_whitespace().collect::<Vec<_>>().join(" ");

    // Find first sentence boundary
    let sentence_end = normalized
        .find(". ")
        .or_else(|| normalized.find(".\n"))
        .map(|pos| pos + 1); // Include the period

    let truncated = match sentence_end {
        Some(end) if end <= 120 => &normalized[..end],
        _ => {
            if normalized.len() <= 120 {
                &normalized
            } else {
                // Find last space before 120 chars to avoid cutting words
                let cut = normalized[..120]
                    .rfind(' ')
                    .unwrap_or(120);
                &normalized[..cut]
            }
        }
    };

    truncated.trim().to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scan_skills_from_filesystem() {
        let tmp = tempfile::tempdir().unwrap();
        let skills_dir = tmp.path().join("team/coding-agent/skills");

        // Create gh skill
        let gh_dir = skills_dir.join("gh");
        fs::create_dir_all(&gh_dir).unwrap();
        fs::write(
            gh_dir.join("SKILL.md"),
            "---\nname: gh\ndescription: Manages GitHub Projects v2 workflows and issue operations.\n---\n\n# GH Skill\n",
        )
        .unwrap();

        // Create status-workflow skill
        let sw_dir = skills_dir.join("status-workflow");
        fs::create_dir_all(&sw_dir).unwrap();
        fs::write(
            sw_dir.join("SKILL.md"),
            "---\nname: status-workflow\ndescription: Performs status transitions on issues.\n---\n\n# Status Workflow\n",
        )
        .unwrap();

        let dirs = vec!["team/coding-agent/skills".to_string()];
        let result = scan_skills(tmp.path(), &dirs);

        assert_eq!(result.len(), 2);
        // Sorted by name
        assert_eq!(result[0].name, "gh");
        assert!(result[0].description.starts_with("Manages GitHub"));
        assert_eq!(
            result[0].load_command,
            "team/coding-agent/skills/gh/SKILL.md"
        );
        assert_eq!(result[1].name, "status-workflow");
    }

    #[test]
    fn scan_skills_deduplicates_by_name() {
        let tmp = tempfile::tempdir().unwrap();

        // Create same skill in two dirs
        for dir_name in &["team/skills", "member/skills"] {
            let skill_dir = tmp.path().join(dir_name).join("gh");
            fs::create_dir_all(&skill_dir).unwrap();
            fs::write(
                skill_dir.join("SKILL.md"),
                "---\nname: gh\ndescription: GitHub skill\n---\n",
            )
            .unwrap();
        }

        let dirs = vec![
            "team/skills".to_string(),
            "member/skills".to_string(),
        ];
        let result = scan_skills(tmp.path(), &dirs);
        assert_eq!(result.len(), 1, "Should deduplicate skills with same name");
        assert_eq!(result[0].name, "gh");
    }

    #[test]
    fn scan_skills_skips_missing_dirs() {
        let tmp = tempfile::tempdir().unwrap();
        let dirs = vec!["nonexistent/skills".to_string()];
        let result = scan_skills(tmp.path(), &dirs);
        assert!(result.is_empty());
    }

    #[test]
    fn scan_skills_skips_dirs_without_skill_md() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_dir = tmp.path().join("skills/empty-skill");
        fs::create_dir_all(&skill_dir).unwrap();
        // No SKILL.md file

        let dirs = vec!["skills".to_string()];
        let result = scan_skills(tmp.path(), &dirs);
        assert!(result.is_empty());
    }

    #[test]
    fn truncate_description_first_sentence() {
        assert_eq!(
            truncate_description("Manages GitHub workflows. Also does other things."),
            "Manages GitHub workflows."
        );
    }

    #[test]
    fn truncate_description_short_no_truncation() {
        assert_eq!(
            truncate_description("Short description"),
            "Short description"
        );
    }

    #[test]
    fn truncate_description_long_text() {
        let long = "A".repeat(200);
        let result = truncate_description(&long);
        assert!(result.len() <= 120);
    }

    #[test]
    fn extract_frontmatter_basic() {
        let content = "---\nname: gh\ndescription: test\n---\n\n# Content\n";
        let fm = extract_frontmatter(content).unwrap();
        assert!(fm.contains("name: gh"));
        assert!(fm.contains("description: test"));
    }

    #[test]
    fn extract_frontmatter_no_markers() {
        assert!(extract_frontmatter("No frontmatter here").is_none());
    }

    #[test]
    fn truncate_description_multiline_yaml() {
        let desc = "Manages GitHub\nProjects v2 workflows. Also does other things.";
        let result = truncate_description(desc);
        assert_eq!(result, "Manages GitHub Projects v2 workflows.");
    }
}
