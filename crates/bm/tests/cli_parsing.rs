//! CLI argument parsing tests for the `bm` binary.
//!
//! These tests exercise clap's argument definitions — aliases, flags,
//! required args, error messages, and help text — without requiring a real
//! team setup or HOME env mutation. They run fast and in parallel.

use std::path::Path;
use std::process::Command;

/// Helper: create a `Command` for the `bm` binary with HOME isolation.
fn bm(home: &Path) -> Command {
    let mut cmd = Command::new(env!("CARGO_BIN_EXE_bm"));
    cmd.env("HOME", home);
    cmd.env("XDG_CONFIG_HOME", home.join(".config"));
    cmd
}

/// Clap uses exit code 2 for argument parsing / usage errors.
/// Runtime errors (missing config, etc.) exit with 1 via anyhow.
const CLAP_PARSE_ERROR_CODE: i32 = 2;

// ── Command aliases (2 tests) ────────────────────────────────────────

#[test]
fn start_and_up_are_aliases() {
    let tmp = tempfile::tempdir().unwrap();
    let start = bm(tmp.path()).args(["start", "--help"]).output().unwrap();
    let up = bm(tmp.path()).args(["up", "--help"]).output().unwrap();

    assert!(start.status.success(), "bm start --help should exit 0");
    assert!(up.status.success(), "bm up --help should exit 0");

    let start_text = String::from_utf8_lossy(&start.stdout);
    let up_text = String::from_utf8_lossy(&up.stdout);

    assert_eq!(
        start_text, up_text,
        "start and up should produce identical help text"
    );
}

#[test]
fn aliases_shown_in_help() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["--help"]).output().unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);

    // The `start` command (which has `alias = "up"`) should appear in help.
    assert!(
        stdout.contains("start"),
        "help should list the 'start' command, output:\n{}",
        stdout
    );

    // Verify the alias resolves to a known command (not "unknown subcommand").
    // Note: `alias` (vs `visible_alias`) is hidden from --help output, so
    // we verify the alias works functionally rather than checking for "up" text.
    let up_help = bm(tmp.path()).args(["up", "--help"]).output().unwrap();
    assert!(
        up_help.status.success(),
        "'bm up --help' should succeed — alias resolves to start"
    );
}

// ── Flag parsing (5 tests) ───────────────────────────────────────────

#[test]
fn team_flag_short_and_long() {
    let tmp = tempfile::tempdir().unwrap();
    // Commands accepting -t/--team will fail at runtime (no config) but must
    // NOT fail at the argument-parsing stage. Exit code 2 = parse error,
    // exit code 1 = runtime error (parsing succeeded).
    let cases: &[&[&str]] = &[
        &["hire", "somerole", "-t", "myteam"],
        &["hire", "somerole", "--team", "myteam"],
        &["status", "-t", "myteam"],
        &["status", "--team", "myteam"],
        &["start", "-t", "myteam"],
        &["stop", "-t", "myteam"],
        &["members", "list", "-t", "myteam"],
        &["roles", "list", "-t", "myteam"],
        &["teams", "sync", "-t", "myteam"],
        &["projects", "add", "https://example.com", "-t", "myteam"],
    ];

    for args in cases {
        let output = bm(tmp.path()).args(*args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "command `bm {}` should not have a parsing error (exit 2), stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn force_flag_on_stop() {
    let tmp = tempfile::tempdir().unwrap();
    // --force and -f are both defined via #[arg(short, long)] on Stop
    for args in [
        vec!["stop", "--force"],
        vec!["stop", "-f"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn sync_repos_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "sync", "--repos"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams sync --repos` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sync_bridge_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "sync", "--bridge"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams sync --bridge` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sync_all_flag_long_and_short() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["teams", "sync", "--all"],
        vec!["teams", "sync", "-a"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn sync_repos_and_bridge_together() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "sync", "--repos", "--bridge"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams sync --repos --bridge` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn sync_push_flag_removed() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "sync", "--push"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "`bm teams sync --push` should be a parse error (flag removed)"
    );
}

#[test]
fn sync_no_flags_default() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "sync"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams sync` (no flags) should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verbose_flag_on_status() {
    let tmp = tempfile::tempdir().unwrap();
    // -v and --verbose are both defined via #[arg(short, long)] on Status
    for args in [
        vec!["status", "-v"],
        vec!["status", "--verbose"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn verbose_flag_on_sync() {
    let tmp = tempfile::tempdir().unwrap();
    // -v and --verbose are both defined via #[arg(short, long)] on Sync
    for args in [
        vec!["teams", "sync", "-v"],
        vec!["teams", "sync", "--verbose"],
        vec!["teams", "sync", "--repos", "-v"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn name_flag_on_hire() {
    let tmp = tempfile::tempdir().unwrap();
    // --name is defined with #[arg(long)] only (no short -n)
    let output = bm(tmp.path())
        .args(["hire", "somerole", "--name", "alice"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm hire somerole --name alice` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Required arguments (3 tests) ─────────────────────────────────────

#[test]
fn hire_requires_role_argument() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["hire"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm hire (no role) should exit with clap error code 2"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("<ROLE>") || stderr.contains("role") || stderr.contains("required"),
        "error should mention the missing role argument, stderr:\n{}",
        stderr
    );
}

#[test]
fn profiles_describe_requires_profile_name() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["profiles", "describe"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm profiles describe (no name) should exit with clap error code 2"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("<PROFILE>")
            || stderr.contains("profile")
            || stderr.contains("required"),
        "error should mention the missing profile argument, stderr:\n{}",
        stderr
    );
}

#[test]
fn projects_add_requires_url() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["projects", "add"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm projects add (no url) should exit with clap error code 2"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("<URL>") || stderr.contains("url") || stderr.contains("required"),
        "error should mention the missing URL argument, stderr:\n{}",
        stderr
    );
}

#[test]
fn projects_sync_help_works() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["projects", "sync", "--help"]).output().unwrap();
    assert!(
        output.status.success(),
        "bm projects sync --help should exit 0"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Sync") || stdout.contains("sync") || stdout.contains("Status"),
        "help should mention sync, stdout:\n{}",
        stdout
    );
}

// ── Unknown/invalid input (3 tests) ──────────────────────────────────

#[test]
fn unknown_subcommand_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["foobar"]).output().unwrap();
    assert!(
        !output.status.success(),
        "bm foobar should exit non-zero"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "stderr should contain a helpful error message"
    );
}

#[test]
fn unknown_flag_errors() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["status", "--foobar"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm status --foobar should exit with clap error code 2"
    );
}

#[test]
fn completions_requires_valid_shell() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["completions", "notashell"]).output().unwrap();
    assert!(
        !output.status.success(),
        "bm completions notashell should exit non-zero"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.is_empty(),
        "error should provide a helpful message about valid shells, stderr:\n{}",
        stderr
    );
}

// ── Help text (2 tests) ──────────────────────────────────────────────

#[test]
fn help_flag_shows_all_commands() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["--help"]).output().unwrap();
    assert!(output.status.success(), "bm --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let expected_commands = [
        "init",
        "hire",
        "chat",
        "start",
        "stop",
        "status",
        "teams",
        "members",
        "roles",
        "profiles",
        "projects",
        "bridge",
        "completions",
    ];

    for cmd in &expected_commands {
        assert!(
            stdout.contains(cmd),
            "bm --help should list '{}' command, output:\n{}",
            cmd,
            stdout
        );
    }

    // Hidden daemon-run command should NOT appear in help
    assert!(
        !stdout.contains("daemon-run"),
        "bm --help should NOT show hidden 'daemon-run' command, output:\n{}",
        stdout
    );
}

#[test]
fn subcommand_help_works() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "--help"]).output().unwrap();
    assert!(output.status.success(), "bm teams --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);

    assert!(
        stdout.contains("list"),
        "bm teams --help should list 'list' subcommand, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("sync"),
        "bm teams --help should list 'sync' subcommand, output:\n{}",
        stdout
    );
}

// ── Daemon CLI parsing (4 tests) ─────────────────────────────────────

#[test]
fn daemon_subcommand_help() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["daemon", "--help"]).output().unwrap();
    assert!(output.status.success(), "bm daemon --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(stdout.contains("start"), "Should list start subcommand");
    assert!(stdout.contains("stop"), "Should list stop subcommand");
    assert!(stdout.contains("status"), "Should list status subcommand");
}

#[test]
fn daemon_start_flags_parsed() {
    // Use an empty HOME so daemon start fails at runtime (no config) rather than
    // actually spawning a daemon against the real ~/.botminter/config.yml.
    let tmp = tempfile::tempdir().unwrap();

    // All flags should parse without clap error (will fail at runtime due to no config)
    for args in [
        vec!["daemon", "start", "--mode", "poll"],
        vec!["daemon", "start", "--mode", "webhook", "--port", "9999"],
        vec!["daemon", "start", "--interval", "30"],
        vec!["daemon", "start", "-t", "myteam"],
    ] {
        let output = bm(tmp.path())
            .args(&args)
            .output()
            .unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn daemon_stop_flags_parsed() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["daemon", "stop", "-t", "myteam"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "daemon stop -t should parse"
    );
}

#[test]
fn daemon_status_flags_parsed() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["daemon", "status", "-t", "myteam"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "daemon status -t should parse"
    );
}

// ── Show/describe subcommand parsing (6 tests) ───────────────────────

#[test]
fn teams_show_help_works() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "show", "--help"]).output().unwrap();
    assert!(
        output.status.success(),
        "bm teams show --help should exit 0"
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Show") || stdout.contains("show") || stdout.contains("team"),
        "help should describe the show command, stdout:\n{}",
        stdout
    );
}

#[test]
fn teams_show_with_name_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["teams", "show", "my-team"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams show my-team` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn teams_show_with_team_flag_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["teams", "show", "-t", "my-team"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams show -t my-team` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn members_show_requires_member_arg() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["members", "show"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm members show (no member) should exit with clap error code 2"
    );
}

#[test]
fn members_show_with_member_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["members", "show", "architect-01"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm members show architect-01` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn projects_list_help_works() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["projects", "list", "--help"]).output().unwrap();
    assert!(
        output.status.success(),
        "bm projects list --help should exit 0"
    );
}

#[test]
fn projects_show_requires_project_arg() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["projects", "show"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm projects show (no project) should exit with clap error code 2"
    );
}

#[test]
fn projects_show_with_project_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["projects", "show", "my-app"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm projects show my-app` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn subcommand_help_shows_new_commands() {
    let tmp = tempfile::tempdir().unwrap();
    // Teams help should show show, list, sync
    let output = bm(tmp.path()).args(["teams", "--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("show"),
        "bm teams --help should list 'show' subcommand, output:\n{}",
        stdout
    );

    // Members help should show show and list
    let output = bm(tmp.path()).args(["members", "--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("show"),
        "bm members --help should list 'show' subcommand, output:\n{}",
        stdout
    );

    // Projects help should show list, show, add, sync
    let output = bm(tmp.path()).args(["projects", "--help"]).output().unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("list"),
        "bm projects --help should list 'list' subcommand, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("show"),
        "bm projects --help should list 'show' subcommand, output:\n{}",
        stdout
    );
}

// ── --show-tags flag (3 tests) ──────────────────────────────────────

#[test]
fn profiles_describe_show_tags_outputs_tagged_files() {
    // Extract profiles to disk so the command can find them
    let profiles_tmp = tempfile::tempdir().unwrap();
    let profiles_path = profiles_tmp.path().join("botminter").join("profiles");
    std::fs::create_dir_all(&profiles_path).unwrap();
    bm::profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm(profiles_tmp.path())
        .args(["profiles", "describe", "scrum", "--show-tags"])
        .env("XDG_CONFIG_HOME", profiles_tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "bm profiles describe scrum --show-tags should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Coding-Agent Dependent Files"),
        "--show-tags should show 'Coding-Agent Dependent Files' section, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("context.md"),
        "--show-tags should list context.md as tagged, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("claude-code"),
        "--show-tags should reference claude-code agent, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_describe_without_show_tags_omits_tag_section() {
    // Extract profiles to disk so the command can find them
    let profiles_tmp = tempfile::tempdir().unwrap();
    let profiles_path = profiles_tmp.path().join("botminter").join("profiles");
    std::fs::create_dir_all(&profiles_path).unwrap();
    bm::profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm(profiles_tmp.path())
        .args(["profiles", "describe", "scrum"])
        .env("XDG_CONFIG_HOME", profiles_tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success(), "bm profiles describe scrum should succeed");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        !stdout.contains("Coding-Agent Dependent Files"),
        "Without --show-tags, 'Coding-Agent Dependent Files' section should be absent, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_init_extracts_to_temp_dir() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["profiles", "init"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bm profiles init should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("Extracted") && stdout.contains("profiles"),
        "Should print extraction summary, output:\n{}",
        stdout
    );

    // Verify profiles were actually created on disk
    let profiles_dir = tmp.path().join("botminter").join("profiles");
    assert!(
        profiles_dir.join("scrum").join("botminter.yml").exists(),
        "scrum profile should be extracted"
    );
    assert!(
        profiles_dir.join("agentic-sdlc-minimal").join("botminter.yml").exists(),
        "agentic-sdlc-minimal profile should be extracted"
    );
}

#[test]
fn profiles_init_without_force_skips_on_piped_stdin() {
    let tmp = tempfile::tempdir().unwrap();

    // First run — extract
    let output1 = bm(tmp.path())
        .args(["profiles", "init"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(output1.status.success());

    // Second run — piped stdin (no TTY) means EOF → default to skip
    let output2 = bm(tmp.path())
        .args(["profiles", "init"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(output2.status.success());

    let stdout = String::from_utf8_lossy(&output2.stdout);
    assert!(
        stdout.contains("skipped"),
        "Second run should skip existing profiles on piped stdin, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Summary"),
        "Should print summary, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_init_force_overwrites() {
    let tmp = tempfile::tempdir().unwrap();

    // First run
    bm(tmp.path())
        .args(["profiles", "init"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();

    // Second run with --force — overwrites all silently
    let output = bm(tmp.path())
        .args(["profiles", "init", "--force"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bm profiles init --force should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("overwritten"),
        "--force should overwrite profiles, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("Summary"),
        "--force should print summary, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_init_help_text() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["profiles", "init", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--force"),
        "Help should document --force flag, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_init_extracts_minty_config() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["profiles", "init"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "bm profiles init should succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("minty"),
        "Should mention minty extraction, output:\n{}",
        stdout
    );

    // Verify minty config was extracted alongside profiles
    let minty_dir = tmp.path().join("botminter").join("minty");
    assert!(
        minty_dir.join("prompt.md").exists(),
        "Minty prompt.md should be extracted"
    );
    assert!(
        minty_dir.join("config.yml").exists(),
        "Minty config.yml should be extracted"
    );
    assert!(
        minty_dir.join(".claude").join("skills").is_dir(),
        "Minty .claude/skills/ directory should be extracted"
    );

    // Verify minty is separate from profiles
    let profiles_dir = tmp.path().join("botminter").join("profiles");
    assert!(
        profiles_dir.is_dir(),
        "profiles/ should exist as sibling of minty/"
    );
    assert!(
        !profiles_dir.join("minty").exists(),
        "minty/ should NOT be inside profiles/"
    );
}

#[test]
fn profiles_describe_show_tags_help_text() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["profiles", "describe", "--help"])
        .output()
        .unwrap();
    assert!(output.status.success());

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--show-tags"),
        "Help should document --show-tags flag, output:\n{}",
        stdout
    );
}

// ── Init --bridge flag (2 tests) ─────────────────────────────────────

#[test]
fn init_bridge_flag_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args([
            "init",
            "--non-interactive",
            "--profile", "agentic-sdlc-minimal",
            "--team-name", "test",
            "--org", "testorg",
            "--repo", "testrepo",
            "--bridge", "telegram",
            "--skip-github",
        ])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm init --non-interactive --bridge telegram` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn init_without_bridge_flag_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args([
            "init",
            "--non-interactive",
            "--profile", "agentic-sdlc-minimal",
            "--team-name", "test",
            "--org", "testorg",
            "--repo", "testrepo",
            "--skip-github",
        ])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm init --non-interactive` without --bridge should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Bridge CLI parsing (10 tests) ────────────────────────────────────

#[test]
fn parse_bridge_start() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["bridge", "start"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge start` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_start_team() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "start", "-t", "myteam"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge start -t myteam` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_stop() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["bridge", "stop"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge stop` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_status() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["bridge", "status"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge status` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_identity_add() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "identity", "add", "testuser"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge identity add testuser` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_identity_rotate() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "identity", "rotate", "testuser"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge identity rotate testuser` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_identity_remove() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "identity", "remove", "testuser"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge identity remove testuser` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_identity_list() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "identity", "list"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge identity list` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_room_create() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "room", "create", "general"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge room create general` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn parse_bridge_room_list() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["bridge", "room", "list"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm bridge room list` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Auto-prompt pattern (ensure_profiles_initialized) ──────────────

#[test]
fn profiles_list_auto_initializes_when_profiles_missing() {
    // Use an empty XDG_CONFIG_HOME — no profiles on disk.
    // Piped stdin = non-TTY → should auto-initialize and succeed.
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["profiles", "list"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "profiles list should auto-init and succeed, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Initialized") && stderr.contains("profiles"),
        "Should print init message on stderr, stderr:\n{}",
        stderr
    );

    // The command should also produce profile listing output
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("scrum"),
        "After auto-init, profiles list should show scrum, output:\n{}",
        stdout
    );
}

#[test]
fn profiles_list_skips_init_when_profiles_exist() {
    let tmp = tempfile::tempdir().unwrap();
    // Pre-populate profiles
    let profiles_path = tmp.path().join("botminter").join("profiles");
    std::fs::create_dir_all(&profiles_path).unwrap();
    bm::profile::extract_embedded_to_disk(&profiles_path).unwrap();

    let output = bm(tmp.path())
        .args(["profiles", "list"])
        .env("XDG_CONFIG_HOME", tmp.path())
        .output()
        .unwrap();
    assert!(output.status.success());

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        !stderr.contains("Initialized"),
        "Should not print init message when profiles exist, stderr:\n{}",
        stderr
    );
}

// ── Chat CLI parsing (5 tests) ──────────────────────────────────────

#[test]
fn chat_requires_member_argument() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["chat"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm chat (no member) should exit with clap error code 2"
    );

    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("<MEMBER>") || stderr.contains("member") || stderr.contains("required"),
        "error should mention the missing member argument, stderr:\n{}",
        stderr
    );
}

#[test]
fn chat_help_shows_flags() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["chat", "--help"]).output().unwrap();
    assert!(output.status.success(), "bm chat --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("--hat"),
        "chat help should show --hat flag, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("--render-system-prompt"),
        "chat help should show --render-system-prompt flag, output:\n{}",
        stdout
    );
    assert!(
        stdout.contains("-t") || stdout.contains("--team"),
        "chat help should show team flag, output:\n{}",
        stdout
    );
}

#[test]
fn chat_with_member_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["chat", "architect-01"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm chat architect-01` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn chat_with_all_flags_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args([
            "chat",
            "architect-01",
            "-t",
            "my-team",
            "--hat",
            "executor",
            "--render-system-prompt",
        ])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm chat` with all flags should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn chat_team_flag_short_and_long() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["chat", "bob", "-t", "my-team"],
        vec!["chat", "bob", "--team", "my-team"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// ── Minty CLI parsing (4 tests) ─────────────────────────────────────

#[test]
fn minty_help_shows_team_flag() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["minty", "--help"]).output().unwrap();
    assert!(output.status.success(), "bm minty --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        stdout.contains("-t") || stdout.contains("--team"),
        "minty help should show team flag, output:\n{}",
        stdout
    );
}

#[test]
fn minty_no_args_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["minty"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm minty` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn minty_with_team_flag_parses() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path())
        .args(["minty", "-t", "my-team"])
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm minty -t my-team` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn minty_team_flag_short_and_long() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["minty", "-t", "my-team"],
        vec!["minty", "--team", "my-team"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` should not be a parse error, stderr: {}",
            args.join(" "),
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

// ── CT-154-05: bm session list and bm session finalize ──────────────

#[test]
fn session_list_subcommand_parses() {
    let tmp = tempfile::tempdir().unwrap();
    // bm session list (no team) must parse without clap error (exit 1 = runtime, not 2 = parse)
    for args in [
        vec!["session", "list"],
        vec!["session", "list", "--json"],
        vec!["session", "list", "-t", "myteam"],
        vec!["session", "list", "--json", "-t", "myteam"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` must not be a parse error (exit {}), stderr: {}",
            args.join(" "),
            code,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn session_finalize_parses_with_session_id() {
    let tmp = tempfile::tempdir().unwrap();
    for args in [
        vec!["session", "finalize", "abc123"],
        vec!["session", "finalize", "abc123", "-t", "myteam"],
    ] {
        let output = bm(tmp.path()).args(&args).output().unwrap();
        let code = output.status.code().unwrap_or(-1);
        assert_ne!(
            code, CLAP_PARSE_ERROR_CODE,
            "`bm {}` must not be a parse error (exit {}), stderr: {}",
            args.join(" "),
            code,
            String::from_utf8_lossy(&output.stderr)
        );
    }
}

#[test]
fn status_history_flag_rejected_with_migration_message() {
    let tmp = tempfile::tempdir().unwrap();
    let output = bm(tmp.path()).args(["status", "--history"]).output().unwrap();
    assert!(
        !output.status.success(),
        "bm status --history must exit non-zero after --history removal"
    );
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("has been removed") && stderr.contains("bm session list"),
        "`bm status --history` must print migration message directing to 'bm session list', stderr:\n{}",
        stderr
    );
}
