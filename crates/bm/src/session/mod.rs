pub mod cleanup;
pub mod dirty_state;
pub mod finalization;
pub mod history;
pub mod manager;
pub mod registry;
pub mod retention;
pub mod stop;
pub mod types;
pub mod work_item_lock;

pub use types::{SessionId, SessionRecord, SessionState, SessionType};

use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

/// Runs a ralph CLI command in the given workspace and returns stdout.
pub fn run_ralph_cmd(workspace: &Path, args: &[&str]) -> Result<String> {
    let output = Command::new("ralph")
        .args(args)
        .current_dir(workspace)
        .output()
        .with_context(|| format!("Failed to run ralph {}", args.join(" ")))?;

    if !output.status.success() {
        bail!("ralph {} failed", args.join(" "));
    }

    Ok(String::from_utf8_lossy(&output.stdout).to_string())
}

/// Launch an interactive Claude Code session with a skill.
///
/// Spawns `claude` with the skill path, inheriting stdin/stdout/stderr
/// so the user can interact. Blocks until the session ends.
pub fn interactive_claude_session(
    working_dir: &Path,
    skill_path: &Path,
    env_vars: &[(String, String)],
) -> Result<()> {
    interactive_claude_session_with_check(working_dir, skill_path, env_vars, |name| {
        which::which(name).map(|_| ())
    })
}

/// Internal helper that accepts a binary-check closure for testability.
fn interactive_claude_session_with_check<F>(
    working_dir: &Path,
    skill_path: &Path,
    env_vars: &[(String, String)],
    check_binary: F,
) -> Result<()>
where
    F: FnOnce(&str) -> Result<(), which::Error>,
{
    if check_binary("claude").is_err() {
        bail!("'claude' not found in PATH. Install Claude Code first.");
    }

    // Read skill content and write to a temp file for --append-system-prompt-file
    let skill_content = std::fs::read_to_string(skill_path)
        .with_context(|| format!("Failed to read skill at {}", skill_path.display()))?;

    let mut tmp_file = tempfile::Builder::new()
        .prefix("bm-session-")
        .suffix(".md")
        .tempfile()
        .context("Failed to create temp file for session prompt")?;
    std::io::Write::write_all(&mut tmp_file, skill_content.as_bytes())
        .context("Failed to write session prompt to temp file")?;

    let mut cmd = Command::new("claude");
    cmd.arg("--append-system-prompt-file")
        .arg(tmp_file.path())
        .current_dir(working_dir);

    // Pass environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // Interactive: inherit all stdio
    let status = cmd
        .status()
        .context("Failed to launch Claude Code session")?;

    if !status.success() {
        bail!("Claude Code session exited with error");
    }

    Ok(())
}

/// Launch a one-shot headless Ralph session.
///
/// Spawns `ralph run -p <prompt_path>` and blocks until completion.
/// The ralph.yml in the working directory controls execution mode.
/// Returns the exit status.
pub fn oneshot_ralph_session(
    working_dir: &Path,
    prompt_path: &Path,
    _ralph_yml_path: &Path,
    env_vars: &[(String, String)],
) -> Result<ExitStatus> {
    oneshot_ralph_session_with_check(
        working_dir,
        prompt_path,
        _ralph_yml_path,
        env_vars,
        |name| which::which(name).map(|_| ()),
    )
}

/// Internal helper that accepts a binary-check closure for testability.
fn oneshot_ralph_session_with_check<F>(
    working_dir: &Path,
    prompt_path: &Path,
    _ralph_yml_path: &Path,
    env_vars: &[(String, String)],
    check_binary: F,
) -> Result<ExitStatus>
where
    F: FnOnce(&str) -> Result<(), which::Error>,
{
    if check_binary("ralph").is_err() {
        bail!("'ralph' not found in PATH. Install ralph-orchestrator first.");
    }

    let prompt_str = prompt_path
        .to_str()
        .context("Prompt path is not valid UTF-8")?;

    let mut cmd = Command::new("ralph");
    cmd.args(["run", "-p", prompt_str])
        .current_dir(working_dir)
        // Unset CLAUDECODE to avoid nested-Claude issues
        .env_remove("CLAUDECODE");

    // Pass environment variables
    for (key, value) in env_vars {
        cmd.env(key, value);
    }

    // One-shot: stdin null, stdout/stderr inherited for logging
    cmd.stdin(std::process::Stdio::null());

    let status = cmd.status().context("Failed to launch Ralph session")?;

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn binary_not_found(_name: &str) -> Result<(), which::Error> {
        Err(which::Error::CannotFindBinaryPath)
    }

    #[test]
    fn interactive_session_missing_claude_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("SKILL.md");
        std::fs::write(&skill_path, "# Test skill").unwrap();

        let err =
            interactive_claude_session_with_check(tmp.path(), &skill_path, &[], binary_not_found)
                .expect_err("should error when claude binary not found");

        let msg = err.to_string();
        assert!(
            msg.contains("claude") || msg.contains("Claude"),
            "Error should mention claude: {msg}",
        );
    }

    #[test]
    fn oneshot_session_missing_ralph_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = tmp.path().join("PROMPT.md");
        let ralph_yml = tmp.path().join("ralph.yml");
        std::fs::write(&prompt, "# Test").unwrap();
        std::fs::write(&ralph_yml, "model: sonnet").unwrap();

        let err = oneshot_ralph_session_with_check(
            tmp.path(),
            &prompt,
            &ralph_yml,
            &[],
            binary_not_found,
        )
        .expect_err("should error when ralph binary not found");

        let msg = err.to_string();
        assert!(
            msg.contains("ralph") || msg.contains("Ralph"),
            "Error should mention ralph: {msg}",
        );
    }
}
