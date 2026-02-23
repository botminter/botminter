//! CLI argument parsing tests for the `bm` binary.
//!
//! These tests exercise clap's argument definitions — aliases, flags,
//! required args, error messages, and help text — without requiring a real
//! team setup or HOME env mutation. They run fast and in parallel.

use std::process::Command;

/// Helper: create a `Command` for the `bm` binary.
fn bm() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bm"))
}

/// Clap uses exit code 2 for argument parsing / usage errors.
/// Runtime errors (missing config, etc.) exit with 1 via anyhow.
const CLAP_PARSE_ERROR_CODE: i32 = 2;

// ── Command aliases (2 tests) ────────────────────────────────────────

#[test]
fn start_and_up_are_aliases() {
    let start = bm().args(["start", "--help"]).output().unwrap();
    let up = bm().args(["up", "--help"]).output().unwrap();

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
    let output = bm().args(["--help"]).output().unwrap();
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
    let up_help = bm().args(["up", "--help"]).output().unwrap();
    assert!(
        up_help.status.success(),
        "'bm up --help' should succeed — alias resolves to start"
    );
}

// ── Flag parsing (5 tests) ───────────────────────────────────────────

#[test]
fn team_flag_short_and_long() {
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
        let output = bm().args(*args).output().unwrap();
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
    // --force and -f are both defined via #[arg(short, long)] on Stop
    for args in [
        vec!["stop", "--force"],
        vec!["stop", "-f"],
    ] {
        let output = bm().args(&args).output().unwrap();
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
fn push_flag_on_sync() {
    let output = bm().args(["teams", "sync", "--push"]).output().unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "`bm teams sync --push` should not be a parse error, stderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn verbose_flag_on_status() {
    // -v and --verbose are both defined via #[arg(short, long)] on Status
    for args in [
        vec!["status", "-v"],
        vec!["status", "--verbose"],
    ] {
        let output = bm().args(&args).output().unwrap();
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
    // --name is defined with #[arg(long)] only (no short -n)
    let output = bm()
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
    let output = bm().args(["hire"]).output().unwrap();
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
    let output = bm().args(["profiles", "describe"]).output().unwrap();
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
    let output = bm().args(["projects", "add"]).output().unwrap();
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
    let output = bm().args(["projects", "sync", "--help"]).output().unwrap();
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
    let output = bm().args(["foobar"]).output().unwrap();
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
    let output = bm().args(["status", "--foobar"]).output().unwrap();
    assert_eq!(
        output.status.code(),
        Some(CLAP_PARSE_ERROR_CODE),
        "bm status --foobar should exit with clap error code 2"
    );
}

#[test]
fn completions_requires_valid_shell() {
    let output = bm().args(["completions", "notashell"]).output().unwrap();
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
    let output = bm().args(["--help"]).output().unwrap();
    assert!(output.status.success(), "bm --help should exit 0");

    let stdout = String::from_utf8_lossy(&output.stdout);

    let expected_commands = [
        "init",
        "hire",
        "start",
        "stop",
        "status",
        "teams",
        "members",
        "roles",
        "profiles",
        "projects",
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
    let output = bm().args(["teams", "--help"]).output().unwrap();
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
    let output = bm().args(["daemon", "--help"]).output().unwrap();
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
        let output = bm()
            .args(&args)
            .env("HOME", tmp.path())
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
    let output = bm()
        .args(["daemon", "stop", "-t", "myteam"])
        .env("HOME", tmp.path())
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
    let output = bm()
        .args(["daemon", "status", "-t", "myteam"])
        .env("HOME", tmp.path())
        .output()
        .unwrap();
    let code = output.status.code().unwrap_or(-1);
    assert_ne!(
        code, CLAP_PARSE_ERROR_CODE,
        "daemon status -t should parse"
    );
}
