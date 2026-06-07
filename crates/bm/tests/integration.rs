//! Integration tests for the `bm` CLI.
//!
//! These tests exercise multi-command workflows against temporary directories.
//! Each test uses fully isolated file system trees — no global env var mutation.
//! In-process library calls use explicit-path APIs. Commands that resolve
//! config via `dirs::home_dir()` are invoked as subprocesses with per-test HOME.
//!
//! Tests requiring the `ralph` binary (start/stop/status) are omitted since
//! ralph is not available in the test environment.

use std::fs;
use std::net::{TcpListener, TcpStream};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::thread;
use std::time::{Duration, Instant};

use bm::config::{BotminterConfig, Credentials, TeamEntry};
use bm::profile::{self, CodingAgentDef};

// ── Test helpers ──────────────────────────────────────────────────────

/// Returns the default Claude Code coding agent definition for tests.
fn claude_code_agent() -> CodingAgentDef {
    CodingAgentDef {
        name: "claude-code".into(),
        display_name: "Claude Code".into(),
        context_file: "CLAUDE.md".into(),
        agent_dir: ".claude".into(),
        binary: "claude".into(),
        system_prompt_flag: Some("--append-system-prompt-file".into()),
        skip_permissions_flag: Some("--dangerously-skip-permissions".into()),
    }
}

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
///   {tmp}/.config/botminter/profiles/  — profiles extracted from embedded data
///
/// Does NOT modify any environment variables. All paths are computed explicitly.
/// Returns the path to the team repo (the git repo inside the team dir).
fn setup_team(tmp: &Path, team_name: &str, profile_name: &str) -> PathBuf {
    // Compute profiles path directly — no env vars needed
    let profiles_path = profile::profiles_dir_for(tmp);
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();

    // Git init with config
    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "test@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM Test"]);

    // Extract profile content from disk into team repo (explicit base path, no env vars)
    profile::extract_profile_from(&profiles_path, profile_name, &team_repo, &claude_code_agent()).unwrap();

    // Create members/ and projects/ dirs (as bm init does)
    fs::create_dir_all(team_repo.join("members")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("members/.gitkeep"), "").unwrap();
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
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        }],
        vms: Vec::new(),
        keyring_collection: None,
    };

    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

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
    let profiles_path = profile::profiles_dir_for(tmp);
    profile::extract_profile_from(&profiles_path, profile_name, &team_repo, &claude_code_agent()).unwrap();
    fs::create_dir_all(team_repo.join("members")).unwrap();
    fs::create_dir_all(team_repo.join("projects")).unwrap();
    fs::write(team_repo.join("members/.gitkeep"), "").unwrap();
    fs::write(team_repo.join("projects/.gitkeep"), "").unwrap();
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init team repo"]);

    config.teams.push(TeamEntry {
        name: team_name.to_string(),
        path: team_dir,
        profile: profile_name.to_string(),
        github_repo: String::new(),
        credentials: Credentials::default(),
        coding_agent: None,
        project_number: None,
        bridge_lifecycle: Default::default(),
            daemon: Default::default(),
        vm: None,
    });

    if make_default {
        config.default_team = Some(team_name.to_string());
    }

    bm::config::save_to(&config_path, &config).unwrap();

    team_repo
}

// ── Subprocess helpers ───────────────────────────────────────────────

/// Creates a `bm` Command with HOME and XDG_CONFIG_HOME set for test isolation.
fn bm_cmd(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bm"));
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd
}

/// Runs a `bm` command and asserts success.
fn bm_run(home: &Path, args: &[&str]) -> std::process::Output {
    let output = bm_cmd(home)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("bm {} failed to run: {}", args.join(" "), e));
    assert!(
        output.status.success(),
        "bm {} failed (exit {:?}), stderr: {}",
        args.join(" "),
        output.status.code(),
        String::from_utf8_lossy(&output.stderr)
    );
    output
}

/// Runs a `bm` command and asserts failure. Returns stderr as a String.
fn bm_run_fail(home: &Path, args: &[&str]) -> String {
    let output = bm_cmd(home)
        .args(args)
        .output()
        .unwrap_or_else(|e| panic!("bm {} failed to run: {}", args.join(" "), e));
    assert!(
        !output.status.success(),
        "bm {} should have failed but exited 0, stdout: {}",
        args.join(" "),
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

/// Hires a member via subprocess.
fn bm_hire(home: &Path, role: &str, name: &str, team: &str) {
    bm_run(home, &["hire", role, "--name", name, "-t", team]);
}

/// Adds a project via subprocess.
fn bm_add_project(home: &Path, url: &str, team: &str) {
    bm_run(home, &["projects", "add", url, "-t", team]);
}

/// Gets an OS-assigned free port by binding to port 0.
fn get_free_port() -> u16 {
    let listener = TcpListener::bind("127.0.0.1:0").expect("failed to bind to free port");
    listener.local_addr().unwrap().port()
}

/// Polls until a TCP port accepts connections, with a timeout.
fn wait_for_port(port: u16, timeout: Duration) -> bool {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if TcpStream::connect(format!("127.0.0.1:{}", port)).is_ok() {
            return true;
        }
        thread::sleep(Duration::from_millis(50));
    }
    false
}

/// Creates a local git repository and returns a `file://` URL for use as a project fork URL.
fn create_fake_fork(tmp: &Path, name: &str) -> String {
    let fork = tmp.join(name);
    fs::create_dir_all(&fork).unwrap();
    git(&fork, &["init", "-b", "main"]);
    git(&fork, &["config", "user.email", "test@botminter.test"]);
    git(&fork, &["config", "user.name", "BM Test"]);
    fs::write(fork.join("README.md"), format!("# {}", name)).unwrap();
    git(&fork, &["add", "-A"]);
    git(&fork, &["commit", "-m", "init"]);
    format!("file://{}", fork.to_string_lossy())
}

// ── Profile tests (need disk profiles) ───────────────────────────────

#[test]
fn profiles_list_returns_all_from_disk() {
    let tmp = tempfile::tempdir().unwrap();
    let profiles_path = profile::profiles_dir_for(tmp.path());
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let profiles = profile::list_profiles_from(&profiles_path).unwrap();
    let embedded = profile::list_embedded_profiles();
    assert_eq!(
        profiles.len(),
        embedded.len(),
        "list_profiles should find all embedded profiles"
    );
    for name in &embedded {
        assert!(
            profiles.contains(name),
            "Profile '{}' should be discoverable",
            name
        );
    }
}

#[test]
fn profiles_describe_returns_complete_data() {
    let tmp = tempfile::tempdir().unwrap();
    let profiles_path = profile::profiles_dir_for(tmp.path());
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    for name in profile::list_profiles_from(&profiles_path).unwrap() {
        let manifest = profile::read_manifest_from(&name, &profiles_path).unwrap();
        assert_eq!(manifest.name, name, "Profile '{}' name should match", name);
        assert!(!manifest.display_name.is_empty(), "Profile '{}' should have display_name", name);
        assert!(!manifest.description.is_empty(), "Profile '{}' should have description", name);
        assert!(!manifest.schema_version.is_empty(), "Profile '{}' should have schema_version", name);
        assert!(!manifest.roles.is_empty(), "Profile '{}' should have roles", name);
        assert!(!manifest.labels.is_empty(), "Profile '{}' should have labels", name);

        for role in &manifest.roles {
            assert!(!role.name.is_empty(), "Profile '{}' role should have name", name);
            assert!(!role.description.is_empty(), "Profile '{}' role '{}' should have description", name, role.name);
        }
    }
}

#[test]
fn profiles_describe_nonexistent_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let profiles_path = profile::profiles_dir_for(tmp.path());
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let result = profile::read_manifest_from("does-not-exist", &profiles_path);
    assert!(result.is_err());
    let err = result.unwrap_err().to_string();
    assert!(err.contains("not found"));
}

// ── Hire tests ───────────────────────────────────────────────────────

#[test]
fn hire_with_explicit_name() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm_hire(tmp.path(), "architect", "bob", "test-team");

    // Verify member directory was created
    let member_dir = team_repo.join("members/architect-bob");
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
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm_run(tmp.path(), &["hire", "architect"]);

    let member_dir = team_repo.join("members/architect-01");
    assert!(member_dir.is_dir(), "architect-01/ should exist (auto-suffix)");
}

#[test]
fn hire_auto_suffix_increments() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm_run(tmp.path(), &["hire", "architect"]);
    bm_run(tmp.path(), &["hire", "architect"]);

    assert!(team_repo.join("members/architect-01").is_dir());
    assert!(team_repo.join("members/architect-02").is_dir());
}

#[test]
fn hire_unknown_role_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let stderr = bm_run_fail(tmp.path(), &["hire", "nonexistent-role", "--name", "alice"]);
    assert!(stderr.contains("nonexistent-role"));
    assert!(stderr.contains("architect")); // should list available roles
}

#[test]
fn hire_duplicate_name_succeeds_for_app_replacement() {
    // Per design doc idempotency table: existing member + no creds = keep member dir,
    // allow App replacement. Without a github_repo, no manifest flow triggers.
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    bm_hire(tmp.path(), "architect", "bob", "test-team");
    bm_run(tmp.path(), &["hire", "architect", "--name", "bob"]);

    // Member dir still exists
    assert!(team_repo.join("members/architect-bob").is_dir());
}

// ── Projects tests ───────────────────────────────────────────────────

#[test]
fn projects_add_creates_dirs_and_updates_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let fork_url = create_fake_fork(tmp.path(), "my-repo");
    bm_add_project(tmp.path(), &fork_url, "test-team");

    // Verify project dirs created
    let proj_dir = team_repo.join("projects/my-repo");
    assert!(proj_dir.join("knowledge").is_dir());
    assert!(proj_dir.join("invariants").is_dir());

    // Verify botminter.yml updated with project
    let manifest_content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    assert!(manifest_content.contains("my-repo"));
    assert!(manifest_content.contains(&fork_url));
}

#[test]
fn projects_add_duplicate_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let fork_url = create_fake_fork(tmp.path(), "my-repo");
    bm_add_project(tmp.path(), &fork_url, "test-team");
    let stderr = bm_run_fail(tmp.path(), &["projects", "add", &fork_url]);
    assert!(stderr.contains("already exists"));
}

#[test]
fn projects_add_nonexistent_url_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Snapshot manifest before the add attempt
    let manifest_before =
        fs::read_to_string(team_repo.join("botminter.yml")).unwrap();

    // Bare local paths are rejected (must use a URI scheme)
    let bad_path = tmp.path().join("does-not-exist-repo");
    let stderr = bm_run_fail(tmp.path(), &["projects", "add", &bad_path.to_string_lossy()]);
    assert!(
        stderr.contains("must use a URI scheme"),
        "error should reject bare path: {}",
        stderr
    );

    // file:// URI to a nonexistent repo is also rejected
    let bad_file_url = format!("file://{}", bad_path.to_string_lossy());
    let stderr = bm_run_fail(tmp.path(), &["projects", "add", &bad_file_url]);
    assert!(
        stderr.contains("not found") || stderr.contains("not a git repository"),
        "error should mention not found: {}",
        stderr
    );

    // Manifest should be unchanged — no partial write
    let manifest_after =
        fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    assert_eq!(manifest_before, manifest_after, "manifest should not change on failed add");
}

#[test]
fn sync_removed_returns_informative_error() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let stderr = bm_run_fail(tmp.path(), &["teams", "sync", "-t", "test-team"]);
    assert!(
        stderr.contains("bm teams sync has been removed"),
        "should explain sync was removed: {}",
        stderr
    );
    assert!(
        stderr.contains("bm minty"),
        "should reference bm minty: {}",
        stderr
    );
    assert!(
        stderr.contains("bm start"),
        "should reference bm start: {}",
        stderr
    );
}


// ── Schema version guard ─────────────────────────────────────────────

#[test]
fn schema_version_mismatch_blocks_hire() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Tamper with schema_version in team repo's botminter.yml
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: v99");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: bump schema"]);

    let stderr = bm_run_fail(tmp.path(), &["hire", "architect", "--name", "alice"]);
    assert!(stderr.contains("bm upgrade"), "Should suggest bm upgrade: {}", stderr);
}


// ── Multi-team and -t flag tests ─────────────────────────────────────

#[test]
fn multi_team_default_resolution() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo_alpha = setup_team(tmp.path(), "alpha", "scrum");
    add_team_to_config(tmp.path(), "beta", "agentic-sdlc-minimal", false);

    // Default team is "alpha" (set by setup_team)
    // Hire into default team (no -t flag)
    bm_run(tmp.path(), &["hire", "architect", "--name", "alice"]);
    assert!(team_repo_alpha.join("members/architect-alice").is_dir());
}

#[test]
fn team_flag_overrides_default() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "alpha", "scrum");
    let team_repo_beta = add_team_to_config(tmp.path(), "beta", "agentic-sdlc-minimal", false);

    // Use -t to target non-default team
    bm_hire(tmp.path(), "engineer", "clark", "beta");

    // Verify member landed in beta, not alpha
    assert!(team_repo_beta.join("members/engineer-clark").is_dir());
}

#[test]
fn team_flag_nonexistent_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "alpha", "scrum");

    let stderr = bm_run_fail(
        tmp.path(),
        &["hire", "architect", "--name", "bob", "-t", "nonexistent"],
    );
    assert!(stderr.contains("nonexistent"));
    assert!(stderr.contains("alpha")); // lists available teams
}

// ── Full lifecycle tests ─────────────────────────────────────────────





// ── Roles list test ──────────────────────────────────────────────────

#[test]
fn roles_list_shows_profile_roles() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    // Should not error — just prints a table to stdout
    bm_run(tmp.path(), &["roles", "list"]);
}

// ── Members list test ────────────────────────────────────────────────

#[test]
fn members_list_shows_hired_members() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    bm_hire(tmp.path(), "architect", "alice", "test-team");

    // Should not error — prints table with alice
    bm_run(tmp.path(), &["members", "list"]);
}

#[test]
fn members_list_empty_team() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    // Should not error — prints "no members" message
    bm_run(tmp.path(), &["members", "list"]);
}

// ── Member-discovery regression tests ─────────────────────────────────
//
// These tests document the bug where `bm status` (and `bm start`) scan
// `team.path.join("team")` — the team repo root — instead of
// `team.path.join("team").join("members")` where hired members live.
// Structural dirs (knowledge/, invariants/, etc.) get listed as members.

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
const STRUCTURAL_DIRS: &[&str] = &["knowledge", "invariants", "projects", "coding-agent"];

#[test]
fn status_only_lists_hired_members() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "regression-team", "scrum");

    let role = "architect";
    let expected_member = format!("{}-alice", role);

    // Hire one member via subprocess
    bm_hire(tmp.path(), role, "alice", "regression-team");

    // Run bm status via CLI subprocess to capture stdout
    let output = bm_run(tmp.path(), &["status", "-t", "regression-team"]);

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
    setup_team(tmp.path(), "match-team", "scrum");

    let role = "architect";

    // Hire one member via subprocess
    bm_hire(tmp.path(), role, "alice", "match-team");

    // Run bm status
    let status_out = bm_run(tmp.path(), &["status", "-t", "match-team"]);

    // Run bm members list
    let members_out = bm_run(tmp.path(), &["members", "list", "-t", "match-team"]);

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
    // Dynamic: registration script calls the binary at tab-time via COMPLETE env var
    assert!(
        stdout.contains("COMPLETE"),
        "bash completions should be dynamic (reference COMPLETE env var), output:\n{}",
        stdout
    );
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
    assert!(
        stdout.contains("COMPLETE"),
        "zsh completions should be dynamic, output:\n{}",
        stdout
    );
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
    assert!(
        stdout.contains("COMPLETE"),
        "fish completions should be dynamic, output:\n{}",
        stdout
    );
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
    assert!(
        stdout.contains("COMPLETE"),
        "powershell completions should be dynamic, output:\n{}",
        stdout
    );
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
    assert!(
        stdout.contains("COMPLETE"),
        "elvish completions should be dynamic, output:\n{}",
        stdout
    );
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

// ── Dynamic completion value tests ───────────────────────────────────

/// Invoke `bm` with COMPLETE=bash to simulate a tab-completion request.
/// Returns the completion candidates as a string.
///
/// `args` should be the command-line words as bash would tokenize them,
/// including the program name as the first element. The last element is
/// the word being completed (may be empty `""`).
fn complete_bash(args: &[&str], home: &Path) -> String {
    // The index of the word being completed (0-based).
    let index = args.len() - 1;

    // CompleteEnv protocol: COMPLETE=bash bm -- <words...>
    let mut cmd_args = vec!["--"];
    cmd_args.extend_from_slice(args);

    let output = Command::new(env!("CARGO_BIN_EXE_bm"))
        .env("COMPLETE", "bash")
        .env("HOME", home)
        .env("XDG_CONFIG_HOME", home.join(".config"))
        .env("_CLAP_COMPLETE_INDEX", index.to_string())
        .env("_CLAP_COMPLETE_COMP_TYPE", "9") // Normal completion
        .env("_CLAP_COMPLETE_SPACE", "true")
        .env("_CLAP_IFS", "\n")
        .args(&cmd_args)
        .output()
        .expect("failed to run bm");
    String::from_utf8_lossy(&output.stdout).to_string()
}

#[test]
fn dynamic_completions_include_profile_names() {
    let tmp = tempfile::tempdir().unwrap();
    // Extract profiles to disk so completions can find them
    let profiles_path = profile::profiles_dir_for(tmp.path());
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let completions = complete_bash(&["bm", "profiles", "describe", ""], tmp.path());
    // All embedded profiles should appear as candidates
    for name in profile::list_embedded_profiles() {
        assert!(
            completions.contains(&name),
            "profiles describe should suggest '{}', got:\n{}",
            name,
            completions
        );
    }
}

#[test]
fn dynamic_completions_include_team_names() {
    let tmp = tempfile::tempdir().unwrap();
    let team_name = "test-completions-team";
    setup_team(tmp.path(), team_name, "agentic-sdlc-minimal");

    let completions = complete_bash(&["bm", "hire", "somerole", "-t", ""], tmp.path());
    assert!(
        completions.contains(team_name),
        "hire -t should suggest '{}', got:\n{}",
        team_name,
        completions
    );
}

#[test]
fn dynamic_completions_include_role_names() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "roles-team", "scrum");

    let completions = complete_bash(&["bm", "hire", ""], tmp.path());
    assert!(
        completions.contains("architect"),
        "hire should suggest 'architect' role, got:\n{}",
        completions
    );
}

#[test]
fn dynamic_completions_include_member_names() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "members-team", "scrum");

    // Create a hired member directory
    let member_dir = team_repo.join("members").join("architect-01");
    fs::create_dir_all(&member_dir).unwrap();
    fs::write(member_dir.join(".botminter.yml"), "role: architect\n").unwrap();

    let completions = complete_bash(&["bm", "members", "show", ""], tmp.path());
    assert!(
        completions.contains("architect-01"),
        "members show should suggest 'architect-01', got:\n{}",
        completions
    );
}

#[test]
fn dynamic_completions_graceful_without_config() {
    let tmp = tempfile::tempdir().unwrap();
    // No config — completions should still work (empty candidates, no crash).
    let completions = complete_bash(&["bm", "hire", ""], tmp.path());
    // Should not crash. Output may be empty (no roles to suggest without config).
    // Just verify no panic/crash happened.
    let _ = completions;
}

#[test]
fn dynamic_completions_include_subcommands() {
    let tmp = tempfile::tempdir().unwrap();
    let completions = complete_bash(&["bm", ""], tmp.path());
    // Top-level subcommands should always be suggested.
    assert!(
        completions.contains("hire"),
        "top-level completions should include 'hire', got:\n{}",
        completions
    );
    assert!(
        completions.contains("start"),
        "top-level completions should include 'start', got:\n{}",
        completions
    );
}

// ── Cross-command consistency tests ──────────────────────────────────

/// Verifies that `bm status` and `bm members list` report the same members.
#[test]
fn status_and_members_list_agree() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "agree-team", "scrum");

    let role_a = "architect";
    let role_b = "human-assistant";

    for (role, name) in [(role_a, "alice"), (role_b, "bob")] {
        bm_hire(tmp.path(), role, name, "agree-team");
    }

    let status_out = bm_run(tmp.path(), &["status", "-t", "agree-team"]);
    let members_out = bm_run(tmp.path(), &["members", "list", "-t", "agree-team"]);

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
    setup_team(tmp.path(), "roles-match-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let expected_roles = profile::list_roles_from("scrum", &profiles_path).unwrap();

    let output = bm_run(tmp.path(), &["roles", "list", "-t", "roles-match-team"]);

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
    setup_team(tmp.path(), "hire-shows-team", "scrum");

    let role = "architect";

    bm_hire(tmp.path(), role, "charlie", "hire-shows-team");

    let output = bm_run(tmp.path(), &["members", "list", "-t", "hire-shows-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_member = format!("{}-charlie", role);

    assert!(
        stdout.contains(&expected_member),
        "members list should show '{}', output:\n{}",
        expected_member,
        stdout
    );
    assert!(
        stdout.contains(role),
        "members list should show role '{}', output:\n{}",
        role,
        stdout
    );
}

// ── Multi-member / multi-project scenarios ───────────────────────────



#[test]
fn hire_same_role_twice_auto_suffix() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "suffix-dyn-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    bm_run(tmp.path(), &["hire", role, "-t", "suffix-dyn-team"]);
    bm_run(tmp.path(), &["hire", role, "-t", "suffix-dyn-team"]);

    let m1 = team_repo.join(format!("members/{}-01", role));
    let m2 = team_repo.join(format!("members/{}-02", role));

    assert!(m1.is_dir(), "{}-01 should exist", role);
    assert!(m2.is_dir(), "{}-02 should exist", role);

    // Both should have proper skeleton files
    assert!(m1.join("botminter.yml").exists(), "{}-01 should have botminter.yml", role);
    assert!(m2.join("botminter.yml").exists(), "{}-02 should have botminter.yml", role);
}


// ── Error paths ──────────────────────────────────────────────────────

#[test]
fn status_missing_team_repo_dir_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "phantom-team", "scrum");

    // Delete the team directory entirely
    fs::remove_dir_all(tmp.path().join("workspaces/phantom-team")).unwrap();

    let output = bm_cmd(tmp.path())
        .args(["status", "-t", "phantom-team"])
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
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "corrupt-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    // Corrupt the manifest with invalid YAML
    fs::write(team_repo.join("botminter.yml"), "{{not valid yaml!!!").unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: corrupt manifest"]);

    let stderr = bm_run_fail(tmp.path(), &["hire", role, "--name", "alice"]);
    assert!(
        !stderr.is_empty(),
        "hire should error with corrupt manifest"
    );
}


#[test]
fn teams_list_with_empty_config() {
    let tmp = tempfile::tempdir().unwrap();

    let config = BotminterConfig {
        workzone: tmp.path().join("workspaces"),
        default_team: None,
        teams: vec![],
        vms: Vec::new(),
        keyring_collection: None,
    };
    let config_path = tmp.path().join(".botminter/config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    bm_run(tmp.path(), &["teams", "list"]);
}

#[test]
fn projects_add_invalid_url_format() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "url-team", "scrum");

    // URL with no typical repo path — should derive a name or error helpfully
    let output = bm_cmd(tmp.path())
        .args(["projects", "add", "https://example.com"])
        .output()
        .expect("failed to run bm projects add");

    if output.status.success() {
        // Derived a project name successfully — verify manifest updated
        let team_repo = tmp.path().join("workspaces/url-team/team");
        let manifest = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
        assert!(
            manifest.contains("example"),
            "manifest should reference the derived project name"
        );
    } else {
        // Error message should be helpful, not empty
        let stderr = String::from_utf8_lossy(&output.stderr);
        assert!(!stderr.is_empty(), "error message should not be empty");
    }
}

// ── Output format verification ───────────────────────────────────────

#[test]
fn status_table_has_expected_columns() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "status-cols-team", "scrum");

    let role = "architect";

    bm_hire(tmp.path(), role, "alice", "status-cols-team");

    let output = bm_run(tmp.path(), &["status", "-t", "status-cols-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    for header in &["Member", "Role", "Status", "Branch", "Started", "PID"] {
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
    setup_team(tmp.path(), "members-cols-team", "scrum");

    let role = "architect";

    bm_hire(tmp.path(), role, "alice", "members-cols-team");

    let output = bm_run(tmp.path(), &["members", "list", "-t", "members-cols-team"]);

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
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Team-level knowledge already exists from profile extraction
    // Add project-level and member-level knowledge
    let member_dir = team_repo.join("members/architect-01");
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
    bm_run(tmp.path(), &["knowledge", "list"]);
}

#[test]
fn knowledge_list_scope_filter() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    // Team scope only
    bm_run(tmp.path(), &["knowledge", "list", "--scope", "team"]);
}

#[test]
fn knowledge_show_displays_file_content() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Write a known knowledge file
    fs::write(
        team_repo.join("knowledge/test-file.md"),
        "# Test Content\nHello world\n",
    )
    .unwrap();

    bm_run(tmp.path(), &["knowledge", "show", "knowledge/test-file.md"]);
}

#[test]
fn knowledge_show_file_not_found() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let stderr = bm_run_fail(tmp.path(), &["knowledge", "show", "knowledge/nonexistent.md"]);
    assert!(stderr.contains("File not found"), "Got: {}", stderr);
}

#[test]
fn knowledge_show_path_outside_knowledge_dir() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "test-team", "scrum");

    let stderr = bm_run_fail(tmp.path(), &["knowledge", "show", "botminter.yml"]);
    assert!(
        stderr.contains("not within a knowledge or invariant directory"),
        "Got: {}",
        stderr
    );
}

#[test]
fn knowledge_v1_team_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // Downgrade schema to v1
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: 0.1");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: downgrade to v1"]);

    let stderr = bm_run_fail(tmp.path(), &["knowledge", "list"]);
    assert!(stderr.contains("requires schema 1.0"), "Got: {}", stderr);
    assert!(stderr.contains("bm upgrade"), "Got: {}", stderr);
}

// ── Schema init tests ─────────────────────────────────────────────

#[test]
fn init_creates_skills_and_formations_dirs() {
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
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let formations = bm::formation::list_formations(&team_repo).unwrap();
    assert!(formations.contains(&"local".to_string()));
    assert!(formations.contains(&"k8s".to_string()));
}

#[test]
fn formation_load_local() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    let config = bm::formation::load(&team_repo, "local").unwrap();
    assert_eq!(config.name, "local");
    assert!(config.is_local());
}

#[test]
fn formation_resolve_default() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "scrum");

    // No flag → default to local (formations dir exists)
    let result = bm::formation::resolve_formation(&team_repo, None).unwrap();
    assert_eq!(result, Some("local".to_string()));
}

#[test]
fn formation_v1_gate_blocks_non_default() {
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
        let _ = bm_cmd(&self.home)
            .args(["daemon", "stop", "-t", &self.team_name])
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
    setup_team(tmp.path(), "daemon-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-test");

    let port = get_free_port();
    let output = bm_run(
        tmp.path(),
        &["daemon", "start", "--mode", "poll", "--port", &port.to_string(), "-t", "daemon-test"],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
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
    setup_team(tmp.path(), "daemon-status-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-status-test");

    let port = get_free_port();
    // Start daemon
    bm_run(
        tmp.path(),
        &["daemon", "start", "--mode", "poll", "--port", &port.to_string(), "-t", "daemon-status-test"],
    );

    // Check status
    let output = bm_run(tmp.path(), &["daemon", "status", "-t", "daemon-status-test"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("running"), "Should show running: {}", stdout);
    assert!(stdout.contains("poll"), "Should show mode: {}", stdout);
}

#[test]
fn daemon_stop_cleans_up() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-stop-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-stop-test");

    let port = get_free_port();
    // Start daemon
    bm_run(
        tmp.path(),
        &["daemon", "start", "--mode", "poll", "--port", &port.to_string(), "-t", "daemon-stop-test"],
    );

    // Stop daemon
    let output = bm_run(tmp.path(), &["daemon", "stop", "-t", "daemon-stop-test"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
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
    setup_team(tmp.path(), "daemon-dup-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-dup-test");

    let port = get_free_port();
    // Start daemon
    bm_run(
        tmp.path(),
        &["daemon", "start", "--mode", "poll", "--port", &port.to_string(), "-t", "daemon-dup-test"],
    );

    // Try starting again (same port doesn't matter — PID file check catches it first)
    let stderr = bm_run_fail(
        tmp.path(),
        &["daemon", "start", "--mode", "poll", "--port", &port.to_string(), "-t", "daemon-dup-test"],
    );
    assert!(stderr.contains("already running"), "Should say already running: {}", stderr);
}

#[test]
fn daemon_stop_not_running_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-norun-test", "scrum");

    let stderr = bm_run_fail(tmp.path(), &["daemon", "stop", "-t", "daemon-norun-test"]);
    assert!(
        stderr.contains("not running") || stderr.contains("not found"),
        "Should indicate not running: {}",
        stderr
    );
}

#[test]
fn daemon_status_not_running() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-nostat-test", "scrum");

    let output = bm_run(tmp.path(), &["daemon", "status", "-t", "daemon-nostat-test"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("not running"), "Should say not running: {}", stdout);
}

#[test]
fn daemon_v1_team_blocked() {
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "daemon-v1-test", "scrum");

    // Downgrade to v1
    let manifest_path = team_repo.join("botminter.yml");
    let mut content = fs::read_to_string(&manifest_path).unwrap();
    content = content.replace("schema_version: '1.0'", "schema_version: 0.1");
    fs::write(&manifest_path, content).unwrap();
    git(&team_repo, &["add", "botminter.yml"]);
    git(&team_repo, &["commit", "-m", "chore: downgrade to v1"]);

    let stderr = bm_run_fail(tmp.path(), &["daemon", "start", "-t", "daemon-v1-test"]);
    assert!(stderr.contains("requires schema 1.0"), "Got: {}", stderr);
}

#[test]
fn daemon_webhook_mode_starts_and_stops() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-wh-test", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-test");

    let port = get_free_port();

    // Start in webhook mode on an OS-assigned free port
    let output = bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-test",
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("Daemon started"), "Should show started: {}", stdout);

    // Wait for server to bind (polling instead of fixed sleep)
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Webhook server should bind to port {} within 5s",
        port
    );

    // Check status shows webhook mode
    let status_out = bm_run(tmp.path(), &["daemon", "status", "-t", "daemon-wh-test"]);

    let status_stdout = String::from_utf8_lossy(&status_out.stdout);
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
    assert!(stdout.contains("--bind"), "Help should mention --bind: {}", stdout);
}

// ── Webhook endpoint tests ───────────────────────────────────────────

#[test]
fn daemon_webhook_accepts_relevant_event() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-wh-accept", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-accept");

    let port = get_free_port();

    // Start webhook daemon
    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-accept",
        ],
    );

    // Wait for server to be ready (polling)
    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Webhook server should be ready on port {}",
        port
    );

    // Send a relevant event via HTTP POST
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/webhook", port))
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
    setup_team(tmp.path(), "daemon-wh-reject", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-reject");

    let port = get_free_port();

    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-reject",
        ],
    );

    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Webhook server should be ready on port {}",
        port
    );

    // Send an irrelevant event (push) — daemon should accept but not trigger members
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/webhook", port))
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
    setup_team(tmp.path(), "daemon-wh-404", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-wh-404");

    let port = get_free_port();

    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-wh-404",
        ],
    );

    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Webhook server should be ready on port {}",
        port
    );

    // Send to wrong path
    let client = reqwest::blocking::Client::new();
    let resp = client
        .post(format!("http://127.0.0.1:{}/wrong-path", port))
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

// ── Health endpoint tests ────────────────────────────────────────────

#[test]
fn daemon_health_endpoint_returns_ok() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "daemon-health", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "daemon-health");

    let port = get_free_port();

    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "daemon-health",
        ],
    );

    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Server should be ready on port {}",
        port
    );

    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/health", port))
        .send()
        .expect("Failed to GET /health");

    assert_eq!(resp.status().as_u16(), 200, "Health should return 200");

    let body: serde_json::Value = resp.json().expect("Health response should be valid JSON");
    assert_eq!(body["ok"], true, "Health response should have ok:true");
    assert!(
        body["version"].is_string(),
        "Health response should include version string: {:?}",
        body
    );
}

// ── Projects sync tests ──────────────────────────────────────────────

#[test]
fn projects_sync_fails_without_github_repo() {
    let tmp = tempfile::tempdir().unwrap();
    // setup_team creates a team with empty github_repo
    setup_team(tmp.path(), "test-team", "scrum");

    // projects sync should fail because there's no github_repo configured
    let stderr = bm_run_fail(tmp.path(), &["projects", "sync"]);
    assert!(
        !stderr.is_empty(),
        "sync should fail without github_repo"
    );
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
    let tmp = tempfile::tempdir().unwrap();
    let team_repo = setup_team(tmp.path(), "test-team", "agentic-sdlc-minimal");

    // Parse the extracted manifest
    let content = fs::read_to_string(team_repo.join("botminter.yml")).unwrap();
    let manifest: profile::ProfileManifest = serde_yml::from_str(&content).unwrap();

    assert!(!manifest.views.is_empty(), "agentic-sdlc-minimal profile should have views");

    // The Engineer view should resolve to eng:* statuses + done + error
    let eng_view = manifest
        .views
        .iter()
        .find(|v| v.name == "Engineer")
        .expect("should have an Engineer view");

    let resolved = eng_view.resolve_statuses(&manifest.statuses);
    assert!(
        resolved.iter().any(|s| s == "eng:po:triage"),
        "Engineer view should include eng:po:triage"
    );
    assert!(
        resolved.iter().any(|s| s == "done"),
        "Engineer view should include done"
    );
    assert!(
        !resolved.iter().any(|s| s.starts_with("snt:")),
        "Engineer view should NOT include snt:* statuses"
    );

    // Filter string should start with "status:"
    let filter = eng_view.filter_string(&manifest.statuses);
    assert!(
        filter.starts_with("status:"),
        "filter should start with 'status:', got: {}",
        filter
    );
    assert!(
        filter.contains("eng:po:triage"),
        "filter should contain eng:po:triage, got: {}",
        filter
    );
}

// ── Teams list enrichment tests ──────────────────────────────────────

#[test]
fn teams_list_shows_member_and_project_counts() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "count-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    // Hire two members
    bm_hire(tmp.path(), role, "alice", "count-team");
    bm_hire(tmp.path(), role, "bob", "count-team");

    // Add a project
    let fork = create_fake_fork(tmp.path(), "test-proj");
    bm_add_project(tmp.path(), &fork, "count-team");

    // Verify teams list via subprocess
    let output = bm_run(tmp.path(), &["teams", "list"]);

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Table should have Members and Projects columns
    assert!(
        stdout.contains("Members"),
        "teams list should have Members column, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Projects"),
        "teams list should have Projects column, output:\n{}",
        stdout
    );
    // Should show count 2 for members and 1 for project
    assert!(
        stdout.contains("2"),
        "should show 2 members, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("1"),
        "should show 1 project, output:\n{}",
        stdout
    );
}

// ── Teams show tests ─────────────────────────────────────────────────

#[test]
fn teams_show_displays_full_details() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "show-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    bm_hire(tmp.path(), role, "alice", "show-team");

    let fork = create_fake_fork(tmp.path(), "my-project");
    bm_add_project(tmp.path(), &fork, "show-team");

    let output = bm_run(tmp.path(), &["teams", "show", "-t", "show-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("show-team"),
        "should show team name, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("scrum"),
        "should show profile, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Members"),
        "should have Members section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&format!("{}-alice", role)),
        "should show member, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("my-project"),
        "should show project, output:\n{}",
        stdout
    );
    // New: coding agent and profile source should be shown
    assert!(
        stdout.contains("Coding Agent"),
        "should show coding agent, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Profile Source"),
        "should show profile source, output:\n{}",
        stdout
    );
}

// ── Members show tests ───────────────────────────────────────────────

#[test]
fn members_show_displays_details() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "mshow-team", "scrum");

    let profiles_path = profile::profiles_dir_for(tmp.path());
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    bm_hire(tmp.path(), role, "alice", "mshow-team");

    let member_name = format!("{}-alice", role);
    let output = bm_run(tmp.path(), &["members", "show", &member_name, "-t", "mshow-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains(&member_name),
        "should show member name, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains(role),
        "should show role, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("stopped"),
        "should show stopped status, output:\n{}",
        stdout
    );
    // New: coding agent should be shown
    assert!(
        stdout.contains("Coding Agent"),
        "should show coding agent, output:\n{}",
        stdout
    );
}

#[test]
fn members_show_nonexistent_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "mshow-err-team", "scrum");

    let stderr = bm_run_fail(
        tmp.path(),
        &["members", "show", "nonexistent-member", "-t", "mshow-err-team"],
    );
    assert!(
        stderr.contains("not found"),
        "should say not found, stderr:\n{}",
        stderr
    );
}

// ── Projects list tests ──────────────────────────────────────────────

#[test]
fn projects_list_displays_table() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "plist-team", "scrum");

    let fork = create_fake_fork(tmp.path(), "my-app");
    bm_add_project(tmp.path(), &fork, "plist-team");

    let output = bm_run(tmp.path(), &["projects", "list", "-t", "plist-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("my-app"),
        "should show project name, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Fork URL"),
        "should have Fork URL column, output:\n{}",
        stdout
    );
}

#[test]
fn projects_list_empty_shows_guidance() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "plist-empty-team", "scrum");

    let output = bm_run(tmp.path(), &["projects", "list", "-t", "plist-empty-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No projects configured"),
        "should show guidance message, output:\n{}",
        stdout
    );
}

// ── Projects show tests ─────────────────────────────────────────────

#[test]
fn projects_show_displays_details() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "pshow-team", "scrum");

    let fork = create_fake_fork(tmp.path(), "my-lib");
    bm_add_project(tmp.path(), &fork, "pshow-team");

    let output = bm_run(tmp.path(), &["projects", "show", "my-lib", "-t", "pshow-team"]);

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("my-lib"),
        "should show project name, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Fork URL"),
        "should show fork URL label, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Knowledge"),
        "should show knowledge section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Invariants"),
        "should show invariants section, output:\n{}",
        stdout
    );
}

// ── Chat command tests ───────────────────────────────────────────────

/// Creates a minimal workspace for testing `bm chat`.
///
/// Sets up: team repo with a hired member, and a workspace directory containing
/// `.botminter.workspace`, `ralph.yml`, and `PROMPT.md`.
fn setup_chat_workspace(tmp: &Path, team_name: &str) -> (String, String) {
    let team_repo = setup_team(tmp, team_name, "scrum");

    let profiles_path = profile::profiles_dir_for(tmp);
    let roles = profile::list_roles_from("scrum", &profiles_path).unwrap();
    let role = &roles[0];

    // Hire a member
    bm_hire(tmp, role, "alice", team_name);
    let member_name = format!("{}-alice", role);

    // Create a workspace directory with required files
    let team_dir = tmp.join("workspaces").join(team_name);
    let ws_path = team_dir.join(&member_name);
    fs::create_dir_all(&ws_path).unwrap();

    // Workspace marker
    fs::write(ws_path.join(".botminter.workspace"), "").unwrap();

    // Copy ralph.yml from the member dir in team repo
    let member_ralph = team_repo.join("members").join(&member_name).join("ralph.yml");
    fs::copy(&member_ralph, ws_path.join("ralph.yml")).unwrap();

    // Copy PROMPT.md from the member dir in team repo
    let member_prompt = team_repo.join("members").join(&member_name).join("PROMPT.md");
    fs::copy(&member_prompt, ws_path.join("PROMPT.md")).unwrap();

    (member_name, role.clone())
}

#[test]
fn chat_render_system_prompt_outputs_meta_prompt() {
    let tmp = tempfile::tempdir().unwrap();
    let (member_name, role) = setup_chat_workspace(tmp.path(), "chat-team");

    let output = bm_run(
        tmp.path(),
        &[
            "chat",
            &member_name,
            "-t",
            "chat-team",
            "--render-system-prompt",
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Verify meta-prompt structure
    let display_name = member_name.replace(&format!("{}-", role), "");
    assert!(
        stdout.contains(&format!("# Interactive Session — {}", display_name)),
        "should contain member display name in header, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains(&display_name),
        "should contain member name (alice), output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("## Your Capabilities"),
        "should contain capabilities section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("## Guardrails"),
        "should contain guardrails section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("## Role Context"),
        "should contain role context section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("## Reference: Operation Mode"),
        "should contain reference section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("team/ralph-prompts/reference/"),
        "should reference ralph-prompts via team/ submodule path, output:\n{}",
        stdout
    );
}

#[test]
fn chat_render_system_prompt_with_hat_filter() {
    let tmp = tempfile::tempdir().unwrap();
    let (member_name, _role) = setup_chat_workspace(tmp.path(), "chat-hat-team");

    // Get a valid hat name from the member's ralph.yml
    let team_dir = tmp.path().join("workspaces").join("chat-hat-team");
    let ws_path = team_dir.join(&member_name);
    let ralph_content = fs::read_to_string(ws_path.join("ralph.yml")).unwrap();

    // Parse to find first hat name
    let ralph_val: serde_yml::Value = serde_yml::from_str(&ralph_content).unwrap();
    let hats = ralph_val["hats"].as_mapping().unwrap();
    let first_hat_name = hats.keys().next().unwrap().as_str().unwrap().to_string();

    // Run with --hat filter
    let output = bm_run(
        tmp.path(),
        &[
            "chat",
            &member_name,
            "-t",
            "chat-hat-team",
            "--hat",
            &first_hat_name,
            "--render-system-prompt",
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);

    // Hat-specific mode: should have capabilities but NOT list all hats as H3
    assert!(
        stdout.contains("## Your Capabilities"),
        "should contain capabilities section, output:\n{}",
        stdout
    );

    // In hat-specific mode, the hat name should NOT appear as an H3 heading
    // (that's the hatless mode pattern). Instead, the instructions are directly under ## Your Capabilities.
    assert!(
        !stdout.contains(&format!("### {}", first_hat_name)),
        "hat-specific mode should NOT have hat as H3 subsection, output:\n{}",
        stdout
    );
}

#[test]
fn chat_nonexistent_member_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "chat-err-team", "scrum");

    let stderr = bm_run_fail(
        tmp.path(),
        &[
            "chat",
            "nonexistent-member",
            "-t",
            "chat-err-team",
        ],
    );
    assert!(
        stderr.contains("not found"),
        "should say member not found, stderr:\n{}",
        stderr
    );
}

#[test]
fn chat_without_workspace_errors() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "chat-nows-team", "scrum");

    // Do NOT hire the member — "nonexistent-bob" is not in the team repo's members dir.
    // The daemon will create a session workspace, but prepare_chat_session_from_path
    // will fail because the member doesn't exist in the team repo.
    let stderr = bm_run_fail(
        tmp.path(),
        &[
            "chat",
            "nonexistent-bob",
            "-t",
            "chat-nows-team",
        ],
    );
    assert!(
        stderr.contains("nonexistent-bob") || stderr.contains("not found") || stderr.contains("Member"),
        "should mention member not found, stderr:\n{}",
        stderr
    );
}

// ── bm attach on local formation ────────────────────────────────────

#[test]
fn attach_local_formation_returns_not_applicable() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "attach-local-team", "scrum");

    let stderr = bm_run_fail(
        tmp.path(),
        &["attach", "-t", "attach-local-team"],
    );
    assert!(
        stderr.contains("not applicable") || stderr.contains("already in the local environment"),
        "bm attach on local formation should say 'not applicable', got: {}",
        stderr
    );
}

// ── bm init --non-interactive tests ──────────────────────────────────

#[test]
fn init_non_interactive_creates_team_with_skip_github() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let workzone = home.join("workzone");

    let output = bm_cmd(home)
        .args([
            "init",
            "--non-interactive",
            "--profile",
            "agentic-sdlc-minimal",
            "--team-name",
            "test-team",
            "--org",
            "test-org",
            "--repo",
            "test-repo",
            "--skip-github",
            "--workzone",
            &workzone.to_string_lossy(),
        ])
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("failed to run bm init --non-interactive");

    assert!(
        output.status.success(),
        "bm init --non-interactive should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify config.yml was written
    let config_path = home
        .join(".botminter")
        .join("config.yml");
    assert!(
        config_path.exists(),
        "config.yml should exist at {}",
        config_path.display()
    );

    // Verify team directory structure
    let team_dir = workzone.join("test-team");
    assert!(team_dir.exists(), "Team directory should exist");

    let team_repo = team_dir.join("team");
    assert!(team_repo.exists(), "Team repo should exist");

    // Verify botminter.yml exists in team repo
    assert!(
        team_repo.join("botminter.yml").exists(),
        "botminter.yml should exist in team repo"
    );

    // Verify git repo was initialized
    assert!(
        team_repo.join(".git").is_dir(),
        "Team repo should be a git repository"
    );

    // Verify profiles were extracted with botminter.yml manifest
    // profiles_dir uses dirs::config_dir() -> XDG_CONFIG_HOME or HOME/.config
    let profiles_dir = home
        .join(".config")
        .join("botminter")
        .join("profiles");
    assert!(
        profiles_dir.join("agentic-sdlc-minimal").join("botminter.yml").exists(),
        "botminter.yml should exist in extracted profile directory"
    );

    // Verify roles/ directory exists in extracted profiles (not stale members/ layout)
    let scrum_compact_roles = profiles_dir.join("agentic-sdlc-minimal").join("roles");
    assert!(
        scrum_compact_roles.is_dir(),
        "roles/ directory should exist in extracted profile at {}",
        scrum_compact_roles.display()
    );

    // Verify config contains the team entry
    let config_content = fs::read_to_string(&config_path).unwrap();
    assert!(
        config_content.contains("test-team"),
        "config should contain team name"
    );
    assert!(
        config_content.contains("test-org/test-repo"),
        "config should contain github repo"
    );
}

#[test]
fn init_non_interactive_fails_without_required_args() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    // Missing --profile
    let stderr = bm_run_fail(
        home,
        &[
            "init",
            "--non-interactive",
            "--team-name",
            "test-team",
            "--org",
            "test-org",
            "--repo",
            "test-repo",
            "--skip-github",
        ],
    );
    assert!(
        stderr.contains("--profile is required"),
        "Should error about missing --profile, got: {}",
        stderr
    );
}

#[test]
fn init_non_interactive_fails_on_invalid_profile() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();

    let stderr = bm_run_fail(
        home,
        &[
            "init",
            "--non-interactive",
            "--profile",
            "nonexistent-profile",
            "--team-name",
            "test-team",
            "--org",
            "test-org",
            "--repo",
            "test-repo",
            "--skip-github",
            "--workzone",
            &tmp.path().join("wz").to_string_lossy(),
        ],
    );
    assert!(
        stderr.contains("not found"),
        "Should error about invalid profile, got: {}",
        stderr
    );
}

#[test]
fn init_non_interactive_fails_on_duplicate_team_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let workzone = home.join("workzone");

    // First init should succeed
    let output = bm_cmd(home)
        .args([
            "init",
            "--non-interactive",
            "--profile",
            "agentic-sdlc-minimal",
            "--team-name",
            "dup-team",
            "--org",
            "test-org",
            "--repo",
            "test-repo",
            "--skip-github",
            "--workzone",
            &workzone.to_string_lossy(),
        ])
        .env("GIT_AUTHOR_NAME", "Test")
        .env("GIT_AUTHOR_EMAIL", "test@test.com")
        .env("GIT_COMMITTER_NAME", "Test")
        .env("GIT_COMMITTER_EMAIL", "test@test.com")
        .output()
        .expect("failed to run first init");
    assert!(
        output.status.success(),
        "First init should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Second init with same team name should fail
    let stderr = bm_run_fail(
        home,
        &[
            "init",
            "--non-interactive",
            "--profile",
            "agentic-sdlc-minimal",
            "--team-name",
            "dup-team",
            "--org",
            "test-org",
            "--repo",
            "test-repo2",
            "--skip-github",
            "--workzone",
            &workzone.to_string_lossy(),
        ],
    );
    assert!(
        stderr.contains("already exists"),
        "Should error about existing directory, got: {}",
        stderr
    );
}

// ── Bridge CLI tests ─────────────────────────────────────────────────

/// Sets up a minimal team with a stub bridge for bridge CLI tests.
/// Returns (home_dir, team_name, workzone_path, team_dir_path).
fn setup_bridge_test() -> (tempfile::TempDir, String, PathBuf, PathBuf) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let team_name = "bridge-team";
    let workzone = home.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    // Create team repo directory structure
    fs::create_dir_all(&team_repo).unwrap();

    // Create botminter.yml with bridge configured
    fs::write(
        team_repo.join("botminter.yml"),
        "schema_version: '1.0'\nprofile: agentic-sdlc-minimal\nbridge: stub\n",
    )
    .unwrap();

    // Copy stub bridge fixture into team repo
    let stub_src = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join(".planning")
        .join("specs")
        .join("bridge")
        .join("examples")
        .join("stub");
    let stub_dst = team_repo.join("bridges").join("stub");
    fs::create_dir_all(&stub_dst).unwrap();
    for entry in fs::read_dir(&stub_src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = stub_dst.join(entry.file_name());
        fs::copy(&src_path, &dst_path).unwrap();
    }

    // Create config
    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir.clone(),
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        }],
        vms: Vec::new(),
        keyring_collection: None,
    };

    let config_path = home.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    (tmp, team_name.to_string(), workzone, team_dir)
}

/// Sets up a team without any bridge configured.
fn setup_no_bridge_test() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let team_name = "nobridge-team";
    let workzone = home.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();
    fs::write(
        team_repo.join("botminter.yml"),
        "schema_version: \"1.0.0\"\nprofile: agentic-sdlc-minimal\n",
    )
    .unwrap();

    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        }],
        vms: Vec::new(),
        keyring_collection: None,
    };

    let config_path = home.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    (tmp, team_name.to_string())
}

/// Sets up a team with an external bridge (no lifecycle).
fn setup_external_bridge_test() -> (tempfile::TempDir, String) {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    let team_name = "ext-team";
    let workzone = home.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");

    fs::create_dir_all(&team_repo).unwrap();
    fs::write(
        team_repo.join("botminter.yml"),
        "schema_version: \"1.0.0\"\nprofile: agentic-sdlc-minimal\nbridge: telegram\n",
    )
    .unwrap();

    let bridge_dir = team_repo.join("bridges").join("telegram");
    fs::create_dir_all(&bridge_dir).unwrap();
    fs::write(
        bridge_dir.join("bridge.yml"),
        r#"apiVersion: botminter.dev/v1alpha1
kind: Bridge
metadata:
  name: telegram
  displayName: "Telegram"
spec:
  type: external
  configSchema: schema.json
  identity:
    onboard: onboard
    rotate-credentials: rotate
    remove: remove
  configDir: "$BRIDGE_CONFIG_DIR"
"#,
    )
    .unwrap();

    // Create a minimal Justfile for identity commands
    fs::write(
        bridge_dir.join("Justfile"),
        r#"onboard username:
    @mkdir -p "$BRIDGE_CONFIG_DIR"
    @echo '{"username": "{{username}}", "user_id": "tg-id", "token": "tg-token"}' > "$BRIDGE_CONFIG_DIR/config.json"

rotate username:
    @mkdir -p "$BRIDGE_CONFIG_DIR"
    @echo '{"username": "{{username}}", "user_id": "tg-id", "token": "tg-rotated-token"}' > "$BRIDGE_CONFIG_DIR/config.json"

remove username:
    @echo "telegram: removed {{username}}" >&2
"#,
    )
    .unwrap();

    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir,
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        }],
        vms: Vec::new(),
        keyring_collection: None,
    };

    let config_path = home.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    (tmp, team_name.to_string())
}

/// Reads and parses bridge-state.json from the team directory.
fn read_bridge_state(team_dir: &Path) -> bm::bridge::BridgeState {
    let state_path = team_dir.join("bridge-state.json");
    bm::bridge::load_state(&state_path).unwrap()
}

#[test]
fn bridge_start() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    let output = bm_run(home, &["bridge", "start"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("started"),
        "Should print started message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "running");
    assert!(state.service_url.is_some(), "Should have service_url");
    assert!(state.started_at.is_some(), "Should have started_at");
    assert_eq!(state.bridge_name.as_deref(), Some("stub"));
}

#[test]
fn bridge_stop() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Start first
    bm_run(home, &["bridge", "start"]);
    // Then stop
    let output = bm_run(home, &["bridge", "stop"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stopped"),
        "Should print stopped message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "stopped");
    assert!(state.started_at.is_none(), "started_at should be cleared");
}

#[test]
fn bridge_status() {
    let (tmp, _team_name, _workzone, _team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Start first so there's something to report
    bm_run(home, &["bridge", "start"]);

    let output = bm_run(home, &["bridge", "status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("stub"),
        "Should show bridge name, got: {}",
        stdout
    );
    assert!(
        stdout.contains("running"),
        "Should show running status, got: {}",
        stdout
    );
}

#[test]
fn bridge_identity_add() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    let output = bm_run(home, &["bridge", "identity", "add", "testuser"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("testuser"),
        "Should mention username, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert!(state.identities.contains_key("testuser"));
    let identity = state.identities.get("testuser").unwrap();
    assert_eq!(identity.username, "testuser");
    assert_eq!(identity.user_id, "stub-id");
    // Token is now stored in keyring, not in bridge-state.json
    assert!(identity.token.is_none(), "Token should not be stored in bridge-state.json");
}

#[test]
fn bridge_identity_rotate() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Add first
    bm_run(home, &["bridge", "identity", "add", "testuser"]);
    // Rotate
    let output = bm_run(home, &["bridge", "identity", "rotate", "testuser"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("rotated"),
        "Should print rotated message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    let identity = state.identities.get("testuser").unwrap();
    // Token is now stored in keyring, not in bridge-state.json
    assert!(
        identity.token.is_none(),
        "Token should not be stored in bridge-state.json after rotation"
    );
}

#[test]
fn bridge_identity_list() {
    let (tmp, _team_name, _workzone, _team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Add first
    bm_run(home, &["bridge", "identity", "add", "testuser"]);
    // List
    let output = bm_run(home, &["bridge", "identity", "list"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("testuser"),
        "Should list the username, got: {}",
        stdout
    );
}

#[test]
fn bridge_identity_remove() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Add first
    bm_run(home, &["bridge", "identity", "add", "testuser"]);
    // Remove
    let output = bm_run(home, &["bridge", "identity", "remove", "testuser"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("removed"),
        "Should print removed message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert!(
        !state.identities.contains_key("testuser"),
        "Identity should be removed"
    );
}

#[test]
fn bridge_room_create() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    let output = bm_run(home, &["bridge", "room", "create", "general"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("general"),
        "Should mention room name, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert!(!state.rooms.is_empty(), "Should have at least one room");
    assert_eq!(state.rooms[0].name, "general");
    assert_eq!(state.rooms[0].room_id.as_deref(), Some("stub-room-id"));
}

#[test]
fn bridge_room_list() {
    let (tmp, _team_name, _workzone, _team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Create first
    bm_run(home, &["bridge", "room", "create", "general"]);
    // List
    let output = bm_run(home, &["bridge", "room", "list"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("general"),
        "Should list room name, got: {}",
        stdout
    );
}

#[test]
fn bridge_external_start() {
    let (tmp, _team_name) = setup_external_bridge_test();
    let home = tmp.path();

    let output = bm_run(home, &["bridge", "start"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("external"),
        "Should mention external, got: {}",
        stdout
    );
    assert!(
        stdout.contains("managed externally"),
        "Should say managed externally, got: {}",
        stdout
    );
}

#[test]
fn bridge_no_bridge() {
    let (tmp, _team_name) = setup_no_bridge_test();
    let home = tmp.path();

    let output = bm_run(home, &["bridge", "start"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("No bridge configured"),
        "Should say no bridge configured, got: {}",
        stdout
    );
}

// ── Bridge lifecycle integration (bm start/stop with bridge) ──────────

#[test]
fn start_bridge_only() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Extract profiles so schema version check passes
    bm_run(home, &["profiles", "init", "--force"]);

    // --bridge-only should start bridge without launching members
    let output = bm_run(home, &["start", "--bridge-only"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Bridge 'stub' started"),
        "Should show bridge started message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "running");
    assert_eq!(state.bridge_name.as_deref(), Some("stub"));
}

#[test]
fn start_no_bridge_flag() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Extract profiles so schema version check passes
    bm_run(home, &["profiles", "init", "--force"]);

    // --no-bridge --bridge-only: skip bridge (no-bridge takes precedence), then return early
    let output = bm_run(home, &["start", "--no-bridge", "--bridge-only"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Bridge"),
        "Should not mention bridge with --no-bridge, got: {}",
        stdout
    );

    // Bridge state should not exist or be default
    let state = read_bridge_state(&team_dir);
    assert_eq!(
        state.status, "unknown",
        "Bridge state should be default (unknown) when --no-bridge used"
    );
}

#[test]
fn stop_leaves_bridge_running_by_default() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Extract profiles so schema version check passes
    bm_run(home, &["profiles", "init", "--force"]);

    // Start bridge first
    bm_run(home, &["start", "--bridge-only"]);
    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "running");

    // Stop without --bridge should leave bridge running and print hint
    let output = bm_run(home, &["stop"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("left running"),
        "Should show bridge left-running hint, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "running", "Bridge should still be running");
}

#[test]
fn stop_bridge_flag_stops_bridge() {
    let (tmp, _team_name, _workzone, team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Extract profiles so schema version check passes
    bm_run(home, &["profiles", "init", "--force"]);

    // Start bridge first
    bm_run(home, &["start", "--bridge-only"]);
    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "running");

    // Stop with --bridge should stop bridge
    let output = bm_run(home, &["stop", "--bridge"]);
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Bridge 'stub' stopped"),
        "Should show bridge stopped message, got: {}",
        stdout
    );

    let state = read_bridge_state(&team_dir);
    assert_eq!(state.status, "stopped");
}

#[test]
fn status_shows_bridge() {
    let (tmp, _team_name, _workzone, _team_dir) = setup_bridge_test();
    let home = tmp.path();

    // Extract profiles so schema version check passes
    bm_run(home, &["profiles", "init", "--force"]);

    // Add an identity to have something to display
    bm_run(home, &["bridge", "identity", "add", "testbot"]);

    // Start bridge so status is "running"
    bm_run(home, &["start", "--bridge-only"]);

    // Create members dir so status doesn't bail
    let team_repo = tmp.path().join("workspaces").join("bridge-team").join("team");
    let members_dir = team_repo.join("members").join("dummy");
    fs::create_dir_all(&members_dir).unwrap();
    fs::write(members_dir.join("botminter.yml"), "role: dev\n").unwrap();

    let output = bm_run(home, &["status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("Bridge: stub"),
        "Status should show bridge name, got: {}",
        stdout
    );
    assert!(
        stdout.contains("running"),
        "Status should show bridge status, got: {}",
        stdout
    );
    assert!(
        stdout.contains("testbot"),
        "Status should show bridge identity mapping, got: {}",
        stdout
    );
}

#[test]
fn status_no_bridge_shows_normal() {
    let (tmp, _team_name) = setup_no_bridge_test();
    let home = tmp.path();

    // Create members dir so status doesn't bail early
    let team_repo = tmp.path().join("workspaces").join("nobridge-team").join("team");
    let members_dir = team_repo.join("members").join("dummy");
    fs::create_dir_all(&members_dir).unwrap();
    fs::write(members_dir.join("botminter.yml"), "role: dev\n").unwrap();

    let output = bm_run(home, &["status"]);
    let stdout = String::from_utf8_lossy(&output.stdout);

    // Should show normal team info without any bridge section
    assert!(
        !stdout.contains("Bridge:"),
        "Status should not show bridge section when no bridge configured, got: {}",
        stdout
    );
}

// ── Init bridge selection (non-interactive) ──────────────────────────

/// Sets up git global config for a test HOME so `bm init` can commit.
fn setup_git_config(home: &Path) {
    let git_config = home.join(".gitconfig");
    fs::write(
        &git_config,
        "[user]\n    email = test@botminter.test\n    name = BM Test\n",
    )
    .unwrap();
}

#[test]
fn init_bridge_records_in_manifest() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    setup_git_config(home);

    // Pre-populate profiles
    let profiles_path = profile::profiles_dir_for(home);
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm_cmd(home)
        .args([
            "init",
            "--non-interactive",
            "--profile", "agentic-sdlc-minimal",
            "--team-name", "bridge-init-test",
            "--org", "testorg",
            "--repo", "testrepo",
            "--bridge", "telegram",
            "--skip-github",
            "--workzone", &home.join("workspaces").to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init with --bridge should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    // Verify bridge key in team's botminter.yml
    let manifest_path = home
        .join("workspaces")
        .join("bridge-init-test")
        .join("team")
        .join("botminter.yml");
    let contents = fs::read_to_string(&manifest_path).unwrap();
    let value: serde_yml::Value = serde_yml::from_str(&contents).unwrap();
    assert_eq!(
        value.get("bridge").and_then(|v| v.as_str()),
        Some("telegram"),
        "botminter.yml should contain bridge: telegram after init, contents:\n{}",
        contents
    );
}

#[test]
fn init_no_bridge_has_no_bridge_key() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    setup_git_config(home);

    let profiles_path = profile::profiles_dir_for(home);
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm_cmd(home)
        .args([
            "init",
            "--non-interactive",
            "--profile", "agentic-sdlc-minimal",
            "--team-name", "no-bridge-test",
            "--org", "testorg",
            "--repo", "testrepo",
            "--skip-github",
            "--workzone", &home.join("workspaces").to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "init without --bridge should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let manifest_path = home
        .join("workspaces")
        .join("no-bridge-test")
        .join("team")
        .join("botminter.yml");
    let contents = fs::read_to_string(&manifest_path).unwrap();
    let value: serde_yml::Value = serde_yml::from_str(&contents).unwrap();
    assert!(
        value.get("bridge").is_none(),
        "botminter.yml should NOT contain bridge key without --bridge, contents:\n{}",
        contents
    );
}

#[test]
fn init_bridge_invalid_name_fails() {
    let tmp = tempfile::tempdir().unwrap();
    let home = tmp.path();
    setup_git_config(home);

    let profiles_path = profile::profiles_dir_for(home);
    fs::create_dir_all(&profiles_path).unwrap();
    profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm_cmd(home)
        .args([
            "init",
            "--non-interactive",
            "--profile", "agentic-sdlc-minimal",
            "--team-name", "bad-bridge-test",
            "--org", "testorg",
            "--repo", "testrepo",
            "--bridge", "nonexistent",
            "--skip-github",
            "--workzone", &home.join("workspaces").to_string_lossy(),
        ])
        .output()
        .unwrap();
    assert!(
        !output.status.success(),
        "init with invalid --bridge should fail"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("not found") || stderr.contains("telegram"),
        "Error should mention the bridge issue, stderr: {}",
        stderr
    );
}

// ── Console API smoke tests ──────────────────────────────────────────

#[test]
fn daemon_api_teams_endpoint() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "console-teams", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "console-teams");

    let port = get_free_port();

    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "console-teams",
        ],
    );

    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Server should be ready on port {}",
        port
    );

    // GET /api/teams should return the team list
    let client = reqwest::blocking::Client::new();
    let resp = client
        .get(format!("http://127.0.0.1:{}/api/teams", port))
        .send()
        .expect("Failed to GET /api/teams");

    assert_eq!(resp.status().as_u16(), 200, "/api/teams should return 200");

    let body: serde_json::Value = resp.json().expect("/api/teams should return valid JSON");
    let teams = body.as_array().expect("/api/teams should return a JSON array");
    assert!(!teams.is_empty(), "Teams array should not be empty");
    assert_eq!(
        teams[0]["name"], "console-teams",
        "First team should be 'console-teams'"
    );
    assert!(
        teams[0]["profile"].is_string(),
        "Team should have a profile field"
    );
}

#[test]
fn daemon_start_shows_console_url() {
    let tmp = tempfile::tempdir().unwrap();
    setup_team(tmp.path(), "console-url", "scrum");
    let _guard = DaemonGuard::new(tmp.path(), "console-url");

    let port = get_free_port();

    let output = bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", "console-url",
        ],
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    let expected_url = format!("Console: http://localhost:{}", port);
    assert!(
        stdout.contains(&expected_url),
        "Daemon start output should include console URL '{}', got: {}",
        expected_url,
        stdout
    );
}

/// Copies the fixture team-repo into a tempdir, git-inits it, and writes a config
/// pointing to it. Follows the production layout: team.path = team_dir,
/// team repo content at team_dir/team/ (where botminter.yml, members/ etc. live).
fn setup_fixture_team(tmp: &Path, team_name: &str) -> PathBuf {
    let fixture_base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../.agents/planning/2026-03-22-console-web-ui/fixture-gen/fixtures/team-repo");

    let workzone = tmp.join("workspaces");
    let team_dir = workzone.join(team_name);
    let team_repo = team_dir.join("team");
    fs::create_dir_all(&team_repo).unwrap();

    // Copy fixture into team_dir/team/ (production layout: botminter.yml lives here)
    copy_fixture_dir(&fixture_base, &team_repo);

    // Git init the team repo (required for file write API)
    git(&team_repo, &["init", "-b", "main"]);
    git(&team_repo, &["config", "user.email", "test@botminter.test"]);
    git(&team_repo, &["config", "user.name", "BM Test"]);
    git(&team_repo, &["add", "-A"]);
    git(&team_repo, &["commit", "-m", "feat: init fixture team repo"]);

    // Write config (team.path = team_dir, matching production)
    let config = BotminterConfig {
        workzone: workzone.clone(),
        default_team: Some(team_name.to_string()),
        teams: vec![TeamEntry {
            name: team_name.to_string(),
            path: team_dir.clone(),
            profile: "agentic-sdlc-minimal".to_string(),
            github_repo: String::new(),
            credentials: Credentials::default(),
            coding_agent: None,
            project_number: None,
            bridge_lifecycle: Default::default(),
            daemon: Default::default(),
            vm: None,
        }],
        vms: Vec::new(),
        keyring_collection: None,
    };
    let config_path = tmp.join(".botminter").join("config.yml");
    bm::config::save_to(&config_path, &config).unwrap();

    team_dir
}

fn copy_fixture_dir(src: &Path, dst: &Path) {
    fs::create_dir_all(dst).unwrap();
    for entry in fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let src_path = entry.path();
        let dst_path = dst.join(entry.file_name());
        if src_path.is_dir() {
            copy_fixture_dir(&src_path, &dst_path);
        } else {
            fs::copy(&src_path, &dst_path).unwrap();
        }
    }
}

/// E2E test: starts a daemon with fixture data and verifies all console API endpoints.
///
/// Uses real fixture team-repo with 3 members, knowledge, invariants, and workflows.
/// Single daemon instance, multiple endpoint checks — fast and realistic.
#[test]
fn daemon_console_api_e2e_with_fixtures() {
    let tmp = tempfile::tempdir().unwrap();
    let team_name = "fixture-team";
    let _team_dir = setup_fixture_team(tmp.path(), team_name);
    let _guard = DaemonGuard::new(tmp.path(), team_name);

    let port = get_free_port();

    bm_run(
        tmp.path(),
        &[
            "daemon", "start",
            "--mode", "webhook",
            "--port", &port.to_string(),
            "-t", team_name,
        ],
    );

    assert!(
        wait_for_port(port, Duration::from_secs(5)),
        "Server should be ready on port {}",
        port
    );

    let client = reqwest::blocking::Client::new();
    let base = format!("http://127.0.0.1:{}", port);

    // ── GET /api/teams ──────────────────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams"))
            .send()
            .expect("GET /api/teams");
        assert_eq!(resp.status().as_u16(), 200);
        let body: serde_json::Value = resp.json().unwrap();
        let teams = body.as_array().expect("should be array");
        assert_eq!(teams.len(), 1);
        assert_eq!(teams[0]["name"], team_name);
        assert_eq!(teams[0]["profile"], "agentic-sdlc-minimal");
    }

    // ── GET /api/teams/:team/overview ───────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/overview"))
            .send()
            .expect("GET overview");
        assert_eq!(resp.status().as_u16(), 200, "overview should return 200");
        let body: serde_json::Value = resp.json().unwrap();

        // Profile
        assert_eq!(body["profile"], "agentic-sdlc-minimal");
        assert_eq!(body["name"], team_name);

        // Members — fixture has 3
        let members = body["members"].as_array().expect("members array");
        assert_eq!(members.len(), 3, "fixture has 3 members");
        let member_names: Vec<&str> = members.iter()
            .map(|m| m["name"].as_str().unwrap())
            .collect();
        assert!(member_names.contains(&"superman-alice"));
        assert!(member_names.contains(&"superman-bob"));
        assert!(member_names.contains(&"chief-of-staff-mgr"));

        // Roles — fixture has 2 (superman, chief-of-staff)
        let roles = body["roles"].as_array().expect("roles array");
        assert_eq!(roles.len(), 2, "fixture has 2 roles");
        let role_names: Vec<&str> = roles.iter()
            .map(|r| r["name"].as_str().unwrap())
            .collect();
        assert!(role_names.contains(&"superman"));
        assert!(role_names.contains(&"chief-of-staff"));

        // Knowledge files — fixture has 3
        let knowledge = body["knowledge_files"].as_array().expect("knowledge_files");
        assert_eq!(knowledge.len(), 3, "fixture has 3 knowledge files");

        // Invariant files — fixture has 2
        let invariants = body["invariant_files"].as_array().expect("invariant_files");
        assert_eq!(invariants.len(), 2, "fixture has 2 invariant files");
    }

    // ── GET /api/teams/:team/members ────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/members"))
            .send()
            .expect("GET members");
        assert_eq!(resp.status().as_u16(), 200, "members should return 200");
        let body: serde_json::Value = resp.json().unwrap();
        let members = body.as_array().expect("members array");
        assert_eq!(members.len(), 3, "fixture has 3 members");

        // Find alice and verify her fields
        let alice = members.iter()
            .find(|m| m["name"] == "superman-alice")
            .expect("alice should exist");
        assert_eq!(alice["role"], "superman");
        assert_eq!(alice["hat_count"], 14, "alice has 14 hats");

        // Find mgr and verify
        let mgr = members.iter()
            .find(|m| m["name"] == "chief-of-staff-mgr")
            .expect("mgr should exist");
        assert_eq!(mgr["role"], "chief-of-staff");
        assert_eq!(mgr["hat_count"], 1, "mgr has 1 hat");
    }

    // ── GET /api/teams/:team/members/:name ──────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/members/superman-alice"))
            .send()
            .expect("GET member detail");
        assert_eq!(resp.status().as_u16(), 200, "member detail should return 200");
        let body: serde_json::Value = resp.json().unwrap();

        assert_eq!(body["name"], "superman-alice");
        assert_eq!(body["role"], "superman");

        // Has ralph_yml content
        assert!(body["ralph_yml"].is_string(), "should have ralph_yml");
        assert!(!body["ralph_yml"].as_str().unwrap().is_empty(), "ralph_yml not empty");

        // Has hats array with 14 entries
        let hats = body["hats"].as_array().expect("hats array");
        assert_eq!(hats.len(), 14, "alice has 14 hats");

        // Has invariant files (design-quality.md)
        let inv_files = body["invariant_files"].as_array().expect("invariant_files");
        assert!(
            inv_files.iter().any(|f| f.as_str().is_some_and(|s| s.contains("design-quality"))),
            "alice should have design-quality.md invariant"
        );
    }

    // ── GET /api/teams/:team/process ────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/process"))
            .send()
            .expect("GET process");
        assert_eq!(resp.status().as_u16(), 200, "process should return 200");
        let body: serde_json::Value = resp.json().unwrap();

        // Workflows — fixture has 4 DOT files
        let workflows = body["workflows"].as_array().expect("workflows array");
        assert!(
            !workflows.is_empty(),
            "process should have workflows from DOT files"
        );

        // Statuses — fixture botminter.yml has 28 statuses
        let statuses = body["statuses"].as_array().expect("statuses array");
        assert!(
            statuses.len() >= 20,
            "fixture has 28 statuses, got {}",
            statuses.len()
        );

        // markdown — fixture has PROCESS.md
        assert!(
            body["markdown"].is_string() && !body["markdown"].as_str().unwrap().is_empty(),
            "process should include PROCESS.md content"
        );
    }

    // ── GET /api/teams/:team/tree ───────────────────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/tree"))
            .send()
            .expect("GET tree");
        assert_eq!(resp.status().as_u16(), 200, "tree should return 200");
        let body: serde_json::Value = resp.json().unwrap();
        let entries = body["entries"].as_array().expect("entries array");

        let entry_names: Vec<&str> = entries.iter()
            .filter_map(|e| e["name"].as_str())
            .collect();
        assert!(entry_names.contains(&"members"), "tree should contain members/");
        assert!(entry_names.contains(&"knowledge"), "tree should contain knowledge/");
        assert!(entry_names.contains(&"invariants"), "tree should contain invariants/");
        assert!(entry_names.contains(&"workflows"), "tree should contain workflows/");
    }

    // ── GET /api/teams/:team/files/botminter.yml ────────────────
    {
        let resp = client
            .get(format!("{base}/api/teams/{team_name}/files/botminter.yml"))
            .send()
            .expect("GET file");
        assert_eq!(resp.status().as_u16(), 200, "file read should return 200");
        let body: serde_json::Value = resp.json().unwrap();

        assert!(
            body["content"].is_string(),
            "file response should have content"
        );
        let content = body["content"].as_str().unwrap();
        assert!(
            content.contains("agentic-sdlc-minimal"),
            "botminter.yml should contain profile name"
        );
        assert!(
            body["content_type"].as_str().is_some_and(|ct| ct.contains("yaml")),
            "content_type should indicate yaml"
        );
    }

    // ── GET / (Console HTML) ──────────────────────────────────────
    // Verifies the embedded SPA frontend is served at the root.
    // When console/build/ is not present, assets are empty and we get
    // a graceful 404 with "Console not built" — that's acceptable.
    #[cfg(feature = "console")]
    {
        let resp = client
            .get(format!("{base}/"))
            .send()
            .expect("GET /");
        let status = resp.status().as_u16();
        let body = resp.text().unwrap();
        if status == 404 && body.contains("Console not built") {
            // console/build/ not present — acceptable in dev builds
        } else {
            assert_eq!(status, 200, "root should return 200");
            assert!(
                body.contains("<!doctype html>") || body.contains("<!DOCTYPE html>"),
                "root should serve HTML, got: {}",
                &body[..body.len().min(200)]
            );
        }
    }

    // ── GET /teams/anything (SPA client-side route) ───────────────
    #[cfg(feature = "console")]
    {
        let resp = client
            .get(format!("{base}/teams/nonexistent"))
            .send()
            .expect("GET SPA route");
        let status = resp.status().as_u16();
        let body = resp.text().unwrap();
        if status == 404 && body.contains("Console not built") {
            // console/build/ not present — acceptable in dev builds
        } else {
            assert_eq!(status, 200, "SPA route should return 200");
            assert!(
                body.contains("<!doctype html>") || body.contains("<!DOCTYPE html>"),
                "SPA fallback should serve index.html, got: {}",
                &body[..body.len().min(200)]
            );
        }
    }
}
