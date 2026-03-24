use std::fs;

use anyhow::{bail, Context, Result};

use crate::config;
use crate::profile;

/// Handles `bm knowledge list [-t team] [--scope <scope>]`.
pub fn list(team_flag: Option<&str>, scope_filter: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let team_schema = profile::read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    println!("Team: {} (schema {})", team.name, team_schema);
    println!();

    let show_scope = |scope: &str| -> bool {
        scope_filter.is_none() || scope_filter == Some(scope)
    };

    if show_scope("team") {
        println!("Team scope:");
        display_scope_dir(&team_repo, "knowledge");
        display_scope_dir(&team_repo, "invariants");
        println!();
    }

    if show_scope("project") {
        let projects_dir = team_repo.join("projects");
        for project in &profile::list_subdirs(&projects_dir) {
            println!("Project scope ({}):", project);
            display_scope_dir(&projects_dir.join(project), "knowledge");
            display_scope_dir(&projects_dir.join(project), "invariants");
            println!();
        }
    }

    if show_scope("member") {
        let members_dir = team_repo.join("members");
        for member in &profile::list_subdirs(&members_dir) {
            println!("Member scope ({}):", member);
            display_scope_dir(&members_dir.join(member), "knowledge");
            display_scope_dir(&members_dir.join(member), "invariants");
            println!();
        }
    }

    if show_scope("member-project") {
        let members_dir = team_repo.join("members");
        for member in &profile::list_subdirs(&members_dir) {
            let member_projects_dir = members_dir.join(member).join("projects");
            for project in &profile::list_subdirs(&member_projects_dir) {
                println!("Member+Project scope ({}/{}):", member, project);
                display_scope_dir(&member_projects_dir.join(project), "knowledge");
                println!();
            }
        }
    }

    Ok(())
}

/// Handles `bm knowledge show <path> [-t team]`.
pub fn show(path: &str, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let team_schema = profile::read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;
    profile::validate_knowledge_path(path)?;

    let file_path = team_repo.join(path);

    let canonical_repo = team_repo
        .canonicalize()
        .context("Failed to resolve team repo path")?;
    if file_path.exists() {
        let canonical_file = file_path
            .canonicalize()
            .context("Failed to resolve file path")?;
        if !canonical_file.starts_with(&canonical_repo) {
            bail!("Path resolves outside the team repo");
        }
    }

    if !file_path.exists() {
        bail!("File not found: {}", path);
    }

    let contents = fs::read_to_string(&file_path)
        .with_context(|| format!("Failed to read {}", path))?;
    print!("{}", contents);

    Ok(())
}

/// Handles `bm knowledge [-t team]` (bare — launches interactive Claude session).
pub fn interactive(team_flag: Option<&str>, _scope: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let team_schema = profile::read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    let skill_path = team_repo.join("skills/knowledge-manager/SKILL.md");
    if !skill_path.exists() {
        bail!(
            "Knowledge manager skill not found at {}. \
             Ensure the team was initialized with a v2 profile.",
            skill_path.display()
        );
    }

    let manifest = profile::read_manifest_from(&team.profile, &profile::profiles_dir()?)?;
    let coding_agent = profile::resolve_coding_agent(&team, &manifest)?;

    crate::session::interactive_agent_session(
        &team_repo,
        &skill_path,
        &profile::credentials_env(&team),
        coding_agent,
    )
}

/// Displays .md files in a scope subdirectory, formatted for the knowledge list.
fn display_scope_dir(base: &std::path::Path, subdir: &str) {
    println!("  {}/", subdir);
    let files = profile::list_scope_files(base, subdir);
    if files.is_empty() {
        println!("    (none)");
    } else {
        for file in &files {
            println!("    {}", file);
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::profile;

    // ── validate_knowledge_path ──────────────────────────────────────

    #[test]
    fn valid_team_knowledge_path() {
        assert!(profile::validate_knowledge_path("knowledge/commit-convention.md").is_ok());
    }

    #[test]
    fn valid_team_invariant_path() {
        assert!(profile::validate_knowledge_path("invariants/code-review.md").is_ok());
    }

    #[test]
    fn valid_project_knowledge_path() {
        assert!(profile::validate_knowledge_path("projects/my-project/knowledge/api.md").is_ok());
    }

    #[test]
    fn valid_project_invariant_path() {
        assert!(
            profile::validate_knowledge_path("projects/my-project/invariants/test.md").is_ok()
        );
    }

    #[test]
    fn valid_member_knowledge_path() {
        assert!(
            profile::validate_knowledge_path("members/architect-alice/knowledge/patterns.md")
                .is_ok()
        );
    }

    #[test]
    fn valid_member_invariant_path() {
        assert!(
            profile::validate_knowledge_path("members/architect-alice/invariants/quality.md")
                .is_ok()
        );
    }

    #[test]
    fn valid_member_project_knowledge_path() {
        assert!(profile::validate_knowledge_path(
            "members/architect-alice/projects/my-project/knowledge/notes.md"
        )
        .is_ok());
    }

    #[test]
    fn invalid_path_botminter_yml() {
        let result = profile::validate_knowledge_path("botminter.yml");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not within a knowledge or invariant directory"));
    }

    #[test]
    fn invalid_path_random_file() {
        let result = profile::validate_knowledge_path("PROCESS.md");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_path_member_no_scope() {
        let result = profile::validate_knowledge_path("members/architect-alice/PROMPT.md");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_path_project_no_scope() {
        let result = profile::validate_knowledge_path("projects/my-project/README.md");
        assert!(result.is_err());
    }
}
