//! E2E tests for the init → hire → projects add → sync lifecycle.
//!
//! These tests create real GitHub repos under the `devguyio-bot-squad` org
//! and verify that the full `bm` CLI pipeline produces correct workspaces,
//! labels, and member listings.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;

use bm::config::{BotminterConfig, Credentials, TeamEntry};
use bm::profile;

use super::helpers::{assert_cmd_success, bm_cmd};

/// GitHub org with repo-creation permissions for the E2E test token.
const E2E_ORG: &str = "devguyio-bot-squad";

/// Skip the test if `gh auth status` fails.
macro_rules! require_gh_auth {
    () => {
        if !super::github::gh_auth_ok() {
            eprintln!("SKIP: gh auth not available — skipping test");
            return;
        }
    };
}

// ── Helpers ──────────────────────────────────────────────────────────

/// Runs a git command in a directory.
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {} failed to run: {}", args.join(" "), e));
    assert!(
        output.status.success(),
        "git {} in {} failed: {}",
        args.join(" "),
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Pushes to GitHub using `gh auth git-credential` as the credential helper.
///
/// The default system credential helper (e.g. libsecret) may not work in
/// non-interactive environments. Using gh's credential helper ensures
/// authentication works when `gh auth status` passes.
fn git_push(dir: &Path) {
    let output = Command::new("git")
        .args([
            "-c",
            "credential.helper=",
            "-c",
            "credential.helper=!gh auth git-credential",
            "push",
            "-u",
            "origin",
            "main",
        ])
        .current_dir(dir)
        .output()
        .expect("failed to run git push");
    assert!(
        output.status.success(),
        "git push in {} failed: {}",
        dir.display(),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Finds a profile with at least `min_roles` roles.
/// Returns (profile_name, roles_vec).
fn find_profile_with_roles(min_roles: usize) -> (String, Vec<String>) {
    for name in profile::list_profiles() {
        if let Ok(roles) = profile::list_roles(&name) {
            if roles.len() >= min_roles {
                return (name, roles);
            }
        }
    }
    panic!("No embedded profile has at least {} roles", min_roles);
}

/// Sets up a team repo programmatically and pushes it to a real GitHub repo.
///
/// Mimics what `bm init` does, but without the interactive wizard:
/// 1. Extracts profile content
/// 2. Creates team/ and projects/ dirs
/// 3. Git init + commit
/// 4. Adds GitHub remote + pushes
/// 5. Writes config file
///
/// Does NOT set HOME — caller must pass HOME to subprocess `bm` commands.
fn setup_team_with_github(
    tmp: &Path,
    team_name: &str,
    profile_name: &str,
    github_full_name: &str,
) -> PathBuf {
    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();

    // Git init with local config
    git(&team_repo, &["init", "-b", "main"]);
    git(
        &team_repo,
        &["config", "user.email", "e2e@botminter.test"],
    );
    git(&team_repo, &["config", "user.name", "BM E2E"]);

    // Extract profile content into team repo
    profile::extract_profile_to(profile_name, &team_repo).unwrap();

    // Create team/ and projects/ dirs (as bm init does)
    fs::create_dir_all(team_repo.join("team")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("team/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();

    // Initial commit
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    // Push to GitHub (use gh credential helper since libsecret may not work)
    let remote_url = format!("https://github.com/{}.git", github_full_name);
    git(&team_repo, &["remote", "add", "origin", &remote_url]);
    git_push(&team_repo);

    // Write config
    let config = BotminterConfig {
        workzone,
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: profile_name.to_string(),
            github_repo: github_full_name.to_string(),
            credentials: Credentials::default(),
        }],
    };
    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    team_repo
}

/// Bootstraps labels on GitHub from the profile manifest.
fn bootstrap_labels(repo: &str, profile_name: &str) {
    let manifest = profile::read_manifest(profile_name).unwrap();
    for label in &manifest.labels {
        let output = Command::new("gh")
            .args([
                "label",
                "create",
                &label.name,
                "--color",
                &label.color,
                "--description",
                &label.description,
                "--force",
                "--repo",
                repo,
            ])
            .output()
            .unwrap_or_else(|e| panic!("failed to create label '{}': {}", label.name, e));
        if !output.status.success() {
            eprintln!(
                "Warning: failed to create label '{}': {}",
                label.name,
                String::from_utf8_lossy(&output.stderr)
            );
        }
    }
}

/// Creates a local git repo for use as a project fork URL.
fn create_fake_fork(tmp: &Path, name: &str) -> PathBuf {
    let fork = tmp.join(name);
    fs::create_dir_all(&fork).unwrap();
    git(&fork, &["init", "-b", "main"]);
    git(&fork, &["config", "user.email", "e2e@botminter.test"]);
    git(&fork, &["config", "user.name", "BM E2E"]);
    fs::write(fork.join("README.md"), format!("# {}", name)).unwrap();
    git(&fork, &["add", "-A"]);
    git(&fork, &["commit", "-m", "init"]);
    fork
}

// ── Tests ────────────────────────────────────────────────────────────

/// Full lifecycle: init → hire × 2 → projects add → sync.
/// Verifies workspace dirs, symlinks, `.botminter/`, `.claude/agents/`, `ralph.yml`.
#[test]
fn e2e_init_hire_sync_lifecycle() {
    require_gh_auth!();

    let repo = super::github::TempRepo::new_in_org("bm-e2e-lifecycle", E2E_ORG)
        .expect("Failed to create temp GitHub repo under devguyio-bot-squad");
    let tmp = tempfile::tempdir().unwrap();

    // Find a profile with at least 2 roles (profile-agnostic)
    let (profile_name, roles) = find_profile_with_roles(2);
    let role_1 = &roles[0];
    let role_2 = &roles[1];
    let team_name = "e2e-lifecycle";

    // Programmatic team setup with GitHub remote
    setup_team_with_github(tmp.path(), team_name, &profile_name, &repo.full_name);
    bootstrap_labels(&repo.full_name, &profile_name);

    // Create fake fork for project
    let fork = create_fake_fork(tmp.path(), "test-project");

    // ── CLI operations ───────────────────────────────────────────────

    // Hire alice (role_1)
    let mut cmd = bm_cmd();
    cmd.args(["hire", role_1, "--name", "alice", "-t", team_name])
        .env("HOME", tmp.path());
    let out = assert_cmd_success(&mut cmd);
    eprintln!("hire alice: {}", out.trim());

    // Hire bob (role_2)
    let mut cmd = bm_cmd();
    cmd.args(["hire", role_2, "--name", "bob", "-t", team_name])
        .env("HOME", tmp.path());
    let out = assert_cmd_success(&mut cmd);
    eprintln!("hire bob: {}", out.trim());

    // Add project
    let mut cmd = bm_cmd();
    cmd.args([
        "projects",
        "add",
        &fork.to_string_lossy(),
        "-t",
        team_name,
    ])
    .env("HOME", tmp.path());
    let out = assert_cmd_success(&mut cmd);
    eprintln!("projects add: {}", out.trim());

    // Sync workspaces
    let mut cmd = bm_cmd();
    cmd.args(["teams", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    let out = assert_cmd_success(&mut cmd);
    eprintln!("teams sync: {}", out.trim());

    // ── Verify workspace structure ───────────────────────────────────

    let team_dir = tmp.path().join("workspaces").join(team_name);
    let alice_dir = format!("{}-alice", role_1);
    let bob_dir = format!("{}-bob", role_2);

    for member_name in [&alice_dir, &bob_dir] {
        // With a project, workspace is at: {team_dir}/{member}/{project}/
        let ws = team_dir.join(member_name).join("test-project");

        assert!(
            ws.join(".botminter").is_dir(),
            "{}/test-project should have .botminter/",
            member_name
        );

        // Symlinks: PROMPT.md, CLAUDE.md
        for file in ["PROMPT.md", "CLAUDE.md"] {
            assert!(
                ws.join(file).exists(),
                "{}/test-project should have {}",
                member_name,
                file
            );
            assert!(
                ws.join(file)
                    .symlink_metadata()
                    .unwrap()
                    .file_type()
                    .is_symlink(),
                "{}/test-project/{} should be a symlink",
                member_name,
                file
            );
        }

        // ralph.yml: copied (NOT symlink)
        assert!(
            ws.join("ralph.yml").exists(),
            "{}/test-project should have ralph.yml",
            member_name
        );
        assert!(
            !ws.join("ralph.yml")
                .symlink_metadata()
                .unwrap()
                .file_type()
                .is_symlink(),
            "{}/test-project/ralph.yml should be a copy, not symlink",
            member_name
        );

        // .claude/agents/ directory exists
        assert!(
            ws.join(".claude/agents").is_dir(),
            "{}/test-project should have .claude/agents/",
            member_name
        );
    }
}

/// Verifies labels bootstrapped on GitHub match the profile manifest exactly.
#[test]
fn e2e_labels_bootstrapped_on_github() {
    require_gh_auth!();

    let repo = super::github::TempRepo::new_in_org("bm-e2e-labels", E2E_ORG)
        .expect("Failed to create temp GitHub repo");
    let tmp = tempfile::tempdir().unwrap();

    let (profile_name, _) = find_profile_with_roles(1);
    let team_name = "e2e-labels";

    setup_team_with_github(tmp.path(), team_name, &profile_name, &repo.full_name);
    bootstrap_labels(&repo.full_name, &profile_name);

    // Read expected labels from profile manifest
    let manifest = profile::read_manifest(&profile_name).unwrap();

    // Query actual labels on GitHub
    let gh_labels = super::github::list_labels_json(&repo.full_name);

    // Verify every expected label exists with correct color
    for expected in &manifest.labels {
        let found = gh_labels.iter().find(|(name, _)| name == &expected.name);
        assert!(
            found.is_some(),
            "Label '{}' from profile manifest not found on GitHub. GitHub has: {:?}",
            expected.name,
            gh_labels
                .iter()
                .map(|(n, _)| n.as_str())
                .collect::<Vec<_>>()
        );
        let (_, gh_color) = found.unwrap();
        // Colors may have '#' prefix or not — normalize for comparison
        let norm_expected = expected.color.trim_start_matches('#').to_lowercase();
        let norm_actual = gh_color.trim_start_matches('#').to_lowercase();
        assert_eq!(
            norm_expected, norm_actual,
            "Label '{}' color mismatch: expected '{}', got '{}'",
            expected.name, expected.color, gh_color
        );
    }

    // Verify no unexpected labels (beyond GitHub defaults)
    let github_defaults: &[&str] = &[
        "bug",
        "documentation",
        "duplicate",
        "enhancement",
        "good first issue",
        "help wanted",
        "invalid",
        "question",
        "wontfix",
    ];
    for (name, _) in &gh_labels {
        let is_expected = manifest.labels.iter().any(|l| l.name == *name);
        let is_default = github_defaults.contains(&name.as_str());
        assert!(
            is_expected || is_default,
            "Unexpected label '{}' on GitHub — not in profile manifest or GitHub defaults",
            name
        );
    }
}

/// Verifies that running `bm teams sync` twice succeeds without errors
/// and the workspace structure remains correct.
#[test]
fn e2e_sync_idempotent_with_github() {
    require_gh_auth!();

    let repo = super::github::TempRepo::new_in_org("bm-e2e-idem", E2E_ORG)
        .expect("Failed to create temp GitHub repo");
    let tmp = tempfile::tempdir().unwrap();

    let (profile_name, roles) = find_profile_with_roles(1);
    let role = &roles[0];
    let team_name = "e2e-idem";

    setup_team_with_github(tmp.path(), team_name, &profile_name, &repo.full_name);

    // Hire one member
    let mut cmd = bm_cmd();
    cmd.args(["hire", role, "--name", "alice", "-t", team_name])
        .env("HOME", tmp.path());
    assert_cmd_success(&mut cmd);

    // First sync
    let mut cmd = bm_cmd();
    cmd.args(["teams", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    let out1 = assert_cmd_success(&mut cmd);
    eprintln!("sync 1: {}", out1.trim());

    // Second sync — should succeed without errors
    let mut cmd = bm_cmd();
    cmd.args(["teams", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    let out2 = assert_cmd_success(&mut cmd);
    eprintln!("sync 2: {}", out2.trim());

    // Verify workspace structure unchanged after double sync
    let team_dir = tmp.path().join("workspaces").join(team_name);
    let member = format!("{}-alice", role);
    let ws = team_dir.join(&member);

    assert!(
        ws.join(".botminter").is_dir(),
        "workspace should have .botminter/ after double sync"
    );
    assert!(
        ws.join("PROMPT.md").exists(),
        "workspace should have PROMPT.md after double sync"
    );
    assert!(
        ws.join("CLAUDE.md").exists(),
        "workspace should have CLAUDE.md after double sync"
    );
    assert!(
        ws.join("ralph.yml").exists(),
        "workspace should have ralph.yml after double sync"
    );
    assert!(
        ws.join(".claude").is_dir(),
        "workspace should have .claude/ after double sync"
    );
}

/// After hiring 2 members and syncing, `bm members list` shows them
/// with correct roles.
#[test]
fn e2e_members_list_after_full_setup() {
    require_gh_auth!();

    let repo = super::github::TempRepo::new_in_org("bm-e2e-members", E2E_ORG)
        .expect("Failed to create temp GitHub repo");
    let tmp = tempfile::tempdir().unwrap();

    let (profile_name, roles) = find_profile_with_roles(2);
    let role_1 = &roles[0];
    let role_2 = &roles[1];
    let team_name = "e2e-members";

    setup_team_with_github(tmp.path(), team_name, &profile_name, &repo.full_name);

    // Hire two members
    let mut cmd = bm_cmd();
    cmd.args(["hire", role_1, "--name", "alice", "-t", team_name])
        .env("HOME", tmp.path());
    assert_cmd_success(&mut cmd);

    let mut cmd = bm_cmd();
    cmd.args(["hire", role_2, "--name", "bob", "-t", team_name])
        .env("HOME", tmp.path());
    assert_cmd_success(&mut cmd);

    // Sync
    let mut cmd = bm_cmd();
    cmd.args(["teams", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    assert_cmd_success(&mut cmd);

    // Run members list
    let mut cmd = bm_cmd();
    cmd.args(["members", "list", "-t", team_name])
        .env("HOME", tmp.path());
    let stdout = assert_cmd_success(&mut cmd);

    // Verify both members appear
    let alice = format!("{}-alice", role_1);
    let bob = format!("{}-bob", role_2);

    assert!(
        stdout.contains(&alice),
        "members list should show '{}', output:\n{}",
        alice,
        stdout
    );
    assert!(
        stdout.contains(&bob),
        "members list should show '{}', output:\n{}",
        bob,
        stdout
    );

    // Verify roles appear
    assert!(
        stdout.contains(role_1.as_str()),
        "members list should show role '{}', output:\n{}",
        role_1,
        stdout
    );
    assert!(
        stdout.contains(role_2.as_str()),
        "members list should show role '{}', output:\n{}",
        role_2,
        stdout
    );
}

/// `bm teams list` output includes the GitHub repo URL/name.
#[test]
fn e2e_teams_list_shows_github_repo() {
    require_gh_auth!();

    let repo = super::github::TempRepo::new_in_org("bm-e2e-teams", E2E_ORG)
        .expect("Failed to create temp GitHub repo");
    let tmp = tempfile::tempdir().unwrap();

    let (profile_name, _) = find_profile_with_roles(1);
    let team_name = "e2e-teams";

    setup_team_with_github(tmp.path(), team_name, &profile_name, &repo.full_name);

    // Run teams list
    let mut cmd = bm_cmd();
    cmd.args(["teams", "list"]).env("HOME", tmp.path());
    let stdout = assert_cmd_success(&mut cmd);

    // Verify output includes the GitHub repo full name
    assert!(
        stdout.contains(&repo.full_name),
        "teams list should show GitHub repo '{}', output:\n{}",
        repo.full_name,
        stdout
    );
}

/// E2E: `bm projects sync` creates a GitHub Project, syncs Status field options
/// via GraphQL, and prints view instructions.
///
/// Uses a local-only team repo setup (no git push needed) with a real GitHub
/// Project for the API calls.
#[test]
fn e2e_projects_sync_status_and_views() {
    require_gh_auth!();

    let tmp = tempfile::tempdir().unwrap();
    let team_name = "e2e-project-sync";
    let github_repo = format!("{}/test-team-repo", E2E_ORG);

    // Set up team locally (no push — projects sync doesn't need remote content)
    let workzone = tmp.path().join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");
    fs::create_dir_all(&team_repo).unwrap();

    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "e2e@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM E2E"]);
    profile::extract_profile_to("scrum-compact", &team_repo).unwrap();
    fs::create_dir_all(team_repo.join("team")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("team/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    let config = BotminterConfig {
        workzone,
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: "scrum-compact".to_string(),
            github_repo: github_repo.clone(),
            credentials: Credentials::default(),
        }],
    };
    let config_path = tmp.path().join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    // Create a GitHub Project for this test (RAII cleanup)
    let project = super::github::TempProject::new(E2E_ORG, &format!("{} Board", team_name))
        .expect("Failed to create temp GitHub Project");

    // ── Run bm projects sync ─────────────────────────────────────────
    let mut cmd = bm_cmd();
    cmd.args(["projects", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    let stdout = assert_cmd_success(&mut cmd);

    // ── Verify Status field was synced ────────────────────────────────
    let options = super::github::list_project_status_options(E2E_ORG, project.number);
    assert!(
        options.len() >= 20,
        "Status field should have at least 20 options after sync, got {}: {:?}",
        options.len(),
        options
    );
    assert!(
        options.contains(&"po:triage".to_string()),
        "Status field should contain 'po:triage', got: {:?}",
        options
    );
    assert!(
        options.contains(&"done".to_string()),
        "Status field should contain 'done', got: {:?}",
        options
    );
    assert!(
        options.contains(&"error".to_string()),
        "Status field should contain 'error', got: {:?}",
        options
    );

    // ── Verify output format ─────────────────────────────────────────
    assert!(
        stdout.contains("Status field synced"),
        "stdout should confirm sync, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("View"),
        "stdout should show view table header, got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Filter"),
        "stdout should show filter column header, got:\n{}",
        stdout
    );
    // Verify at least one view with a filter string
    assert!(
        stdout.contains("status:po:"),
        "stdout should contain a PO filter like 'status:po:', got:\n{}",
        stdout
    );
    assert!(
        stdout.contains("status:arch:"),
        "stdout should contain an Architect filter like 'status:arch:', got:\n{}",
        stdout
    );

    // ── Verify idempotency (re-run should succeed) ───────────────────
    let mut cmd = bm_cmd();
    cmd.args(["projects", "sync", "-t", team_name])
        .env("HOME", tmp.path());
    let stdout2 = assert_cmd_success(&mut cmd);
    assert!(
        stdout2.contains("Status field synced"),
        "second sync should also succeed, got:\n{}",
        stdout2
    );

    // TempProject drops here → deletes the project
}
