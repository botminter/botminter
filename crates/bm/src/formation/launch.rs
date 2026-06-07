use std::fs;
use std::process::{Child, Command};

use anyhow::{Context, Result};

/// Spawns a detached thread that calls `child.wait()`, triggering `waitpid()`
/// so the child is reaped by the kernel instead of becoming a zombie.
///
/// SIGCHLD=SIG_IGN is not viable because it breaks `Command::output()` (used
/// by `gh api` in poll mode). This per-child reaper is the alternative.
pub(crate) fn reap_child(mut child: Child) {
    std::thread::spawn(move || {
        let _ = child.wait();
    });
}

/// Launches `ralph run -p PROMPT.md` in the given workspace directory.
/// Returns the child PID.
///
/// If `gh_config_dir` is set, `GH_CONFIG_DIR` is set instead of `GH_TOKEN`.
/// This is used for members with GitHub App credentials: the daemon writes
/// `hosts.yml` with the installation token, and `gh` reads from it.
/// `GH_TOKEN` would override `hosts.yml`, so we must not set both.
pub fn launch_ralph(
    workspace: &std::path::Path,
    member_token: Option<&str>,
    bridge_type: Option<&str>,
    service_url: Option<&str>,
    gh_config_dir: Option<&std::path::Path>,
) -> Result<u32> {
    let mut cmd = Command::new("ralph");
    cmd.args(["run", "-p", "PROMPT.md"])
        .current_dir(workspace)
        .env_remove("CLAUDECODE");

    // App-credential members use GH_CONFIG_DIR (daemon-managed hosts.yml).
    // Members without App creds rely on the host's `gh auth` session.
    if let Some(config_dir) = gh_config_dir {
        cmd.env("GH_CONFIG_DIR", config_dir);
        // Remove token env vars so gh uses the App token from hosts.yml
        // instead of the operator's PAT.
        cmd.env_remove("GH_TOKEN");
        cmd.env_remove("GITHUB_TOKEN");
    }

    if let Some(token) = member_token {
        match bridge_type {
            Some("rocketchat") => {
                cmd.env("RALPH_ROCKETCHAT_AUTH_TOKEN", token);
                if let Some(url) = service_url {
                    cmd.env("RALPH_ROCKETCHAT_SERVER_URL", url);
                }
            }
            Some("tuwunel") => {
                cmd.env("RALPH_MATRIX_ACCESS_TOKEN", token);
                if let Some(url) = service_url {
                    cmd.env("RALPH_MATRIX_HOMESERVER_URL", url);
                }
            }
            _ => {
                cmd.env("RALPH_TELEGRAM_BOT_TOKEN", token);
                if let Some(url) = service_url {
                    cmd.env("RALPH_TELEGRAM_API_URL", url);
                }
            }
        }
    }

    // Detach stdio from current process
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null());

    let child = cmd.spawn().with_context(|| {
        format!("Failed to spawn ralph in {}", workspace.display())
    })?;

    let pid = child.id();
    reap_child(child);
    Ok(pid)
}

/// Configuration for launching a brain process, bundling bridge-related params.
pub struct BrainLaunchConfig<'a> {
    pub workspace: &'a std::path::Path,
    pub system_prompt_path: &'a std::path::Path,
    pub member_token: Option<&'a str>,
    pub bridge_type: Option<&'a str>,
    pub service_url: Option<&'a str>,
    pub room_id: Option<&'a str>,
    pub user_id: Option<&'a str>,
    pub operator_user_id: Option<&'a str>,
    pub team_repo: Option<&'a std::path::Path>,
    /// When set, uses GH_CONFIG_DIR instead of GH_TOKEN (App credential path).
    pub gh_config_dir: Option<&'a std::path::Path>,
    /// Durable member-state directory for cross-session persistence (e.g., dm-room.json).
    /// Forwarded to brain process as BM_MEMBER_STATE_DIR.
    pub member_state_dir: Option<&'a std::path::Path>,
}

/// Launches the brain multiplexer for a chat-first member.
///
/// Spawns `bm brain-run` as a background process, which runs the multiplexer
/// event loop (ACP session + event watcher + heartbeat). Returns the child PID.
pub fn launch_brain(config: &BrainLaunchConfig<'_>) -> Result<u32> {
    let bm_binary = std::env::current_exe()
        .context("Failed to determine bm binary path")?;

    let mut cmd = Command::new(&bm_binary);
    cmd.args([
        "brain-run",
        "--workspace",
    ])
    .arg(config.workspace)
    .arg("--system-prompt")
    .arg(config.system_prompt_path)
    .current_dir(config.workspace)
    .env_remove("CLAUDECODE");

    // App-credential members use GH_CONFIG_DIR (daemon-managed hosts.yml).
    // Members without App creds rely on the host's `gh auth` session.
    if let Some(config_dir) = config.gh_config_dir {
        cmd.env("GH_CONFIG_DIR", config_dir);
        // Remove token env vars so gh uses the App token from hosts.yml
        // instead of the operator's PAT. gh respects all of these.
        cmd.env_remove("GH_TOKEN");
        cmd.env_remove("GITHUB_TOKEN");
    }

    if let Some(token) = config.member_token {
        match config.bridge_type {
            Some("rocketchat") => {
                cmd.env("RALPH_ROCKETCHAT_AUTH_TOKEN", token);
                if let Some(url) = config.service_url {
                    cmd.env("RALPH_ROCKETCHAT_SERVER_URL", url);
                }
            }
            Some("tuwunel") => {
                cmd.env("RALPH_MATRIX_ACCESS_TOKEN", token);
                if let Some(url) = config.service_url {
                    cmd.env("RALPH_MATRIX_HOMESERVER_URL", url);
                }
            }
            _ => {
                cmd.env("RALPH_TELEGRAM_BOT_TOKEN", token);
            }
        }
    }

    // Bridge adapter config: room ID and member user ID for Matrix bridge I/O
    if let Some(rid) = config.room_id {
        cmd.env("BM_BRAIN_ROOM_ID", rid);
    }
    if let Some(uid) = config.user_id {
        cmd.env("BM_BRAIN_USER_ID", uid);
    }
    if let Some(op_uid) = config.operator_user_id {
        cmd.env("BM_BRAIN_OPERATOR_USER_ID", op_uid);
    }
    // Team repo path for gh commands and board awareness
    if let Some(repo) = config.team_repo {
        cmd.env("BM_TEAM_REPO", repo);
    }
    // Durable member-state directory for cross-session persistence (dm-room.json, etc.)
    if let Some(dir) = config.member_state_dir {
        cmd.env("BM_MEMBER_STATE_DIR", dir);
    }

    // Detach from current process group — redirect stderr to log file for diagnostics.
    let log_path = config.workspace.join("brain-stderr.log");
    let log_file = std::fs::File::create(&log_path)
        .with_context(|| format!("Failed to create brain stderr log at {}", log_path.display()))?;
    cmd.stdin(std::process::Stdio::null())
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::from(log_file));

    let child = cmd.spawn().with_context(|| {
        format!(
            "Failed to spawn brain in {}\n  binary: {}\n  system_prompt: {}",
            config.workspace.display(),
            bm_binary.display(),
            config.system_prompt_path.display(),
        )
    })?;

    let pid = child.id();
    reap_child(child);
    Ok(pid)
}

/// Returns true if the workspace has a `brain-prompt.md` file,
/// indicating this member should run in brain (chat-first) mode.
#[cfg_attr(not(test), allow(dead_code))]
pub fn is_brain_member(workspace: &std::path::Path) -> bool {
    workspace.join("brain-prompt.md").exists()
}

/// Checks if a member has a credential but RObot.enabled is false in ralph.yml.
///
/// Returns `true` if there is a mismatch (credential present but RObot disabled),
/// meaning the workspace needs to be re-provisioned to update RObot configuration.
#[cfg_attr(not(test), allow(dead_code))]
pub fn check_robot_enabled_mismatch(
    ralph_yml_path: &std::path::Path,
    has_credential: bool,
) -> bool {
    if !has_credential {
        return false;
    }
    if !ralph_yml_path.exists() {
        return false;
    }
    let contents = match fs::read_to_string(ralph_yml_path) {
        Ok(c) => c,
        Err(_) => return false,
    };
    let doc: serde_yml::Value = match serde_yml::from_str(&contents) {
        Ok(d) => d,
        Err(_) => return false,
    };

    // Check if RObot.enabled is explicitly false
    match doc
        .get("RObot")
        .and_then(|r| r.get("enabled"))
        .and_then(|e| e.as_bool())
    {
        Some(false) => true,  // Mismatch: has cred but disabled
        _ => false,           // Either enabled or not set at all
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    #[test]
    fn launch_ralph_receives_per_member_credential() {
        // This test verifies that launch_ralph correctly accepts bridge-type-aware
        // parameters. We can't test actual process spawning, but we verify the
        // function signature accepts the new bridge_type + service_url parameters.
        //
        // The real test is that `bm start` resolves credentials per-member
        // via resolve_credential_from_store() in the member loop.

        type LaunchRalphFn = fn(&std::path::Path, Option<&str>, Option<&str>, Option<&str>, Option<&std::path::Path>) -> Result<u32>;
        let _: LaunchRalphFn = launch_ralph;
    }

    #[test]
    fn launch_brain_signature_accepts_config_struct() {
        // Verify launch_brain compiles with BrainLaunchConfig,
        // including room_id and user_id for bridge adapter config.
        let _: fn(&BrainLaunchConfig<'_>) -> Result<u32> = launch_brain;
    }

    #[test]
    fn is_brain_member_with_brain_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        fs::write(tmp.path().join("brain-prompt.md"), "# Brain").unwrap();
        assert!(is_brain_member(tmp.path()));
    }

    #[test]
    fn is_brain_member_without_brain_prompt() {
        let tmp = tempfile::tempdir().unwrap();
        assert!(!is_brain_member(tmp.path()));
    }

    #[test]
    fn check_robot_enabled_diagnostic() {
        // Test the diagnostic warning logic: when a member has a credential
        // but RObot.enabled is false, a warning should be emitted.
        // This validates the function exists and works correctly.
        let tmp = tempfile::tempdir().unwrap();
        let ralph_yml = tmp.path().join("ralph.yml");
        fs::write(
            &ralph_yml,
            "preset: feature-development\nRObot:\n  enabled: false\n",
        )
        .unwrap();

        let has_credential = true;
        let robot_enabled = check_robot_enabled_mismatch(&ralph_yml, has_credential);
        assert!(
            robot_enabled,
            "should return true when credential exists but RObot.enabled is false"
        );
    }
}
