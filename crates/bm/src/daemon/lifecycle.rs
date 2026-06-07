use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};

use crate::profile;
use crate::state;

use super::config::{read_team_schema, DaemonConfig, DaemonPaths};

/// Result of a successful daemon start.
pub struct DaemonStartResult {
    pub pid: u32,
}

/// Status information about a daemon.
pub enum DaemonStatusInfo {
    /// Daemon is running with the given PID and optional config.
    Running {
        pid: u32,
        config: Option<DaemonConfig>,
    },
    /// Daemon is not running.
    NotRunning {
        reason: &'static str,
    },
}

/// Starts a daemon for the given team as a detached child process.
///
/// Validates the team schema, checks for an existing daemon, spawns the
/// `bm daemon-run` child process, writes PID and config files, and verifies
/// the child didn't exit immediately.
pub fn start_daemon(
    team_name: &str,
    team_repo: &Path,
    mode: &str,
    port: u16,
    interval: u64,
    bind: &str,
    log_level: &str,
) -> Result<DaemonStartResult> {
    // Schema v2 gate
    let team_schema = read_team_schema(team_repo)?;
    profile::require_current_schema(team_name, &team_schema)?;

    let paths = DaemonPaths::new(team_name)?;

    // Check if already running
    let pid_file = paths.pid();
    if pid_file.exists() {
        let pid_str =
            fs::read_to_string(&pid_file).context("Failed to read daemon PID file")?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if state::is_alive(pid) {
                bail!(
                    "Daemon already running for team '{}' (PID {})",
                    team_name,
                    pid
                );
            }
            // Stale PID file — clean up
            let _ = fs::remove_file(&pid_file);
        }
    }

    // Validate mode
    if mode != "webhook" && mode != "poll" {
        bail!("Invalid daemon mode '{}'. Use 'webhook' or 'poll'.", mode);
    }

    // Spawn the daemon as a detached child process using `bm daemon-run`
    let exe = std::env::current_exe().context("Failed to determine bm executable path")?;
    let log_file_path = paths.log()?;

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .with_context(|| {
            format!("Failed to open log file at {}", log_file_path.display())
        })?;

    let log_file_err = log_file
        .try_clone()
        .context("Failed to clone log file handle")?;

    let child = Command::new(exe)
        .args([
            "daemon-run",
            "--team",
            team_name,
            "--mode",
            mode,
            "--port",
            &port.to_string(),
            "--interval",
            &interval.to_string(),
            "--bind",
            bind,
            "--log-level",
            log_level,
        ])
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_file_err)
        .spawn()
        .context("Failed to spawn daemon process")?;

    let pid = child.id();

    // Write PID file with 0600 permissions
    if let Some(dir) = pid_file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&pid_file, pid.to_string())?;
    fs::set_permissions(&pid_file, fs::Permissions::from_mode(0o600))?;

    // The daemon writes its own config file after binding (with the actual
    // port, which may differ from the requested port when port=0). Poll
    // for the config file to appear, which indicates the daemon is ready.
    let cfg_path = paths.config();
    let max_wait = Duration::from_secs(10);
    let poll_interval = Duration::from_millis(100);
    let start_time = std::time::Instant::now();

    loop {
        if !state::is_alive(pid) {
            let _ = fs::remove_file(&pid_file);
            let _ = fs::remove_file(&cfg_path);
            bail!(
                "Daemon process exited immediately. Check logs at {}",
                log_file_path.display()
            );
        }

        if cfg_path.exists() {
            break;
        }

        if start_time.elapsed() > max_wait {
            let _ = fs::remove_file(&pid_file);
            bail!(
                "Daemon did not write config within {:?}. Check logs at {}",
                max_wait,
                log_file_path.display()
            );
        }

        thread::sleep(poll_interval);
    }

    Ok(DaemonStartResult { pid })
}

/// Stops a running daemon for the given team.
///
/// Reads the PID file, sends SIGTERM, waits up to 30 seconds, escalates to
/// SIGKILL if needed, then cleans up PID/config/poll-state files.
pub fn stop_daemon(team_name: &str) -> Result<()> {
    let paths = DaemonPaths::new(team_name)?;
    let pid_file = paths.pid();

    if !pid_file.exists() {
        bail!("Daemon not running for team '{}'", team_name);
    }

    let pid_str =
        fs::read_to_string(&pid_file).context("Failed to read daemon PID file")?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .context("Invalid PID in daemon PID file")?;

    if state::is_alive(pid) {
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }
        for _ in 0..30 {
            if !state::is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }
        if state::is_alive(pid) {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }

    // Clean up files
    let _ = fs::remove_file(&pid_file);
    let _ = fs::remove_file(paths.config());
    let _ = fs::remove_file(paths.poll_state());

    Ok(())
}

/// Queries the status of a daemon for the given team.
///
/// Returns structured status information: whether the daemon is running,
/// its PID, and its configuration. Also cleans up stale PID files.
pub fn query_status(team_name: &str) -> Result<DaemonStatusInfo> {
    let paths = DaemonPaths::new(team_name)?;
    let pid_file = paths.pid();

    if !pid_file.exists() {
        return Ok(DaemonStatusInfo::NotRunning {
            reason: "not running",
        });
    }

    let pid_str =
        fs::read_to_string(&pid_file).context("Failed to read daemon PID file")?;
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            let _ = fs::remove_file(&pid_file);
            return Ok(DaemonStatusInfo::NotRunning {
                reason: "not running (corrupt PID file)",
            });
        }
    };

    if !state::is_alive(pid) {
        // Clean up stale files
        let _ = fs::remove_file(&pid_file);
        let _ = fs::remove_file(paths.config());
        return Ok(DaemonStatusInfo::NotRunning {
            reason: "not running (stale PID file)",
        });
    }

    // Read daemon config for details
    let cfg_file = paths.config();
    let config = if cfg_file.exists() {
        let contents =
            fs::read_to_string(&cfg_file).context("Failed to read daemon config")?;
        serde_json::from_str::<DaemonConfig>(&contents).ok()
    } else {
        None
    };

    Ok(DaemonStatusInfo::Running { pid, config })
}
