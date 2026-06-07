use std::collections::BTreeSet;
use std::io::{self, BufRead, Write};

use anyhow::{Context, Result};

use crate::profile;

/// Action taken for each profile during init.
#[derive(Debug, PartialEq)]
enum Action {
    New,
    Overwritten,
    Skipped,
}

/// Handles `bm profiles init [--force]`.
pub fn run(force: bool) -> Result<()> {
    run_with_reader(force, &mut io::stdin().lock())
}

/// Testable inner function that accepts a reader for stdin.
fn run_with_reader(force: bool, reader: &mut dyn BufRead) -> Result<()> {
    let target = profile::profiles_dir()?;
    let embedded = profile::embedded::list_embedded_profiles();

    let existing_on_disk: BTreeSet<String> = if target.exists() {
        std::fs::read_dir(&target)
            .context("Failed to read profiles directory")?
            .filter_map(|e| e.ok())
            .filter(|e| e.file_type().map(|t| t.is_dir()).unwrap_or(false))
            .filter_map(|e| e.file_name().into_string().ok())
            .collect()
    } else {
        BTreeSet::new()
    };

    std::fs::create_dir_all(&target).with_context(|| {
        format!("Failed to create profiles directory {}", target.display())
    })?;

    // Fresh install — extract everything
    if existing_on_disk.is_empty() {
        let count = profile::extract_embedded_to_disk(&target)?;
        println!("Extracted {} profiles to {}", count, target.display());
        extract_minty(&target)?;
        return Ok(());
    }

    // Per-profile decision
    let mut actions: Vec<(String, Action)> = Vec::new();
    for name in &embedded {
        if !existing_on_disk.contains(name) {
            profile::extract_single_profile_to_disk(name, &target)?;
            actions.push((name.clone(), Action::New));
        } else if force {
            profile::extract_single_profile_to_disk(name, &target)?;
            actions.push((name.clone(), Action::Overwritten));
        } else {
            let overwrite = prompt_overwrite(name, reader)?;
            if overwrite {
                profile::extract_single_profile_to_disk(name, &target)?;
                actions.push((name.clone(), Action::Overwritten));
            } else {
                actions.push((name.clone(), Action::Skipped));
            }
        }
    }

    // Summary
    let counts = (
        actions.iter().filter(|(_, a)| *a == Action::New).count(),
        actions.iter().filter(|(_, a)| *a == Action::Overwritten).count(),
        actions.iter().filter(|(_, a)| *a == Action::Skipped).count(),
    );
    println!("\nProfiles directory: {}", target.display());
    for (name, action) in &actions {
        let label = match action {
            Action::New => "new",
            Action::Overwritten => "overwritten",
            Action::Skipped => "skipped",
        };
        println!("  {} ({})", name, label);
    }
    println!("\nSummary: {} new, {} overwritten, {} skipped", counts.0, counts.1, counts.2);

    extract_minty(&target)?;
    Ok(())
}

fn prompt_overwrite(profile_name: &str, reader: &mut dyn BufRead) -> Result<bool> {
    print!("Overwrite {}? [y/N] ", profile_name);
    io::stdout().flush()?;
    let mut line = String::new();
    let bytes_read = reader.read_line(&mut line)?;
    if bytes_read == 0 {
        println!();
        return Ok(false);
    }
    let answer = line.trim().to_lowercase();
    Ok(answer == "y" || answer == "yes")
}

fn extract_minty(profiles_target: &std::path::Path) -> Result<()> {
    let botminter_config = profiles_target
        .parent()
        .context("Could not determine botminter config parent directory")?;
    let minty_target = botminter_config.join("minty");
    std::fs::create_dir_all(&minty_target).with_context(|| {
        format!("Failed to create minty directory {}", minty_target.display())
    })?;
    profile::extract_minty_to_disk(&minty_target)?;
    println!("Extracted minty config to {}", minty_target.display());
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn setup_existing_profiles(target: &std::path::Path) {
        profile::extract_embedded_to_disk(target).unwrap();
    }

    #[test]
    fn fresh_install_extracts_all_without_prompting() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&target).unwrap();
        let count = profile::extract_embedded_to_disk(&target).unwrap();
        assert!(count >= 2);
        for name in profile::embedded::list_embedded_profiles() {
            assert!(target.join(&name).join("botminter.yml").exists());
        }
    }

    #[test]
    fn force_overwrites_all_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("profiles");
        std::fs::create_dir_all(&target).unwrap();
        setup_existing_profiles(&target);

        let sample_profile = &profile::embedded::list_embedded_profiles()[0];
        let marker = target.join(sample_profile).join("botminter.yml");
        std::fs::write(&marker, "modified").unwrap();

        for name in profile::embedded::list_embedded_profiles() {
            profile::extract_single_profile_to_disk(&name, &target).unwrap();
        }

        let content = std::fs::read_to_string(&marker).unwrap();
        assert_ne!(content, "modified");
        assert!(content.contains("name:"));
    }

    #[test]
    fn prompt_overwrite_yes_returns_true() {
        let mut input = io::Cursor::new(b"y\n");
        assert!(prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn prompt_overwrite_yes_full_returns_true() {
        let mut input = io::Cursor::new(b"yes\n");
        assert!(prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn prompt_overwrite_no_returns_false() {
        let mut input = io::Cursor::new(b"n\n");
        assert!(!prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn prompt_overwrite_empty_returns_false() {
        let mut input = io::Cursor::new(b"\n");
        assert!(!prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn prompt_overwrite_eof_returns_false() {
        let mut input = io::Cursor::new(b"");
        assert!(!prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn prompt_overwrite_case_insensitive() {
        let mut input = io::Cursor::new(b"Y\n");
        assert!(prompt_overwrite("scrum", &mut input).unwrap());
    }

    #[test]
    fn run_with_reader_force_overwrites_existing() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("profiles");
        std::fs::create_dir_all(&target).unwrap();
        setup_existing_profiles(&target);

        let sample_profile = &profile::embedded::list_embedded_profiles()[0];
        let marker = target.join(sample_profile).join("botminter.yml");
        std::fs::write(&marker, "modified-by-test").unwrap();

        for name in profile::embedded::list_embedded_profiles() {
            profile::extract_single_profile_to_disk(&name, &target).unwrap();
        }

        let content = std::fs::read_to_string(&marker).unwrap();
        assert!(content.contains("name:"));
    }

    #[test]
    fn skipped_profile_preserves_disk_content() {
        let tmp = tempfile::tempdir().unwrap();
        let target = tmp.path().join("profiles");
        std::fs::create_dir_all(&target).unwrap();
        setup_existing_profiles(&target);

        let skipped_profile = &profile::embedded::list_embedded_profiles()[0];
        let marker = target.join(skipped_profile).join("botminter.yml");
        std::fs::write(&marker, "custom-content").unwrap();

        for name in profile::embedded::list_embedded_profiles() {
            if &name != skipped_profile {
                profile::extract_single_profile_to_disk(&name, &target).unwrap();
            }
        }

        let content = std::fs::read_to_string(&marker).unwrap();
        assert_eq!(content, "custom-content");
    }

    #[test]
    fn extract_single_profile_creates_correct_dir() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles = profile::embedded::list_embedded_profiles();
        let name = &profiles[0];
        profile::extract_single_profile_to_disk(name, tmp.path()).unwrap();
        assert!(tmp.path().join(name).is_dir());
        assert!(tmp.path().join(name).join("botminter.yml").exists());
    }

    #[test]
    fn extract_single_profile_nonexistent_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let result = profile::extract_single_profile_to_disk("nonexistent", tmp.path());
        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn extract_minty_creates_config_files() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        let minty_dir = tmp.path().join("botminter").join("minty");
        assert!(minty_dir.join("prompt.md").exists());
        assert!(minty_dir.join("config.yml").exists());
        assert!(minty_dir.join(".claude").join("skills").is_dir());
    }

    #[test]
    fn extract_minty_skills_creates_all_skill_directories() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        let skills_dir = tmp.path().join("botminter").join("minty").join(".claude").join("skills");
        for skill in &["team-overview", "profile-browser", "hire-guide", "workspace-doctor"] {
            assert!(skills_dir.join(skill).join("SKILL.md").exists());
        }
    }

    #[test]
    fn extract_minty_skills_have_valid_frontmatter() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        let skills_dir = tmp.path().join("botminter").join("minty").join(".claude").join("skills");
        for skill in &["team-overview", "profile-browser", "hire-guide", "workspace-doctor"] {
            let content = std::fs::read_to_string(skills_dir.join(skill).join("SKILL.md")).unwrap();
            assert!(content.starts_with("---\n"));
            let frontmatter_end = content[4..].find("\n---\n")
                .unwrap_or_else(|| panic!("Skill {} should have closing frontmatter delimiter", skill));
            let frontmatter = &content[4..4 + frontmatter_end];
            assert!(frontmatter.contains("name:"));
            assert!(frontmatter.contains("description:"));
            assert!(frontmatter.contains("version:"));
        }
    }

    #[test]
    fn extract_minty_prompt_contains_persona() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        let prompt = std::fs::read_to_string(
            tmp.path().join("botminter").join("minty").join("prompt.md"),
        ).unwrap();
        assert!(prompt.contains("Minty"));
        assert!(prompt.contains("BotMinter"));
        assert!(prompt.contains("profiles-only mode"));
    }

    #[test]
    fn extract_minty_config_is_valid_yaml() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        let config_content = std::fs::read_to_string(
            tmp.path().join("botminter").join("minty").join("config.yml"),
        ).unwrap();
        let yaml: serde_yml::Value = serde_yml::from_str(&config_content)
            .expect("config.yml should be valid YAML");
        assert!(yaml.get("prompt").is_some());
        assert!(yaml.get("skills_dir").is_some());
    }

    #[test]
    fn extract_minty_is_idempotent() {
        let tmp = tempfile::tempdir().unwrap();
        let profiles_target = tmp.path().join("botminter").join("profiles");
        std::fs::create_dir_all(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        extract_minty(&profiles_target).unwrap();
        assert!(tmp.path().join("botminter").join("minty").join("prompt.md").exists());
    }
}
