use std::fs;
use std::path::Path;

use anyhow::{Context, Result};

use crate::config::{self, Credentials, TeamEntry};
use crate::git;
use crate::profile;

/// Sets up a new team repo: git init, extract profile, augment with projects,
/// record bridge, create member dirs, create project dirs, and initial commit.
///
/// `profiles_base` is the directory containing extracted profile directories.
/// If `None`, uses the default system profiles directory (`~/.config/botminter/profiles`).
pub fn setup_new_team_repo(
    team_repo: &Path,
    selected_profile: &str,
    manifest: &profile::ProfileManifest,
    members: &[(String, String)],
    projects: &[(String, String)],
    bridge: Option<&str>,
    profiles_base: Option<&Path>,
) -> Result<()> {
    fs::create_dir_all(team_repo).context("Failed to create team repo directory")?;
    git::run_git(team_repo, &["init", "-b", "main"])?;

    let coding_agent = manifest
        .coding_agents
        .get(&manifest.default_coding_agent)
        .with_context(|| {
            format!(
                "Profile '{}' default coding agent '{}' not found in coding_agents map",
                selected_profile, manifest.default_coding_agent
            )
        })?;

    if let Some(base) = profiles_base {
        profile::extract_profile_from(base, selected_profile, team_repo, coding_agent)?;
    } else {
        profile::extract_profile_to(selected_profile, team_repo, coding_agent)?;
    }

    if !projects.is_empty() {
        profile::augment_manifest_with_projects(team_repo, projects)?;
    }

    if let Some(bridge_name) = bridge {
        profile::record_bridge_in_manifest(team_repo, bridge_name, &manifest.bridges)?;
    }

    fs::create_dir_all(team_repo.join("members")).context("Failed to create members/ dir")?;
    fs::create_dir_all(team_repo.join("projects")).context("Failed to create projects/ dir")?;
    fs::write(team_repo.join("members/.gitkeep"), "").ok();
    fs::write(team_repo.join("projects/.gitkeep"), "").ok();

    for (role, name) in members {
        let member_dir_name = format!("{}-{}", role, name);
        let member_dir = team_repo.join("members").join(&member_dir_name);
        fs::create_dir_all(&member_dir)
            .with_context(|| format!("Failed to create member dir {}", member_dir.display()))?;
        if let Some(base) = profiles_base {
            profile::extract_member_from(base, selected_profile, role, &member_dir, coding_agent)?;
        } else {
            profile::extract_member_to(selected_profile, role, &member_dir, coding_agent)?;
        }
        profile::finalize_member_manifest(&member_dir, name)?;
        profile::render_member_placeholders(&member_dir, &member_dir_name, role, name)?;
    }

    for (proj_name, _url) in projects {
        let proj_dir = team_repo.join("projects").join(proj_name);
        fs::create_dir_all(proj_dir.join("knowledge"))
            .with_context(|| format!("Failed to create projects/{}/knowledge/", proj_name))?;
        fs::create_dir_all(proj_dir.join("invariants"))
            .with_context(|| format!("Failed to create projects/{}/invariants/", proj_name))?;
        fs::write(proj_dir.join("knowledge/.gitkeep"), "").ok();
        fs::write(proj_dir.join("invariants/.gitkeep"), "").ok();
    }

    git::run_git(team_repo, &["add", "-A"])?;
    let commit_msg = format!("feat: initialize team repo ({} profile)", selected_profile);
    git::run_git(team_repo, &["commit", "-m", &commit_msg])?;

    Ok(())
}

/// Registers a new team in the botminter config.
pub fn register_team(
    team_name: &str,
    team_dir: &Path,
    profile_name: &str,
    github_repo: &str,
    workzone: &Path,
) -> Result<()> {
    let mut cfg = config::load_or_default();

    let team_entry = TeamEntry {
        name: team_name.to_string(),
        path: team_dir.to_path_buf(),
        profile: profile_name.to_string(),
        github_repo: github_repo.to_string(),
        credentials: Credentials::default(),
        coding_agent: None,
        project_number: None,
        bridge_lifecycle: Default::default(),
        vm: None,
    };
    cfg.teams.push(team_entry);

    if cfg.teams.len() == 1 {
        cfg.default_team = Some(team_name.to_string());
    }
    cfg.workzone = workzone.to_path_buf();

    config::save(&cfg)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn setup_new_team_repo_creates_structure() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path().join("team");

        // Extract embedded profiles to a temp dir (test-path-isolation compliant)
        let profiles_tmp = tempfile::tempdir().unwrap();
        profile::embedded::extract_embedded_to_disk(profiles_tmp.path()).unwrap();
        let profiles = profile::list_profiles_from(profiles_tmp.path()).unwrap();
        let profile_name = &profiles[0];
        let manifest = profile::read_manifest_from(profile_name, profiles_tmp.path()).unwrap();

        setup_new_team_repo(
            &team_repo, profile_name, &manifest,
            &[], &[], None,
            Some(profiles_tmp.path()),
        ).unwrap();

        assert!(team_repo.join(".git").exists(), "Should have git repo");
        assert!(team_repo.join("members").exists(), "Should have members dir");
        assert!(team_repo.join("projects").exists(), "Should have projects dir");
        assert!(team_repo.join("botminter.yml").exists(), "Should have manifest");
    }

    #[test]
    fn setup_new_team_repo_with_bridge() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path().join("team");

        let profiles_tmp = tempfile::tempdir().unwrap();
        profile::embedded::extract_embedded_to_disk(profiles_tmp.path()).unwrap();
        let profiles = profile::list_profiles_from(profiles_tmp.path()).unwrap();
        let profile_name = &profiles[0];
        let manifest = profile::read_manifest_from(profile_name, profiles_tmp.path()).unwrap();

        if !manifest.bridges.is_empty() {
            let bridge_name = &manifest.bridges[0].name;
            setup_new_team_repo(
                &team_repo, profile_name, &manifest,
                &[], &[], Some(bridge_name),
                Some(profiles_tmp.path()),
            ).unwrap();

            let contents = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
            assert!(contents.contains(&format!("bridge: {}", bridge_name)));
        }
    }

    #[test]
    fn setup_new_team_repo_with_projects() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path().join("team");

        let profiles_tmp = tempfile::tempdir().unwrap();
        profile::embedded::extract_embedded_to_disk(profiles_tmp.path()).unwrap();
        let profiles = profile::list_profiles_from(profiles_tmp.path()).unwrap();
        let profile_name = &profiles[0];
        let manifest = profile::read_manifest_from(profile_name, profiles_tmp.path()).unwrap();

        let projects = vec![("my-app".to_string(), "https://github.com/org/my-app.git".to_string())];
        setup_new_team_repo(
            &team_repo, profile_name, &manifest,
            &[], &projects, None,
            Some(profiles_tmp.path()),
        ).unwrap();

        assert!(team_repo.join("projects/my-app/knowledge").exists());
        assert!(team_repo.join("projects/my-app/invariants").exists());
    }

    #[test]
    fn setup_new_team_repo_renders_member_placeholders() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path().join("team");

        // Set up git config for the test environment
        let gitconfig = tmp.path().join(".gitconfig");
        fs::write(&gitconfig, "[user]\n\tname = Test\n\temail = test@test.com\n").unwrap();
        let orig_home = std::env::var("HOME").ok();
        std::env::set_var("HOME", tmp.path());

        let profiles_tmp = tempfile::tempdir().unwrap();
        profile::embedded::extract_embedded_to_disk(profiles_tmp.path()).unwrap();
        let manifest = profile::read_manifest_from("agentic-sdlc-minimal", profiles_tmp.path()).unwrap();

        let members = vec![("engineer".to_string(), "bob".to_string())];
        setup_new_team_repo(
            &team_repo, "agentic-sdlc-minimal", &manifest,
            &members, &[], None,
            Some(profiles_tmp.path()),
        ).unwrap();

        let ralph_yml = team_repo.join("members/engineer-bob/ralph.yml");
        assert!(ralph_yml.exists(), "Member ralph.yml should exist");

        let content = fs::read_to_string(&ralph_yml).unwrap();
        assert!(
            !content.contains("{{member_dir}}"),
            "ralph.yml should not contain unreplaced {{{{member_dir}}}} placeholders"
        );
        assert!(
            content.contains("engineer-bob"),
            "ralph.yml should contain the rendered member dir name 'engineer-bob'"
        );

        // Restore HOME
        match orig_home {
            Some(h) => std::env::set_var("HOME", h),
            None => std::env::remove_var("HOME"),
        }
    }

    #[test]
    fn register_team_creates_config() {
        let tmp = tempfile::tempdir().unwrap();
        let config_path = tmp.path().join(".botminter").join("config.yml");

        // Temporarily point config to our test dir
        let team_dir = tmp.path().join("my-team");
        fs::create_dir_all(&team_dir).unwrap();

        // We need to save to a specific path, so use config::save_to directly
        let mut cfg = config::BotminterConfig {
            workzone: tmp.path().to_path_buf(),
            default_team: None,
            teams: Vec::new(),
            vms: Vec::new(),
            keyring_collection: None,
        };

        cfg.teams.push(TeamEntry {
            name: "my-team".to_string(),
            path: team_dir.clone(),
            profile: "scrum".to_string(),
            github_repo: "org/repo".to_string(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
        vm: None,
        });
        cfg.default_team = Some("my-team".to_string());

        config::save_to(&config_path, &cfg).unwrap();
        let loaded = config::load_from(&config_path).unwrap();
        assert_eq!(loaded.teams.len(), 1);
        assert_eq!(loaded.teams[0].name, "my-team");
        assert_eq!(loaded.default_team, Some("my-team".to_string()));
    }
}
