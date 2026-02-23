use std::fs;

use anyhow::{bail, Context, Result};

use crate::config;
use crate::profile;

use super::init::{derive_project_name, find_project_number, run_git, sync_project_status_field};

/// Handles `bm projects add <url> [-t team]`.
pub fn add(url: &str, team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Read team repo's botminter.yml
    let manifest_path = team_repo.join("botminter.yml");
    let mut manifest: profile::ProfileManifest = {
        let contents = fs::read_to_string(&manifest_path)
            .context("Failed to read team repo's botminter.yml")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };

    // Derive project name from URL
    let name = derive_project_name(url);

    // Check for duplicates
    if manifest.projects.iter().any(|p| p.name == name) {
        bail!("Project '{}' already exists in this team.", name);
    }

    // Add project to manifest
    manifest.projects.push(profile::ProjectDef {
        name: name.clone(),
        fork_url: url.to_string(),
    });

    let contents =
        serde_yml::to_string(&manifest).context("Failed to serialize botminter.yml")?;
    fs::write(&manifest_path, contents).context("Failed to write botminter.yml")?;

    // Create project dirs with .gitkeep
    let proj_dir = team_repo.join("projects").join(&name);
    fs::create_dir_all(proj_dir.join("knowledge"))
        .with_context(|| format!("Failed to create projects/{}/knowledge/", name))?;
    fs::create_dir_all(proj_dir.join("invariants"))
        .with_context(|| format!("Failed to create projects/{}/invariants/", name))?;
    fs::write(proj_dir.join("knowledge/.gitkeep"), "").ok();
    fs::write(proj_dir.join("invariants/.gitkeep"), "").ok();

    // Git add + commit (no auto-push)
    run_git(
        &team_repo,
        &["add", "botminter.yml", &format!("projects/{}/", name)],
    )?;
    let commit_msg = format!("feat: add project {}", name);
    run_git(&team_repo, &["commit", "-m", &commit_msg])?;

    println!("Added project '{}' to team '{}'.", name, team.name);

    Ok(())
}

/// Handles `bm projects sync [-t team]`.
/// Syncs the GitHub Project board's Status field options with the profile,
/// then prints instructions for setting up role-based views.
pub fn sync(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Read the team's manifest
    let manifest_path = team_repo.join("botminter.yml");
    let manifest: profile::ProfileManifest = {
        let contents = fs::read_to_string(&manifest_path)
            .context("Failed to read team repo's botminter.yml")?;
        serde_yml::from_str(&contents).context("Failed to parse botminter.yml")?
    };

    let owner = team
        .github_repo
        .split('/')
        .next()
        .unwrap_or(&team.github_repo);
    let gh_token = team.credentials.gh_token.as_deref();

    // Find the project board
    let project_number = find_project_number(owner, &team.name, gh_token)?;

    // Sync Status field options
    sync_project_status_field(owner, project_number, &manifest.statuses, gh_token)?;
    println!(
        "✓ Status field synced ({} options)",
        manifest.statuses.len()
    );

    // Print view setup instructions
    if manifest.views.is_empty() {
        println!("\nNo views defined in the profile.");
        return Ok(());
    }

    let project_url = format!(
        "https://github.com/orgs/{}/projects/{}",
        owner, project_number
    );
    println!();
    println!("Your GitHub Project board needs role-based views so each role sees");
    println!("only its relevant statuses. Create one view per role listed below.");
    println!();
    println!("Open the board: {}", project_url);
    println!();
    println!("For each view:");
    println!("  1. Click \"+\" next to the existing view tabs");
    println!("  2. Choose \"Board\" layout");
    println!("  3. Rename the tab to the view name below");
    println!("  4. Click the filter bar and paste the filter string");
    println!("  5. Click save");
    println!("  6. To create the next view, click the tab dropdown → Duplicate view, then repeat from step 3");
    println!();

    // Calculate column widths
    let name_width = manifest
        .views
        .iter()
        .map(|v| v.name.len())
        .max()
        .unwrap_or(4)
        .max(4);

    println!(
        "  {:<width$}  Filter",
        "View",
        width = name_width
    );
    println!(
        "  {:<width$}  ------",
        "----",
        width = name_width
    );

    for view in &manifest.views {
        let filter = view.filter_string(&manifest.statuses);
        println!(
            "  {:<width$}  {}",
            view.name,
            filter,
            width = name_width
        );
    }

    println!();

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::super::init::derive_project_name;

    #[test]
    fn derive_project_name_git_url() {
        assert_eq!(
            derive_project_name("git@github.com:org/my-repo.git"),
            "my-repo"
        );
    }

    #[test]
    fn derive_project_name_https() {
        assert_eq!(
            derive_project_name("https://github.com/org/my-repo.git"),
            "my-repo"
        );
    }

    #[test]
    fn derive_project_name_trailing_slash() {
        assert_eq!(
            derive_project_name("https://github.com/org/my-repo/"),
            "my-repo"
        );
    }

    #[test]
    fn derive_project_name_no_git_suffix() {
        assert_eq!(
            derive_project_name("https://github.com/org/my-repo"),
            "my-repo"
        );
    }
}
