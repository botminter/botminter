use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::config::{self, TeamEntry};
use crate::profile;

/// Handles `bm knowledge list [-t team] [--scope <scope>]`.
pub fn list(team_flag: Option<&str>, scope_filter: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let team_schema = read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    println!("Team: {} (schema {})", team.name, team_schema);
    println!();

    let show_scope = |scope: &str| -> bool {
        scope_filter.is_none() || scope_filter == Some(scope)
    };

    // Team scope
    if show_scope("team") {
        println!("Team scope:");
        list_scope_dir(&team_repo, "knowledge");
        list_scope_dir(&team_repo, "invariants");
        println!();
    }

    // Project scope
    if show_scope("project") {
        let projects_dir = team_repo.join("projects");
        if projects_dir.is_dir() {
            let projects = list_subdirs(&projects_dir);
            for project in &projects {
                println!("Project scope ({}):", project);
                let proj_dir = projects_dir.join(project);
                list_scope_dir(&proj_dir, "knowledge");
                list_scope_dir(&proj_dir, "invariants");
                println!();
            }
        }
    }

    // Member scope
    if show_scope("member") {
        let members_dir = team_repo.join("team");
        if members_dir.is_dir() {
            let members = list_subdirs(&members_dir);
            for member in &members {
                println!("Member scope ({}):", member);
                let member_dir = members_dir.join(member);
                list_scope_dir(&member_dir, "knowledge");
                list_scope_dir(&member_dir, "invariants");
                println!();
            }
        }
    }

    // Member+Project scope
    if show_scope("member-project") {
        let members_dir = team_repo.join("team");
        if members_dir.is_dir() {
            let members = list_subdirs(&members_dir);
            for member in &members {
                let member_projects_dir = members_dir.join(member).join("projects");
                if member_projects_dir.is_dir() {
                    let projects = list_subdirs(&member_projects_dir);
                    for project in &projects {
                        println!("Member+Project scope ({}/{}):", member, project);
                        let mp_dir = member_projects_dir.join(project);
                        list_scope_dir(&mp_dir, "knowledge");
                        println!();
                    }
                }
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

    let team_schema = read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    // Validate the path is within a knowledge or invariant directory
    validate_knowledge_path(path)?;

    let file_path = team_repo.join(path);

    // Prevent path traversal
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

    let team_schema = read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    // Check skill exists
    let skill_path = team_repo.join("skills/knowledge-manager/SKILL.md");
    if !skill_path.exists() {
        bail!(
            "Knowledge manager skill not found at {}. \
             Ensure the team was initialized with a v2 profile.",
            skill_path.display()
        );
    }

    // Delegate to session abstraction (will be implemented in session.rs)
    crate::session::interactive_claude_session(
        &team_repo,
        &skill_path,
        &credentials_env(team),
    )
}

/// Builds env vars from team credentials.
fn credentials_env(team: &TeamEntry) -> Vec<(String, String)> {
    let mut env = Vec::new();
    if let Some(token) = &team.credentials.gh_token {
        env.push(("GH_TOKEN".to_string(), token.clone()));
    }
    if let Some(token) = &team.credentials.telegram_bot_token {
        env.push(("RALPH_TELEGRAM_BOT_TOKEN".to_string(), token.clone()));
    }
    env
}

/// Validates that a path points to a knowledge or invariant file.
fn validate_knowledge_path(path: &str) -> Result<()> {
    // Valid path patterns:
    // knowledge/...
    // invariants/...
    // projects/<project>/knowledge/...
    // projects/<project>/invariants/...
    // team/<member>/knowledge/...
    // team/<member>/invariants/...
    // team/<member>/projects/<project>/knowledge/...
    let parts: Vec<&str> = path.split('/').collect();

    let is_knowledge_or_invariant = |segment: &str| -> bool {
        segment == "knowledge" || segment == "invariants"
    };

    let valid = match parts.first() {
        Some(&"knowledge") | Some(&"invariants") => true,
        Some(&"projects") => {
            // projects/<name>/knowledge/... or projects/<name>/invariants/...
            parts.len() >= 3 && is_knowledge_or_invariant(parts[2])
        }
        Some(&"team") => {
            // team/<member>/knowledge/... or team/<member>/invariants/...
            // or team/<member>/projects/<project>/knowledge/...
            (parts.len() >= 3 && is_knowledge_or_invariant(parts[2]))
                || (parts.len() >= 5
                    && parts[2] == "projects"
                    && is_knowledge_or_invariant(parts[4]))
        }
        _ => false,
    };

    if !valid {
        bail!("Path is not within a knowledge or invariant directory");
    }

    Ok(())
}

/// Reads the schema version from the team's botminter.yml.
fn read_team_schema(team_repo: &Path) -> Result<String> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        bail!(
            "Team repo at {} has no botminter.yml",
            team_repo.display()
        );
    }
    let contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml")?;
    let val: serde_yml::Value =
        serde_yml::from_str(&contents).context("Failed to parse team botminter.yml")?;
    Ok(val["schema_version"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// Lists .md files in a scope subdirectory, printing them indented.
fn list_scope_dir(base: &Path, subdir: &str) {
    let dir = base.join(subdir);
    println!("  {}/", subdir);

    if !dir.is_dir() {
        println!("    (none)");
        return;
    }

    let mut files: Vec<String> = Vec::new();
    if let Ok(entries) = fs::read_dir(&dir) {
        for entry in entries.flatten() {
            let name = entry.file_name().to_string_lossy().to_string();
            if name.ends_with(".md") {
                files.push(name);
            }
        }
    }

    files.sort();

    if files.is_empty() {
        println!("    (none)");
    } else {
        for file in &files {
            println!("    {}", file);
        }
    }
}

/// Lists non-hidden subdirectory names, sorted.
fn list_subdirs(dir: &Path) -> Vec<String> {
    let mut dirs = Vec::new();
    if let Ok(entries) = fs::read_dir(dir) {
        for entry in entries.flatten() {
            if entry.file_type().map(|t| t.is_dir()).unwrap_or(false) {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') {
                    dirs.push(name);
                }
            }
        }
    }
    dirs.sort();
    dirs
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── validate_knowledge_path ──────────────────────────────────────

    #[test]
    fn valid_team_knowledge_path() {
        assert!(validate_knowledge_path("knowledge/commit-convention.md").is_ok());
    }

    #[test]
    fn valid_team_invariant_path() {
        assert!(validate_knowledge_path("invariants/code-review.md").is_ok());
    }

    #[test]
    fn valid_project_knowledge_path() {
        assert!(validate_knowledge_path("projects/my-project/knowledge/api.md").is_ok());
    }

    #[test]
    fn valid_project_invariant_path() {
        assert!(validate_knowledge_path("projects/my-project/invariants/test.md").is_ok());
    }

    #[test]
    fn valid_member_knowledge_path() {
        assert!(validate_knowledge_path("team/architect-alice/knowledge/patterns.md").is_ok());
    }

    #[test]
    fn valid_member_invariant_path() {
        assert!(validate_knowledge_path("team/architect-alice/invariants/quality.md").is_ok());
    }

    #[test]
    fn valid_member_project_knowledge_path() {
        assert!(validate_knowledge_path(
            "team/architect-alice/projects/my-project/knowledge/notes.md"
        )
        .is_ok());
    }

    #[test]
    fn invalid_path_botminter_yml() {
        let result = validate_knowledge_path("botminter.yml");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("not within a knowledge or invariant directory"));
    }

    #[test]
    fn invalid_path_random_file() {
        let result = validate_knowledge_path("PROCESS.md");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_path_member_no_scope() {
        let result = validate_knowledge_path("team/architect-alice/PROMPT.md");
        assert!(result.is_err());
    }

    #[test]
    fn invalid_path_project_no_scope() {
        let result = validate_knowledge_path("projects/my-project/README.md");
        assert!(result.is_err());
    }
}
