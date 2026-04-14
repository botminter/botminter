use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use super::CodingAgentDef;
use crate::git::run_git;

/// Result of hiring a new member into a team.
#[derive(Debug)]
pub struct HireResult {
    /// The full member directory name (e.g., "architect-01").
    pub member_dir_name: String,
    /// The member's display name (e.g., "01").
    pub member_name: String,
    /// True when the member directory already existed (skipped extraction).
    /// The caller can use this to decide whether to attach credentials
    /// without re-creating the member skeleton.
    pub already_existed: bool,
}

/// Hires a new member into a team by extracting the role skeleton, finalizing
/// the manifest, and committing to the team repo.
///
/// Does NOT handle bridge token prompting — that's interactive I/O for the
/// command layer.
pub fn hire_member(
    team_repo: &Path,
    team_profile: &str,
    role: &str,
    name: Option<&str>,
    coding_agent: &CodingAgentDef,
) -> Result<HireResult> {
    // Verify role exists in profile
    let available_roles = super::list_roles(team_profile)?;
    if !available_roles.contains(&role.to_string()) {
        bail!(
            "Role '{}' not available in profile '{}'. Available roles: {}",
            role,
            team_profile,
            available_roles.join(", ")
        );
    }

    // Determine member name
    let member_name = match name {
        Some(n) => n.to_string(),
        None => auto_suffix(team_repo, role)?,
    };

    let member_dir_name = format!("{}-{}", role, member_name);
    let member_dir = team_repo.join("members").join(&member_dir_name);

    if member_dir.exists() {
        return Ok(HireResult {
            member_dir_name,
            member_name,
            already_existed: true,
        });
    }

    // Extract member skeleton from embedded profile
    fs::create_dir_all(&member_dir)
        .with_context(|| format!("Failed to create member dir {}", member_dir.display()))?;

    super::extract_member_to(team_profile, role, &member_dir, coding_agent)?;

    // Finalize member manifest: .botminter.yml → botminter.yml with name added
    finalize_member_manifest(&member_dir, &member_name)?;

    // Render {{member_dir}} placeholders in all text files
    render_member_placeholders(&member_dir, &member_dir_name, role, &member_name)?;

    // Git add + commit (no auto-push)
    run_git(
        team_repo,
        &["add", &format!("members/{}/", member_dir_name)],
    )?;
    let commit_msg = format!("feat: hire {} as {}", role, member_name);
    run_git(team_repo, &["commit", "-m", &commit_msg])?;

    Ok(HireResult {
        member_dir_name,
        member_name,
        already_existed: false,
    })
}

/// Computes the next auto-suffix for a role by scanning existing member dirs.
/// Returns a 2-digit, zero-padded string (e.g., "01", "02").
/// Fills gaps: if 01 and 03 exist, returns "02".
pub fn auto_suffix(team_repo: &Path, role: &str) -> Result<String> {
    let team_members_dir = team_repo.join("members");
    let prefix = format!("{}-", role);

    let mut used: Vec<u32> = Vec::new();

    if team_members_dir.is_dir() {
        for entry in fs::read_dir(&team_members_dir)? {
            let entry = entry?;
            if !entry.file_type()?.is_dir() {
                continue;
            }
            let name = entry.file_name().to_string_lossy().to_string();
            if name.starts_with(&prefix) {
                let suffix = &name[prefix.len()..];
                if let Ok(n) = suffix.parse::<u32>() {
                    used.push(n);
                }
            }
        }
    }

    used.sort();

    let mut next = 1u32;
    for &n in &used {
        if n == next {
            next = n + 1;
        } else if n > next {
            break;
        }
    }

    Ok(format!("{:02}", next))
}

/// Reads the .botminter.yml template, augments with the member name, and
/// writes as botminter.yml (without the dot prefix).
pub fn finalize_member_manifest(member_dir: &Path, name: &str) -> Result<()> {
    let template_path = member_dir.join(".botminter.yml");
    if template_path.exists() {
        let contents = fs::read_to_string(&template_path)
            .context("Failed to read .botminter.yml template")?;

        let mut value: serde_yml::Value =
            serde_yml::from_str(&contents).context("Failed to parse .botminter.yml")?;

        if let serde_yml::Value::Mapping(ref mut map) = value {
            map.insert(
                serde_yml::Value::String("name".to_string()),
                serde_yml::Value::String(name.to_string()),
            );
        }

        let augmented =
            serde_yml::to_string(&value).context("Failed to serialize member manifest")?;

        let manifest_path = member_dir.join("botminter.yml");
        fs::write(&manifest_path, augmented)
            .context("Failed to write member botminter.yml")?;

        fs::remove_file(&template_path).ok();
    }

    Ok(())
}

/// Renders `{{member_dir}}`, `{{role}}`, and `{{member_name}}` placeholders
/// in all text files within the member directory. Called during `bm hire`
/// after the member skeleton is extracted and the manifest is finalized.
pub(crate) fn render_member_placeholders(
    member_dir: &Path,
    member_dir_name: &str,
    role: &str,
    member_name: &str,
) -> Result<()> {
    render_placeholders_recursive(member_dir, member_dir_name, role, member_name)
}

fn render_placeholders_recursive(
    dir: &Path,
    member_dir_name: &str,
    role: &str,
    member_name: &str,
) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let path = entry.path();

        if path.is_dir() {
            render_placeholders_recursive(&path, member_dir_name, role, member_name)?;
        } else if is_text_file(&path) {
            let content = fs::read_to_string(&path)?;
            if content.contains("{{member_dir}}")
                || content.contains("{{role}}")
                || content.contains("{{member_name}}")
            {
                let rendered = content
                    .replace("{{member_dir}}", member_dir_name)
                    .replace("{{role}}", role)
                    .replace("{{member_name}}", member_name);
                fs::write(&path, rendered)?;
            }
        }
    }
    Ok(())
}

fn is_text_file(path: &Path) -> bool {
    matches!(
        path.extension().and_then(|e| e.to_str()),
        Some(
            "md" | "yml" | "yaml" | "json" | "txt" | "sh" | "toml" | "graphql" | "env" | "cfg"
                | "conf" | "ini" | "xml" | "html" | "css" | "js" | "ts" | "py" | "rs" | "go"
                | "rb" | "dot"
        )
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn auto_suffix_first_member() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        fs::create_dir_all(team_repo.join("members")).unwrap();

        let result = auto_suffix(team_repo, "architect").unwrap();
        assert_eq!(result, "01");
    }

    #[test]
    fn auto_suffix_increments() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        fs::create_dir_all(team_repo.join("members/architect-01")).unwrap();

        let result = auto_suffix(team_repo, "architect").unwrap();
        assert_eq!(result, "02");
    }

    #[test]
    fn auto_suffix_fills_gaps() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        fs::create_dir_all(team_repo.join("members/architect-01")).unwrap();
        fs::create_dir_all(team_repo.join("members/architect-03")).unwrap();

        let result = auto_suffix(team_repo, "architect").unwrap();
        assert_eq!(result, "02");
    }

    #[test]
    fn auto_suffix_skips_non_numeric() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        fs::create_dir_all(team_repo.join("members/architect-bob")).unwrap();
        fs::create_dir_all(team_repo.join("members/architect-01")).unwrap();

        let result = auto_suffix(team_repo, "architect").unwrap();
        assert_eq!(result, "02");
    }

    #[test]
    fn auto_suffix_different_roles_independent() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        fs::create_dir_all(team_repo.join("members/architect-01")).unwrap();
        fs::create_dir_all(team_repo.join("members/architect-02")).unwrap();
        fs::create_dir_all(team_repo.join("members/dev-01")).unwrap();

        let result = auto_suffix(team_repo, "dev").unwrap();
        assert_eq!(result, "02");

        let result = auto_suffix(team_repo, "architect").unwrap();
        assert_eq!(result, "03");
    }

    #[test]
    fn hire_existing_member_returns_already_existed() {
        // Set HOME to a temp dir so profiles_dir() resolves there
        let home = tempfile::tempdir().unwrap();
        let profiles_path = crate::profile::profiles_dir_for(home.path());
        fs::create_dir_all(&profiles_path).unwrap();
        crate::profile::extract_embedded_to_disk(&profiles_path).unwrap();

        // Temporarily override HOME for list_roles() resolution
        let orig_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", home.path());

        // Set up minimal git config so commits work in the temp HOME
        let gitconfig = home.path().join(".gitconfig");
        fs::write(&gitconfig, "[user]\n\tname = Test\n\temail = test@test.com\n").unwrap();

        let team_tmp = tempfile::tempdir().unwrap();
        let team_repo = team_tmp.path();
        let profile = "agentic-sdlc-minimal";
        let manifest = crate::profile::read_manifest_from(profile, &profiles_path).unwrap();
        let coding_agent = manifest
            .coding_agents
            .get(&manifest.default_coding_agent)
            .unwrap();

        // Bootstrap a minimal team repo
        crate::formation::setup_new_team_repo(
            team_repo, profile, &manifest, &[], &[], None, Some(&profiles_path),
        )
        .unwrap();

        // First hire — creates directory
        let r1 = hire_member(team_repo, profile, "engineer", Some("01"), coding_agent).unwrap();
        assert!(!r1.already_existed, "First hire should create a new member");
        assert_eq!(r1.member_dir_name, "engineer-01");

        // Second hire with same name — returns already_existed
        let r2 = hire_member(team_repo, profile, "engineer", Some("01"), coding_agent).unwrap();
        assert!(r2.already_existed, "Second hire should detect existing member");
        assert_eq!(r2.member_dir_name, "engineer-01");
        assert_eq!(r2.member_name, "01");

        // Restore HOME
        match orig_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn render_member_placeholders_replaces_all() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path();

        // Create a test file with placeholders
        fs::write(
            member_dir.join("test.md"),
            "Knowledge at team/members/{{member_dir}}/knowledge/\nRole: {{role}}\nName: {{member_name}}\n",
        )
        .unwrap();

        // Create a subdirectory with a file
        fs::create_dir_all(member_dir.join("hats")).unwrap();
        fs::write(
            member_dir.join("hats/config.yml"),
            "path: team/members/{{member_dir}}/hats/arch/knowledge/\n",
        )
        .unwrap();

        render_member_placeholders(member_dir, "superman-alice", "superman", "alice").unwrap();

        let content = fs::read_to_string(member_dir.join("test.md")).unwrap();
        assert!(content.contains("team/members/superman-alice/knowledge/"));
        assert!(content.contains("Role: superman"));
        assert!(content.contains("Name: alice"));
        assert!(!content.contains("{{member_dir}}"));

        let hat_content = fs::read_to_string(member_dir.join("hats/config.yml")).unwrap();
        assert!(hat_content.contains("team/members/superman-alice/hats/arch/knowledge/"));
    }

    #[test]
    fn render_member_placeholders_skips_binary_files() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path();

        // Create a non-text file
        fs::write(member_dir.join("image.png"), b"\x89PNG\r\n").unwrap();

        // Should not panic or error on binary files
        render_member_placeholders(member_dir, "superman-bob", "superman", "bob").unwrap();
    }
}
