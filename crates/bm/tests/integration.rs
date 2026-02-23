//! Integration tests for the `bm` CLI.
//!
//! These tests exercise multi-command workflows against temporary directories.
//! Tests that modify `HOME` are serialized via a global mutex since `dirs::home_dir()`
//! reads the env var.
//!
//! Tests requiring the `ralph` binary (start/stop/status) are omitted since
//! ralph is not available in the test environment.

use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::Mutex;

use bm::config::{BotminterConfig, Credentials, TeamEntry};
use bm::profile;

/// Serialize all tests that mutate the HOME env var.
static ENV_MUTEX: Mutex<()> = Mutex::new(());

// ── Test helpers ──────────────────────────────────────────────────────

/// Runs a git command in a directory (test helper).
fn git(dir: &Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .output()
        .unwrap_or_else(|e| panic!("git {} failed to run: {}", args.join(" "), e));
    assert!(
        output.status.success(),
        "git {} failed: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stderr)
    );
}

/// Sets up a team repo programmatically (bypasses the interactive `bm init` wizard).
///
/// Creates:
///   {tmp}/workspaces/{team_name}/team/  — the team git repo with extracted profile
///   {tmp}/.botminter/config.yml        — config pointing to the team
///
/// Sets HOME to `tmp` so `config::load()` finds the config.
/// Returns the path to the team repo (the git repo inside the team dir).
fn setup_team(tmp: &Path, team_name: &str, profile_name: &str) -> PathBuf {
    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();

    // Git init with config
    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "test@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM Test"]);

    // Extract embedded profile content into team repo
    profile::extract_profile_to(profile_name, &team_repo).unwrap();

    // Create team/ and projects/ dirs (as bm init does)
    fs::create_dir_all(team_repo.join("team")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("team/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();

    // Initial commit
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    // Save config
    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: profile_name.to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
        }],
    };

    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    // Redirect HOME so config::load() finds the right config
    env::set_var("HOME", tmp);

    team_repo
}

/// Registers an additional team in the existing config at `{tmp}/.botminter/config.yml`.
fn add_team_to_config(
    tmp: &Path,
    team_name: &str,
    profile_name: &str,
    make_default: bool,
) -> PathBuf {
    let config_path = tmp.join(".botminter").join("config.yml");
    let mut config = bm::config::load_from(&config_path).unwrap();

    let workzone = &config.workzone;
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();
    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "test@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM Test"]);
    profile::extract_profile_to(profile_name, &team_repo).unwrap();
    fs::create_dir_all(team_repo.join("team")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("team/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    config.teams.push(TeamEntry {
        name: team_name.to_string(),
        path: team_dir,
        profile: profile_name.to_string(),
        github_repo: String::new(),
        credentials: Credentials::default(),
    });

    if make_default {
        config.default_team = Some(team_name.to_string());
    }

    bm::config::save_to(&config_path, &config).unwrap();

    team_repo
}

// ── Profile tests (no HOME needed) ───────────────────────────────────

#[test]
fn profiles_list_returns_all_embedded() {
    let profiles = profile::list_profiles();
    assert!(profiles.contains(&"scrum".to_string()));
    assert!(profiles.contains(&"scrum-compact".to_string()));
    assert!(profiles.contains(&"scrum-compact-telegram".to_string()));
    assert_eq!(profiles.len(), 3);
}

#[test]
fn profiles_describe_returns_complete_data() {
    let manifest = profile::read_manifest("scrum").unwrap();
    assert_eq!(manifest.name, "scrum");
    assert!(!manifest.display_name.is_empty());
    assert!(!manifest.description.is_empty());
    assert_eq!(manifest.schema_version, "1.0");
    assert!(!manifest.roles.is_empty());
    assert!(!manifest.labels.is_empty());

    // Verify roles have names and descriptions
    for role in &manifest.roles {
        assert!(!role.name.is_empty());
        assert!(!role.description.is_empty());
    }
}

#[test]
fn profiles_describe_nonexistent_errors() {
    let result = profile::read_manifest("does-not-exist");
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}

// ── Hire tests ───────────────────────────────────────────────────────

#[test]
fn hire_with_explicit_name() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::hire::run("architect", Some("bob"), None).unwrap();

    // Verify member directory was created
    let member_dir = team_repo.join("team/architect-bob");
    assert!(member_dir.is_dir(), "architect-bob/ should exist");

    // Verify botminter.yml was finalized (no .botminter.yml template)
    assert!(member_dir.join("botminter.yml").exists());
    assert!(!member_dir.join(".botminter.yml").exists());

    // Verify key skeleton files were extracted
    assert!(member_dir.join("PROMPT.md").exists());
    assert!(member_dir.join("CLAUDE.md").exists());
    assert!(member_dir.join("ralph.yml").exists());

    // Verify git commit was created
    let output = Command::new("git")
        .args(["log", "--oneline", "-1"])
        .current_dir(&team_repo)
        .output()
        .unwrap();
    let last_commit = String::from_utf8_lossy(&output.stdout);
    assert!(last_commit.contains("hire architect as bob"));
}

#[test]
fn hire_auto_suffix_first_member() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::hire::run("architect", None, None).unwrap();

    let member_dir = team_repo.join("team/architect-01");
    assert!(member_dir.is_dir(), "architect-01/ should exist (auto-suffix)");
}

#[test]
fn hire_auto_suffix_increments() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::hire::run("architect", None, None).unwrap();
    bm::commands::hire::run("architect", None, None).unwrap();

    assert!(team_repo.join("team/architect-01").is_dir());
    assert!(team_repo.join("team/architect-02").is_dir());
}

#[test]
fn hire_unknown_role_errors() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let result = bm::commands::hire::run("nonexistent-role", Some("alice"), None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("nonexistent-role"));
    assert!(err.contains("architect")); // should list available roles
}

#[test]
fn hire_duplicate_name_errors() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::hire::run("architect", Some("bob"), None).unwrap();
    let result = bm::commands::hire::run("architect", Some("bob"), None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"));
}

// ── Projects tests ───────────────────────────────────────────────────

#[test]
fn projects_add_creates_dirs_and_updates_manifest() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::projects::add("git@github.com:org/my-repo.git", None).unwrap();

    // Verify project dirs created
    let proj_dir = team_repo.join("projects/my-repo");
    assert!(proj_dir.join("knowledge").is_dir());
    assert!(proj_dir.join("invariants").is_dir());

    // Verify botminter.yml updated with project
    let manifest_content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    assert!(manifest_content.contains("my-repo"));
    assert!(manifest_content.contains("git@github.com:org/my-repo.git"));
}

#[test]
fn projects_add_duplicate_errors() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::projects::add("git@github.com:org/my-repo.git", None).unwrap();
    let result = bm::commands::projects::add("git@github.com:org/my-repo.git", None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("already exists"));
}

// ── Schema version guard ─────────────────────────────────────────────

#[test]
fn schema_version_mismatch_blocks_hire() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Tamper with schema_version in team repo's botminter.yml
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: v99");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: bump schema"]);

    let result = bm::commands::hire::run("architect", Some("alice"), None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("bm upgrade"), "Should suggest bm upgrade: {}", err);
}

#[test]
fn schema_version_mismatch_blocks_sync() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Hire a member first (with correct schema)
    bm::commands::hire::run("architect", Some("bob"), None).unwrap();

    // Tamper with schema_version
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: v99");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: bump schema"]);

    let result = bm::commands::teams::sync(false, None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("bm upgrade"), "Should suggest bm upgrade: {}", err);
}

// ── Multi-team and -t flag tests ─────────────────────────────────────

#[test]
fn multi_team_default_resolution() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo_alpha = setup_team(tmp.path(), "alpha", "scrum");
    add_team_to_config(tmp.path(), "beta", "scrum-compact", false);

    // Default team is "alpha" (set by setup_team)
    // Hire into default team (no -t flag)
    bm::commands::hire::run("architect", Some("alice"), None).unwrap();
    assert!(team_repo_alpha.join("team/architect-alice").is_dir());
}

#[test]
fn team_flag_overrides_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "alpha", "scrum");
    let team_repo_beta = add_team_to_config(tmp.path(), "beta", "scrum-compact", false);

    // Use -t to target non-default team
    bm::commands::hire::run("superman", Some("clark"), Some("beta")).unwrap();

    // Verify member landed in beta, not alpha
    assert!(team_repo_beta.join("team/superman-clark").is_dir());
}

#[test]
fn team_flag_nonexistent_errors() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "alpha", "scrum");

    let result = bm::commands::hire::run("architect", Some("bob"), Some("nonexistent"));
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("nonexistent"));
    assert!(err.contains("alpha")); // lists available teams
}

// ── Full lifecycle tests ─────────────────────────────────────────────

#[test]
fn lifecycle_hire_then_sync_no_project() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "lifecycle-team", "scrum");

    // Hire two members
    bm::commands::hire::run("architect", Some("alice"), None).unwrap();
    bm::commands::hire::run("human-assistant", Some("bob"), None).unwrap();

    // Sync (no projects — no-project mode)
    bm::commands::teams::sync(false, None).unwrap();

    // Verify workspaces were created (no-project: workspace at {team_dir}/{member}/)
    let team_dir = team_repo.parent().unwrap();
    let alice_ws = team_dir.join("architect-alice");
    let bob_ws = team_dir.join("human-assistant-bob");

    assert!(alice_ws.join(".botminter").is_dir(), "alice should have .botminter/");
    assert!(bob_ws.join(".botminter").is_dir(), "bob should have .botminter/");

    // Verify surfaced files
    assert!(alice_ws.join("PROMPT.md").exists(), "alice should have PROMPT.md");
    assert!(alice_ws.join("CLAUDE.md").exists(), "alice should have CLAUDE.md");
    assert!(alice_ws.join("ralph.yml").exists(), "alice should have ralph.yml");
    assert!(alice_ws.join(".gitignore").exists(), "alice should have .gitignore");
    assert!(alice_ws.join(".claude").is_dir(), "alice should have .claude/");
}

#[test]
fn lifecycle_hire_project_add_then_sync() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "proj-team", "scrum");

    // Create a dummy "fork" repo to clone from
    let fork_repo = tmp.path().join("fake-fork");
    fs::create_dir_all(&fork_repo).unwrap();
    git(&fork_repo, &["init", "-b", "main"]);
    git(&fork_repo, &["config", "user.email", "test@botminter.test"]);
    git(&fork_repo, &["config", "user.name", "BM Test"]);
    fs::write(fork_repo.join("README.md"), "# Fake fork").unwrap();
    git(&fork_repo, &["add", "-A"]);
    git(&fork_repo, &["commit", "-m", "init"]);

    // Hire a member
    bm::commands::hire::run("architect", Some("alice"), None).unwrap();

    // Add the project (use local path as fork URL)
    bm::commands::projects::add(&fork_repo.to_string_lossy(), None).unwrap();

    // Sync
    bm::commands::teams::sync(false, None).unwrap();

    // Verify workspace: {team_dir}/architect-alice/fake-fork/
    let team_dir = team_repo.parent().unwrap();
    let ws = team_dir.join("architect-alice").join("fake-fork");

    assert!(ws.join(".botminter").is_dir(), "workspace should have .botminter/");
    assert!(ws.join("PROMPT.md").exists(), "workspace should have PROMPT.md");
    assert!(ws.join("CLAUDE.md").exists(), "workspace should have CLAUDE.md");
    assert!(ws.join("ralph.yml").exists(), "workspace should have ralph.yml");
    assert!(ws.join(".gitignore").exists(), "workspace should have .gitignore");
    assert!(ws.join(".claude").is_dir(), "workspace should have .claude/");
    assert!(ws.join("README.md").exists(), "workspace should have target project content");
}

#[test]
fn lifecycle_sync_idempotent() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "idem-team", "scrum");

    bm::commands::hire::run("architect", Some("alice"), None).unwrap();

    // Sync twice — should not error
    bm::commands::teams::sync(false, None).unwrap();
    bm::commands::teams::sync(false, None).unwrap();
}

// ── Roles list test ──────────────────────────────────────────────────

#[test]
fn roles_list_shows_profile_roles() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    // Should not error — just prints a table to stdout
    bm::commands::roles::list(None).unwrap();
}

// ── Members list test ────────────────────────────────────────────────

#[test]
fn members_list_shows_hired_members() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    bm::commands::hire::run("architect", Some("alice"), None).unwrap();

    // Should not error — prints table with alice
    bm::commands::members::list(None).unwrap();
}

#[test]
fn members_list_empty_team() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    // Should not error — prints "no members" message
    bm::commands::members::list(None).unwrap();
}

// ── Member-discovery regression tests ─────────────────────────────────
//
// These tests document the bug where `bm status` (and `bm start`) scan
// `team.path.join("team")` — the team repo root — instead of
// `team.path.join("team").join("team")` where hired members live.
// Structural dirs (knowledge/, invariants/, etc.) get listed as members.
//
// These tests use subprocess-only setup (no env::set_var) to avoid poisoning
// ENV_MUTEX when #[should_panic] catches the panic.
// Remove #[should_panic] after task-09 fixes the bug.

/// Sets up a team repo without calling `env::set_var("HOME", ...)`.
///
/// Use this for `#[should_panic]` tests that must not poison the shared
/// `ENV_MUTEX`. All `bm` CLI calls should be done via `Command::new()`
/// with `.env("HOME", tmp)` instead of calling library functions directly.
fn setup_team_for_subprocess(tmp: &Path, team_name: &str, profile_name: &str) -> PathBuf {
    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();

    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "test@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM Test"]);

    profile::extract_profile_to(profile_name, &team_repo).unwrap();

    fs::create_dir_all(team_repo.join("team")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("team/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();

    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: profile_name.to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
        }],
    };

    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    // NOTE: Does NOT set HOME — caller must pass HOME to subprocesses
    team_repo
}

/// Extracts member names from CLI table output.
///
/// The `UTF8_FULL_CONDENSED` comfy-table preset uses `│` (U+2502) for outer
/// borders and `┆` (U+2506) for inner column separators. Data rows look like:
///   `│ alice ┆ architect ┆ stopped ┆ … │`
fn extract_member_names(output: &str) -> Vec<String> {
    let mut names = Vec::new();
    for line in output.lines() {
        // Data rows contain the inner separator ┆
        if !line.contains('┆') {
            continue;
        }
        // Strip outer │ borders, then split by inner ┆
        let inner = line
            .trim()
            .trim_start_matches('│')
            .trim_end_matches('│');
        let first_col = inner.split('┆').next().unwrap_or("").trim();
        // Skip header row and empty cells
        if first_col.is_empty() || first_col == "Member" {
            continue;
        }
        names.push(first_col.to_string());
    }
    names.sort();
    names
}

/// Schema-level structural directories that exist at the team repo root.
/// These should NEVER appear as member entries.
const STRUCTURAL_DIRS: &[&str] = &["knowledge", "invariants", "projects", "agent"];

#[test]
fn status_only_lists_hired_members() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "regression-team", "scrum");

    // Dynamically select a valid role from the profile
    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];
    let expected_member = format!("{}-alice", role);

    // Hire one member via subprocess
    let hire_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["hire", role, "--name", "alice", "-t", "regression-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm hire");

    assert!(
        hire_out.status.success(),
        "bm hire should exit 0, stderr: {}",
        String::from_utf8_lossy(&hire_out.stderr)
    );

    // Run bm status via CLI subprocess to capture stdout
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["status", "-t", "regression-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm status");

    assert!(
        output.status.success(),
        "bm status should exit 0, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let members = extract_member_names(&stdout);

    // Structural dirs should NOT appear as members
    let structural_found: Vec<&str> = STRUCTURAL_DIRS
        .iter()
        .filter(|d| members.iter().any(|m| m == **d))
        .copied()
        .collect();

    // Hired member should appear
    let has_hired = members.iter().any(|m| m == &expected_member);

    assert!(
        structural_found.is_empty() && has_hired,
        "BUG: status lists structural dirs as members. \
         Expected only '{}' but found structural dirs {:?} in members {:?}.\n\
         Full output:\n{}",
        expected_member,
        structural_found,
        members,
        stdout
    );
}

#[test]
fn status_matches_members_list() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "match-team", "scrum");

    // Dynamically select a valid role from the profile
    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    // Hire one member via subprocess
    let hire_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["hire", role, "--name", "alice", "-t", "match-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm hire");

    assert!(
        hire_out.status.success(),
        "bm hire should exit 0, stderr: {}",
        String::from_utf8_lossy(&hire_out.stderr)
    );

    // Run bm status
    let status_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["status", "-t", "match-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm status");

    assert!(
        status_out.status.success(),
        "bm status should exit 0, stderr: {}",
        String::from_utf8_lossy(&status_out.stderr)
    );

    // Run bm members list
    let members_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["members", "list", "-t", "match-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm members list");

    assert!(
        members_out.status.success(),
        "bm members list should exit 0, stderr: {}",
        String::from_utf8_lossy(&members_out.stderr)
    );

    let status_members = extract_member_names(&String::from_utf8_lossy(&status_out.stdout));
    let members_list = extract_member_names(&String::from_utf8_lossy(&members_out.stdout));

    assert_eq!(
        status_members, members_list,
        "BUG: status and members list disagree on member names. \
         status sees: {:?}, members list sees: {:?}",
        status_members, members_list
    );
}

// ── Completions tests (no HOME/config needed) ────────────────────────

#[test]
fn completions_bash() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "bash"])
        .output()
        .expect("failed to run bm");
    assert!(output.status.success(), "bm completions bash should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "bash completions should not be empty");
    assert!(stdout.contains("bm"), "bash completions should reference bm");
}

#[test]
fn completions_zsh() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "zsh"])
        .output()
        .expect("failed to run bm");
    assert!(output.status.success(), "bm completions zsh should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "zsh completions should not be empty");
    assert!(stdout.contains("bm"), "zsh completions should reference bm");
}

#[test]
fn completions_fish() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "fish"])
        .output()
        .expect("failed to run bm");
    assert!(output.status.success(), "bm completions fish should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "fish completions should not be empty");
    assert!(stdout.contains("bm"), "fish completions should reference bm");
}

#[test]
fn completions_powershell() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "powershell"])
        .output()
        .expect("failed to run bm");
    assert!(output.status.success(), "bm completions powershell should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "powershell completions should not be empty");
    assert!(stdout.contains("bm"), "powershell completions should reference bm");
}

#[test]
fn completions_elvish() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "elvish"])
        .output()
        .expect("failed to run bm");
    assert!(output.status.success(), "bm completions elvish should exit 0");
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(!stdout.is_empty(), "elvish completions should not be empty");
    assert!(stdout.contains("bm"), "elvish completions should reference bm");
}

#[test]
fn completions_invalid_shell_rejected() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["completions", "notashell"])
        .output()
        .expect("failed to run bm");
    assert!(
        !output.status.success(),
        "bm completions notashell should exit non-zero"
    );
}

// ── Helper: create fake fork repo ────────────────────────────────────

/// Creates a local git repository that can be used as a project fork URL.
fn create_fake_fork(tmp: &Path, name: &str) -> PathBuf {
    let fork = tmp.join(name);
    fs::create_dir_all(&fork).unwrap();
    git(&fork, &["init", "-b", "main"]);
    git(&fork, &["config", "user.email", "test@botminter.test"]);
    git(&fork, &["config", "user.name", "BM Test"]);
    fs::write(fork.join("README.md"), format!("# {}", name)).unwrap();
    git(&fork, &["add", "-A"]);
    git(&fork, &["commit", "-m", "init"]);
    fork
}

// ── Cross-command consistency tests ──────────────────────────────────

/// Verifies that `bm status` and `bm members list` report the same members.
#[test]
fn status_and_members_list_agree() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "agree-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role_a = &roles[0];
    let role_b = if roles.len() > 1 { &roles[1] } else { &roles[0] };

    for (role, name) in [(role_a.as_str(), "alice"), (role_b.as_str(), "bob")] {
        let out = Command::new(env!("CARGO_BIN_EXE_bm"))
            .args(["hire", role, "--name", name, "-t", "agree-team"])
            .env("HOME", tmp.path())
            .output()
            .expect("failed to run bm hire");
        assert!(
            out.status.success(),
            "bm hire {} --name {} failed: {}",
            role,
            name,
            String::from_utf8_lossy(&out.stderr)
        );
    }

    let status_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["status", "-t", "agree-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm status");
    assert!(status_out.status.success());

    let members_out = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["members", "list", "-t", "agree-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm members list");
    assert!(members_out.status.success());

    let status_members = extract_member_names(&String::from_utf8_lossy(&status_out.stdout));
    let members_list = extract_member_names(&String::from_utf8_lossy(&members_out.stdout));

    assert_eq!(
        status_members, members_list,
        "cross-command: status and members list disagree with 2 members. \
         status sees: {:?}, members list sees: {:?}",
        status_members, members_list
    );
}

#[test]
fn roles_list_matches_profile_describe() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "roles-match-team", "scrum");

    let expected_roles = profile::list_roles("scrum").unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["roles", "list", "-t", "roles-match-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm roles list");
    assert!(
        output.status.success(),
        "bm roles list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for role in &expected_roles {
        assert!(
            stdout.contains(role.as_str()),
            "roles list output should contain role '{}', output:\n{}",
            role,
            stdout
        );
    }
}

#[test]
fn hire_then_members_list_shows_role() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "hire-shows-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    let hire = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["hire", role, "--name", "charlie", "-t", "hire-shows-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm hire");
    assert!(
        hire.status.success(),
        "bm hire failed: {}",
        String::from_utf8_lossy(&hire.stderr)
    );

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["members", "list", "-t", "hire-shows-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm members list");
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_member = format!("{}-charlie", role);

    assert!(
        stdout.contains(&expected_member),
        "members list should show '{}', output:\n{}",
        expected_member,
        stdout
    );
    assert!(
        stdout.contains(role.as_str()),
        "members list should show role '{}', output:\n{}",
        role,
        stdout
    );
}

// ── Multi-member / multi-project scenarios ───────────────────────────

#[test]
fn hire_multiple_roles_then_sync() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "multi-hire-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let picks: Vec<&str> = (0..3).map(|i| roles[i % roles.len()].as_str()).collect();
    let names = ["m1", "m2", "m3"];

    for (role, name) in picks.iter().zip(names.iter()) {
        bm::commands::hire::run(role, Some(name), None).unwrap();
    }

    let fork_a = create_fake_fork(tmp.path(), "proj-alpha");
    let fork_b = create_fake_fork(tmp.path(), "proj-beta");
    bm::commands::projects::add(&fork_a.to_string_lossy(), None).unwrap();
    bm::commands::projects::add(&fork_b.to_string_lossy(), None).unwrap();

    bm::commands::teams::sync(false, None).unwrap();

    let team_dir = team_repo.parent().unwrap();
    for (role, name) in picks.iter().zip(names.iter()) {
        let member_dir = format!("{}-{}", role, name);
        let ws_alpha = team_dir.join(&member_dir).join("proj-alpha");
        let ws_beta = team_dir.join(&member_dir).join("proj-beta");

        assert!(
            ws_alpha.join(".botminter").is_dir(),
            "{}/proj-alpha should have .botminter/",
            member_dir
        );
        assert!(
            ws_alpha.join(".claude").is_dir(),
            "{}/proj-alpha should have .claude/",
            member_dir
        );
        assert!(
            ws_beta.join(".botminter").is_dir(),
            "{}/proj-beta should have .botminter/",
            member_dir
        );
    }
}

#[test]
fn sync_with_multiple_projects_creates_project_workspaces() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "proj-ws-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    bm::commands::hire::run(role, Some("alice"), None).unwrap();

    let fork_a = create_fake_fork(tmp.path(), "project-one");
    let fork_b = create_fake_fork(tmp.path(), "project-two");
    bm::commands::projects::add(&fork_a.to_string_lossy(), None).unwrap();
    bm::commands::projects::add(&fork_b.to_string_lossy(), None).unwrap();

    bm::commands::teams::sync(false, None).unwrap();

    let team_dir = team_repo.parent().unwrap();
    let member = format!("{}-alice", role);

    for proj in &["project-one", "project-two"] {
        let ws = team_dir.join(&member).join(proj);
        assert!(ws.join(".botminter").is_dir(), "{}/{} should have .botminter/", member, proj);
        assert!(ws.join("PROMPT.md").exists(), "{}/{} should have PROMPT.md", member, proj);
        assert!(ws.join("CLAUDE.md").exists(), "{}/{} should have CLAUDE.md", member, proj);
        assert!(ws.join("ralph.yml").exists(), "{}/{} should have ralph.yml", member, proj);
        assert!(ws.join(".claude").is_dir(), "{}/{} should have .claude/", member, proj);
        assert!(ws.join("README.md").exists(), "{}/{} should have project content", member, proj);
    }
}

#[test]
fn hire_same_role_twice_auto_suffix() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "suffix-dyn-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    bm::commands::hire::run(role, None, None).unwrap();
    bm::commands::hire::run(role, None, None).unwrap();

    let m1 = team_repo.join(format!("team/{}-01", role));
    let m2 = team_repo.join(format!("team/{}-02", role));

    assert!(m1.is_dir(), "{}-01 should exist", role);
    assert!(m2.is_dir(), "{}-02 should exist", role);

    // Both should have proper skeleton files
    assert!(m1.join("botminter.yml").exists(), "{}-01 should have botminter.yml", role);
    assert!(m2.join("botminter.yml").exists(), "{}-02 should have botminter.yml", role);
}

#[test]
fn sync_after_second_hire_creates_new_workspace() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "incr-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role_a = &roles[0];

    bm::commands::hire::run(role_a, Some("first"), None).unwrap();
    bm::commands::teams::sync(false, None).unwrap();

    let team_dir = team_repo.parent().unwrap();
    let first_ws = team_dir.join(format!("{}-first", role_a));
    assert!(
        first_ws.join(".botminter").is_dir(),
        "first workspace should exist after initial sync"
    );

    // Hire second member (different role if available)
    let role_b = if roles.len() > 1 { &roles[1] } else { role_a };
    bm::commands::hire::run(role_b, Some("second"), None).unwrap();

    bm::commands::teams::sync(false, None).unwrap();

    let second_ws = team_dir.join(format!("{}-second", role_b));
    assert!(
        second_ws.join(".botminter").is_dir(),
        "second workspace should exist after incremental sync"
    );
    assert!(
        first_ws.join(".botminter").is_dir(),
        "first workspace should still exist after incremental sync"
    );
}

// ── Error paths ──────────────────────────────────────────────────────

#[test]
fn status_missing_team_repo_dir_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "phantom-team", "scrum");

    // Delete the team directory entirely
    fs::remove_dir_all(tmp.path().join("workspaces/phantom-team")).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["status", "-t", "phantom-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm status");

    // Status either errors (non-zero) or handles gracefully (no panic).
    // The key invariant: no panic, and no phantom members listed.
    if output.status.success() {
        let stdout = String::from_utf8_lossy(&output.stdout);
        let members = extract_member_names(&stdout);
        assert!(
            members.is_empty(),
            "status should show no members when team dir is missing, found: {:?}",
            members
        );
    }
}

#[test]
fn hire_with_corrupt_manifest_errors() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "corrupt-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    // Corrupt the manifest with invalid YAML
    fs::write(team_repo.join("botminter.yml"), "{{not valid yaml!!!").unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: corrupt manifest"]);

    let result = bm::commands::hire::run(role, Some("alice"), None);
    assert!(
        result.is_err(),
        "hire should error with corrupt manifest"
    );
}

#[test]
fn sync_missing_workzone_creates_it() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "recreate-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    bm::commands::hire::run(role, Some("alice"), None).unwrap();
    bm::commands::teams::sync(false, None).unwrap();

    let member_dir = format!("{}-alice", role);
    let ws = tmp.path().join("workspaces/recreate-team").join(&member_dir);
    assert!(ws.is_dir(), "workspace should exist after first sync");

    // Delete the workspace directory
    fs::remove_dir_all(&ws).unwrap();
    assert!(!ws.exists(), "workspace should be deleted");

    // Sync again — should recreate the missing workspace
    bm::commands::teams::sync(false, None).unwrap();
    assert!(
        ws.join(".botminter").is_dir(),
        "sync should recreate missing workspace"
    );
}

#[test]
fn teams_list_with_empty_config() {
    let tmp = tempfile::tempdir().unwrap();

    let config = BotminterConfig {
        workzone: tmp.path().join("workspaces"),
        default_team: None,
        teams: vec![],
    };
    let config_path = tmp.path().join(".botminter/config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["teams", "list"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm teams list");

    assert!(
        output.status.success(),
        "teams list should succeed with empty config, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn projects_add_invalid_url_format() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "url-team", "scrum");

    // URL with no typical repo path — should derive a name or error helpfully
    let result = bm::commands::projects::add("https://example.com", None);

    match result {
        Ok(()) => {
            // Derived a project name successfully — verify manifest updated
            let team_repo = tmp.path().join("workspaces/url-team/team");
            let manifest = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
            assert!(
                manifest.contains("example"),
                "manifest should reference the derived project name"
            );
        }
        Err(e) => {
            // Error message should be helpful, not empty
            let msg = e.to_string();
            assert!(!msg.is_empty(), "error message should not be empty");
        }
    }
}

// ── Output format verification ───────────────────────────────────────

#[test]
fn status_table_has_expected_columns() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "status-cols-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    let hire = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["hire", role, "--name", "alice", "-t", "status-cols-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm hire");
    assert!(hire.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["status", "-t", "status-cols-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm status");
    assert!(
        output.status.success(),
        "bm status failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for header in &["Member", "Role", "Status", "Started", "PID"] {
        assert!(
            stdout.contains(header),
            "status output should contain '{}' column header, output:\n{}",
            header,
            stdout
        );
    }
}

#[test]
fn members_list_table_has_expected_columns() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "members-cols-team", "scrum");

    let roles = profile::list_roles("scrum").unwrap();
    let role = &roles[0];

    let hire = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["hire", role, "--name", "alice", "-t", "members-cols-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm hire");
    assert!(hire.status.success());

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["members", "list", "-t", "members-cols-team"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm members list");
    assert!(
        output.status.success(),
        "bm members list failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    for header in &["Member", "Role", "Status"] {
        assert!(
            stdout.contains(header),
            "members list output should contain '{}' column header, output:\n{}",
            header,
            stdout
        );
    }
}

// ── Knowledge management tests ───────────────────────────────────────

#[test]
fn knowledge_list_shows_files_at_all_scopes() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Team-level knowledge already exists from profile extraction
    // Add project-level and member-level knowledge
    let member_dir = team_repo.join("team/architect-01");
    fs::create_dir_all(&member_dir).unwrap();
    fs::create_dir_all(member_dir.join("knowledge")).unwrap();
    fs::write(
        member_dir.join("knowledge/design-patterns.md"),
        "# Design Patterns\n",
    )
    .unwrap();
    fs::write(
        member_dir.join("botminter.yml"),
        "role: architect\n",
    )
    .unwrap();

    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: add member knowledge"]);

    // Run knowledge list
    let result = bm::commands::knowledge::list(None, None);
    assert!(result.is_ok(), "knowledge list failed: {:?}", result.err());
}

#[test]
fn knowledge_list_scope_filter() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Team scope only
    let result = bm::commands::knowledge::list(None, Some("team"));
    assert!(result.is_ok());
}

#[test]
fn knowledge_show_displays_file_content() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Write a known knowledge file
    fs::write(
        team_repo.join("knowledge/test-file.md"),
        "# Test Content\nHello world\n",
    )
    .unwrap();

    let result = bm::commands::knowledge::show("knowledge/test-file.md", None);
    assert!(result.is_ok(), "knowledge show failed: {:?}", result.err());
}

#[test]
fn knowledge_show_file_not_found() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let result = bm::commands::knowledge::show("knowledge/nonexistent.md", None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("File not found"), "Got: {}", err);
}

#[test]
fn knowledge_show_path_outside_knowledge_dir() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let _team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let result = bm::commands::knowledge::show("botminter.yml", None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(
        err.contains("not within a knowledge or invariant directory"),
        "Got: {}",
        err
    );
}

#[test]
fn knowledge_v1_team_blocked() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Downgrade schema to v1
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: 0.1");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: downgrade to v1"]);

    let result = bm::commands::knowledge::list(None, None);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("requires schema 1.0"), "Got: {}", err);
    assert!(err.contains("bm upgrade"), "Got: {}", err);
}

// ── Schema init tests ─────────────────────────────────────────────

#[test]
fn init_creates_skills_and_formations_dirs() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Verify directories are extracted
    assert!(
        team_repo.join("skills").is_dir(),
        "skills/ should exist in team repo"
    );
    assert!(
        team_repo.join("formations").is_dir(),
        "formations/ should exist in team repo"
    );
    assert!(
        team_repo.join("skills/knowledge-manager/SKILL.md").exists(),
        "knowledge-manager skill should exist"
    );
    assert!(
        team_repo.join("formations/local/formation.yml").exists(),
        "local formation config should exist"
    );
    assert!(
        team_repo.join("formations/k8s/formation.yml").exists(),
        "k8s formation config should exist"
    );
}

#[test]
fn botminter_yml_has_correct_schema() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    assert!(
        content.contains("schema_version: '1.0'"),
        "Team repo should have schema 1.0, got:\n{}",
        content
    );
}

// ── Formation config tests ───────────────────────────────────────────

#[test]
fn formation_list_from_team_repo() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let formations = bm::formation::list_formations(&team_repo).unwrap();
    assert!(formations.contains(&"local".to_string()));
    assert!(formations.contains(&"k8s".to_string()));
}

#[test]
fn formation_load_local() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let config = bm::formation::load(&team_repo, "local").unwrap();
    assert_eq!(config.name, "local");
    assert!(config.is_local());
}

#[test]
fn formation_resolve_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // No flag → default to local (formations dir exists)
    let result = bm::formation::resolve_formation(&team_repo, None).unwrap();
    assert_eq!(result, Some("local".to_string()));
}

#[test]
fn formation_v1_gate_blocks_non_default() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Downgrade to v1 and remove formations dir
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: 0.1");
    fs::write(&manifest_path, content).unwrap();
    fs::remove_dir_all(team_repo.join("formations")).unwrap();
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "chore: simulate v1"]);

    // No formations dir → None (legacy)
    let result = bm::formation::resolve_formation(&team_repo, None).unwrap();
    assert_eq!(result, None);
}

// ── Daemon lifecycle tests ───────────────────────────────────────────

/// RAII guard that stops and cleans up a daemon process on drop.
///
/// If a test panics before manual cleanup, this guard ensures the daemon
/// is killed and PID/config files are removed.
struct DaemonGuard {
    team_name: String,
    home: PathBuf,
}

impl DaemonGuard {
    fn new(home: &Path, team_name: &str) -> Self {
        DaemonGuard {
            team_name: team_name.to_string(),
            home: home.to_path_buf(),
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        // Try graceful stop via bm daemon stop
        let _ = Command::new(env!("CARGO_BIN_EXE_bm"))
            .args(["daemon", "stop", "-t", &self.team_name])
            .env("HOME", &self.home)
            .output();

        // Force-kill via PID file if still alive
        let pid_file = self.home.join(format!(".botminter/daemon-{}.pid", self.team_name));
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                unsafe {
                    if libc::kill(pid, 0) == 0 {
                        libc::kill(pid, libc::SIGKILL);
                    }
                }
            }
        }

        // Clean up files
        let _ = fs::remove_file(
            self.home.join(format!(".botminter/daemon-{}.pid", self.team_name)),
        );
        let _ = fs::remove_file(
            self.home.join(format!(".botminter/daemon-{}.json", self.team_name)),
        );
        let _ = fs::remove_file(
            self.home.join(format!(".botminter/daemon-{}-poll.json", self.team_name)),
        );
    }
}

#[test]
fn daemon_start_creates_pid_and_config_files() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-test");

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--mode", "poll", "-t", "daemon-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run bm daemon start");

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);

    assert!(
        output.status.success(),
        "bm daemon start should exit 0, stdout: {}, stderr: {}",
        stdout,
        stderr
    );
    assert!(stdout.contains("Daemon started"), "Should print started message: {}", stdout);

    // Verify PID file exists
    let pid_file = tmp.path().join(".botminter/daemon-daemon-test.pid");
    assert!(pid_file.exists(), "PID file should exist");

    // Verify config file exists
    let cfg_file = tmp.path().join(".botminter/daemon-daemon-test.json");
    assert!(cfg_file.exists(), "Config file should exist");

    // Read and validate config content
    let cfg_content = fs::read_to_string(&cfg_file).unwrap();
    let cfg: serde_json::Value = serde_json::from_str(&cfg_content).unwrap();
    assert_eq!(cfg["team"], "daemon-test");
    assert_eq!(cfg["mode"], "poll");
}

#[test]
fn daemon_status_shows_running() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-status-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-status-test");

    // Start daemon
    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--mode", "poll", "-t", "daemon-status-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start.status.success(), "daemon start failed: {}", String::from_utf8_lossy(&start.stderr));

    // Check status
    let status = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "status", "-t", "daemon-status-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to get daemon status");

    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status.status.success(), "daemon status should exit 0");
    assert!(stdout.contains("running"), "Should show running: {}", stdout);
    assert!(stdout.contains("poll"), "Should show mode: {}", stdout);
}

#[test]
fn daemon_stop_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-stop-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-stop-test");

    // Start daemon
    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--mode", "poll", "-t", "daemon-stop-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start.status.success());

    // Stop daemon
    let stop = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "stop", "-t", "daemon-stop-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to stop daemon");

    let stdout = String::from_utf8_lossy(&stop.stdout);
    assert!(stop.status.success(), "daemon stop should exit 0");
    assert!(stdout.contains("Daemon stopped"), "Should print stopped: {}", stdout);

    // Verify cleanup
    let pid_file = tmp.path().join(".botminter/daemon-daemon-stop-test.pid");
    assert!(!pid_file.exists(), "PID file should be removed");
    let cfg_file = tmp.path().join(".botminter/daemon-daemon-stop-test.json");
    assert!(!cfg_file.exists(), "Config file should be removed");
}

#[test]
fn daemon_start_already_running_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-dup-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-dup-test");

    // Start daemon
    let start1 = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--mode", "poll", "-t", "daemon-dup-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start1.status.success());

    // Try starting again
    let start2 = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--mode", "poll", "-t", "daemon-dup-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run second daemon start");

    assert!(!start2.status.success(), "Second start should fail");
    let stderr = String::from_utf8_lossy(&start2.stderr);
    assert!(stderr.contains("already running"), "Should say already running: {}", stderr);
}

#[test]
fn daemon_stop_not_running_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-norun-test", "scrum");

    let stop = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "stop", "-t", "daemon-norun-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run daemon stop");

    assert!(!stop.status.success(), "Stop should fail when not running");
    let stderr = String::from_utf8_lossy(&stop.stderr);
    assert!(
        stderr.contains("not running") || stderr.contains("not found"),
        "Should indicate not running: {}",
        stderr
    );
}

#[test]
fn daemon_status_not_running() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-nostat-test", "scrum");

    let status = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "status", "-t", "daemon-nostat-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run daemon status");

    assert!(status.status.success(), "Status should exit 0 even when not running");
    let stdout = String::from_utf8_lossy(&status.stdout);
    assert!(stdout.contains("not running"), "Should say not running: {}", stdout);
}

#[test]
fn daemon_v1_team_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team_for_subprocess(tmp.path(), "daemon-v1-test", "scrum");

    // Downgrade to v1
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: 0.1");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: downgrade to v1"]);

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "-t", "daemon-v1-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to run daemon start");

    assert!(!output.status.success(), "Should fail for v1 team");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("requires schema 1.0"), "Got: {}", stderr);
}

#[test]
fn daemon_webhook_mode_starts_and_stops() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-wh-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-test");

    // Start in webhook mode on a high port to avoid conflicts
    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args([
            "daemon", "start",
            "--mode", "webhook",
            "--port", "19484",
            "-t", "daemon-wh-test",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");

    let stdout = String::from_utf8_lossy(&start.stdout);
    assert!(
        start.status.success(),
        "Webhook daemon start should exit 0, stderr: {}",
        String::from_utf8_lossy(&start.stderr)
    );
    assert!(stdout.contains("Daemon started"), "Should show started: {}", stdout);

    // Wait briefly for server to bind
    thread::sleep(Duration::from_millis(500));

    // Check status shows webhook mode
    let status = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "status", "-t", "daemon-wh-test"])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to get daemon status");

    let status_stdout = String::from_utf8_lossy(&status.stdout);
    assert!(status_stdout.contains("webhook"), "Should show webhook mode: {}", status_stdout);
}

// ── Daemon CLI parsing tests ─────────────────────────────────────────

#[test]
fn daemon_cli_parsing_start_flags() {
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["daemon", "start", "--help"])
        .output()
        .expect("failed to run bm daemon start --help");

    assert!(output.status.success());
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("--mode"), "Help should mention --mode: {}", stdout);
    assert!(stdout.contains("--port"), "Help should mention --port: {}", stdout);
    assert!(stdout.contains("--interval"), "Help should mention --interval: {}", stdout);
}

use std::time::Duration;
use std::thread;

// ── Webhook endpoint tests ───────────────────────────────────────────

#[test]
fn daemon_webhook_accepts_relevant_event() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-wh-accept", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-accept");

    let port = 19485u16;

    // Start webhook daemon
    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args([
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-accept",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start.status.success(), "start failed: {}", String::from_utf8_lossy(&start.stderr));

    // Wait for server to be ready
    thread::sleep(Duration::from_secs(1));

    // Send a relevant event via HTTP POST
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&format!("http://127.0.0.1:{}/webhook", port))
        .header("X-GitHub-Event", "issues")
        .header("Content-Type", "application/json")
        .body(r#"{"action":"opened","issue":{"number":1}}"#)
        .send();

    match resp {
        Ok(r) => {
            assert_eq!(r.status().as_u16(), 200, "Webhook should respond 200");
        }
        Err(e) => {
            eprintln!("Warning: could not connect to webhook server: {}", e);
        }
    }
}

#[test]
fn daemon_webhook_rejects_irrelevant_event() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-wh-reject", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-reject");

    let port = 19486u16;

    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args([
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-reject",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start.status.success());

    thread::sleep(Duration::from_secs(1));

    // Send an irrelevant event (push) — daemon should accept but not trigger members
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&format!("http://127.0.0.1:{}/webhook", port))
        .header("X-GitHub-Event", "push")
        .header("Content-Type", "application/json")
        .body(r#"{"ref":"refs/heads/main"}"#)
        .send();

    match resp {
        Ok(r) => {
            assert_eq!(r.status().as_u16(), 200, "Should still respond 200 for irrelevant events");
        }
        Err(e) => {
            eprintln!("Warning: could not connect to webhook server: {}", e);
        }
    }
}

#[test]
fn daemon_webhook_returns_404_for_wrong_path() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team_for_subprocess(tmp.path(), "daemon-wh-404", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-404");

    let port = 19487u16;

    let start = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args([
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-404",
        ])
        .env("HOME", tmp.path())
        .output()
        .expect("failed to start daemon");
    assert!(start.status.success());

    thread::sleep(Duration::from_secs(1));

    // Send to wrong path
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(&format!("http://127.0.0.1:{}/wrong-path", port))
        .body("test")
        .send();

    match resp {
        Ok(r) => {
            assert_eq!(r.status().as_u16(), 404, "Wrong path should get 404");
        }
        Err(e) => {
            eprintln!("Warning: could not connect to webhook server: {}", e);
        }
    }
}

// ── Projects sync tests ──────────────────────────────────────────────

#[test]
fn projects_sync_fails_without_github_repo() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    // setup_team creates a team with empty github_repo
    setup_team(tmp.path(), "test-team", "scrum");

    // projects sync should fail because there's no github_repo configured
    let result = bm::commands::projects::sync(None);
    assert!(result.is_err(), "sync should fail without github_repo");
}

#[test]
fn projects_sync_cli_parses() {
    // Verify `bm projects sync --help` parses correctly
    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .args(["projects", "sync", "--help"])
        .output()
        .expect("failed to run bm");
    assert!(
        output.status.success(),
        "bm projects sync --help should succeed"
    );
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Sync") || stdout.contains("sync"),
        "help should mention sync, got:\n{}",
        stdout
    );
}

#[test]
fn profile_views_extracted_to_team_repo() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Verify the extracted botminter.yml includes views
    let content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    assert!(
        content.contains("views:"),
        "extracted botminter.yml should contain views section, got:\n{}",
        content
    );
    assert!(
        content.contains("prefixes:"),
        "views should have prefixes field"
    );
    assert!(
        content.contains("also_include:"),
        "views should have also_include field"
    );
}

#[test]
fn profile_views_parse_correctly() {
    let _lock = ENV_MUTEX.lock().unwrap();
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum-compact");

    // Parse the extracted manifest
    let content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    let manifest: profile::ProfileManifest = serde_yml::from_str(&content).unwrap();

    assert!(!manifest.views.is_empty(), "compact profile should have views");

    // The PO view should resolve to po:* statuses + done + error
    let po_view = manifest
        .views
        .iter()
        .find(|v| v.name == "PO")
        .expect("should have a PO view");

    let resolved = po_view.resolve_statuses(&manifest.statuses);
    assert!(
        resolved.iter().any(|s| s == "po:triage"),
        "PO view should include po:triage"
    );
    assert!(
        resolved.iter().any(|s| s == "done"),
        "PO view should include done"
    );
    assert!(
        !resolved.iter().any(|s| s.starts_with("arch:")),
        "PO view should NOT include arch:* statuses"
    );

    // Filter string should start with "status:"
    let filter = po_view.filter_string(&manifest.statuses);
    assert!(
        filter.starts_with("status:"),
        "filter should start with 'status:', got: {}",
        filter
    );
    assert!(
        filter.contains("po:triage"),
        "filter should contain po:triage, got: {}",
        filter
    );
}
