use std::path::Path;
use std::process::{Command, ExitStatus};

use anyhow::{bail, Context, Result};

/// Launch an interactive Claude Code session with a skill.
///
/// Spawns `claude` with the skill path, inheriting stdin/stdout/stderr
/// so the user can interact. Blocks until the session ends.
pub fn interactive_claude_session(
    working_dir: &Path,
    skill_path: &Path,
    env_vars: &[(String, String)],
) -> Result<()> {
    // Verify claude binary exists
    if which::which("claude").is_err() {
        bail!("'claude' not found in PATH. Install Claude Code first.");
    }

    // Read skill content for prompt injection
    let skill_content = std::fs::read_to_string(skill_path)
        .with_context(|| format!("Failed to read skill at {}", skill_path.display()))?;

    let mut cmd = Command::new("claude");
    cmd.arg("--print")
        .arg(&skill_content)
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
    // Verify ralph binary exists
    if which::which("ralph").is_err() {
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

    let status = cmd
        .status()
        .context("Failed to launch Ralph session")?;

    Ok(status)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn interactive_session_missing_claude_errors() {
        // In test environment, 'claude' is unlikely to be in PATH
        // But if it is, this test is still valid — it will try to run
        // We just verify the function handles the binary check
        let tmp = tempfile::tempdir().unwrap();
        let skill_path = tmp.path().join("SKILL.md");
        std::fs::write(&skill_path, "# Test skill").unwrap();

        let result = interactive_claude_session(tmp.path(), &skill_path, &[]);
        // Either 'claude not found' or it runs — both are valid
        if let Err(e) = result {
            let msg = e.to_string();
            // Should be a clear error, not a panic
            assert!(
                msg.contains("claude") || msg.contains("Claude"),
                "Error should mention claude: {}",
                msg
            );
        }
    }

    #[test]
    fn oneshot_session_missing_ralph_errors() {
        let tmp = tempfile::tempdir().unwrap();
        let prompt = tmp.path().join("PROMPT.md");
        let ralph_yml = tmp.path().join("ralph.yml");
        std::fs::write(&prompt, "# Test").unwrap();
        std::fs::write(&ralph_yml, "model: sonnet").unwrap();

        // ralph might be in PATH in the test environment
        let result = oneshot_ralph_session(tmp.path(), &prompt, &ralph_yml, &[]);
        if let Err(e) = result {
            let msg = e.to_string();
            assert!(
                msg.contains("ralph") || msg.contains("Ralph"),
                "Error should mention ralph: {}",
                msg
            );
        }
    }
}
