use std::fs;
use std::io::Write as _;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

use crate::config;
use crate::profile;
use crate::state;

/// Daemon config file stored at `~/.botminter/daemon-<team>.json`.
#[derive(Debug, Serialize, Deserialize, Clone)]
pub struct DaemonConfig {
    pub team: String,
    pub mode: String,
    pub port: u16,
    pub interval_secs: u64,
    pub pid: u32,
    pub started_at: String,
}

/// Poll state tracking for poll mode.
#[derive(Debug, Serialize, Deserialize, Default, Clone)]
pub struct PollState {
    pub last_event_id: Option<String>,
    pub last_poll_at: Option<String>,
}

/// Maximum log file size before rotation (10 MB).
const MAX_LOG_SIZE: u64 = 10 * 1024 * 1024;

/// GitHub event types that trigger member launches.
const RELEVANT_EVENTS: &[&str] = &[
    "issues",
    "issue_comment",
    "pull_request",
];

/// Returns the PID file path for a daemon.
pub fn pid_path(team_name: &str) -> Result<PathBuf> {
    Ok(config::config_dir()?.join(format!("daemon-{}.pid", team_name)))
}

/// Returns the config file path for a daemon.
pub fn config_path(team_name: &str) -> Result<PathBuf> {
    Ok(config::config_dir()?.join(format!("daemon-{}.json", team_name)))
}

/// Returns the poll state file path for a daemon.
pub fn poll_state_path(team_name: &str) -> Result<PathBuf> {
    Ok(config::config_dir()?.join(format!(
        "daemon-{}-poll.json",
        team_name
    )))
}

/// Returns the log file path for a daemon.
pub fn log_path(team_name: &str) -> Result<PathBuf> {
    let logs_dir = config::config_dir()?.join("logs");
    fs::create_dir_all(&logs_dir)?;
    Ok(logs_dir.join(format!("daemon-{}.log", team_name)))
}

/// Returns the per-member log file path.
pub fn member_log_path(team_name: &str, member_name: &str) -> Result<PathBuf> {
    let logs_dir = config::config_dir()?.join("logs");
    fs::create_dir_all(&logs_dir)?;
    Ok(logs_dir.join(format!("member-{}-{}.log", team_name, member_name)))
}

/// Handles `bm daemon start`.
pub fn start(
    team_flag: Option<&str>,
    mode: &str,
    port: u16,
    interval: u64,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Schema v2 gate
    let team_schema = read_team_schema(&team_repo)?;
    profile::require_current_schema(&team.name, &team_schema)?;

    // Check if already running
    let pid_file = pid_path(&team.name)?;
    if pid_file.exists() {
        let pid_str = fs::read_to_string(&pid_file)
            .context("Failed to read daemon PID file")?;
        if let Ok(pid) = pid_str.trim().parse::<u32>() {
            if state::is_alive(pid) {
                bail!(
                    "Daemon already running for team '{}' (PID {})",
                    team.name,
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

    eprintln!(
        "Starting daemon for team '{}' in {} mode...",
        team.name, mode
    );

    // Spawn the daemon as a detached child process using `bm daemon-run`
    let exe = std::env::current_exe().context("Failed to determine bm executable path")?;
    let log_file_path = log_path(&team.name)?;

    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .with_context(|| format!("Failed to open log file at {}", log_file_path.display()))?;

    let log_file_err = log_file
        .try_clone()
        .context("Failed to clone log file handle")?;

    let child = Command::new(exe)
        .args([
            "daemon-run",
            "--team",
            &team.name,
            "--mode",
            mode,
            "--port",
            &port.to_string(),
            "--interval",
            &interval.to_string(),
        ])
        .stdin(std::process::Stdio::null())
        .stdout(log_file)
        .stderr(log_file_err)
        .spawn()
        .context("Failed to spawn daemon process")?;

    let pid = child.id();

    // Write PID file with 0600 permissions
    let pid_file = pid_path(&team.name)?;
    if let Some(dir) = pid_file.parent() {
        fs::create_dir_all(dir)?;
    }
    fs::write(&pid_file, pid.to_string())?;
    fs::set_permissions(&pid_file, fs::Permissions::from_mode(0o600))?;

    // Write config
    let daemon_cfg = DaemonConfig {
        team: team.name.clone(),
        mode: mode.to_string(),
        port,
        interval_secs: interval,
        pid,
        started_at: chrono::Utc::now().to_rfc3339(),
    };
    let cfg_path = config_path(&team.name)?;
    let contents =
        serde_json::to_string_pretty(&daemon_cfg).context("Failed to serialize daemon config")?;
    fs::write(&cfg_path, contents)?;

    // Brief wait to detect immediate failures
    thread::sleep(Duration::from_millis(500));
    if !state::is_alive(pid) {
        // Clean up PID/config files
        let _ = fs::remove_file(&pid_file);
        let _ = fs::remove_file(&cfg_path);
        bail!(
            "Daemon process exited immediately. Check logs at {}",
            log_file_path.display()
        );
    }

    println!("Daemon started (PID {})", pid);
    Ok(())
}

/// Handles `bm daemon stop`.
pub fn stop(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let pid_file = pid_path(&team.name)?;
    if !pid_file.exists() {
        bail!("Daemon not running for team '{}'", team.name);
    }

    let pid_str = fs::read_to_string(&pid_file)
        .context("Failed to read daemon PID file")?;
    let pid: u32 = pid_str
        .trim()
        .parse()
        .context("Invalid PID in daemon PID file")?;

    if state::is_alive(pid) {
        // Send SIGTERM
        unsafe {
            libc::kill(pid as i32, libc::SIGTERM);
        }

        // Wait up to 30 seconds
        for _ in 0..30 {
            if !state::is_alive(pid) {
                break;
            }
            thread::sleep(Duration::from_secs(1));
        }

        // If still alive, SIGKILL
        if state::is_alive(pid) {
            unsafe {
                libc::kill(pid as i32, libc::SIGKILL);
            }
        }
    }

    // Clean up files
    let _ = fs::remove_file(&pid_file);
    let cfg_file = config_path(&team.name)?;
    let _ = fs::remove_file(&cfg_file);
    let poll_file = poll_state_path(&team.name)?;
    let _ = fs::remove_file(&poll_file);

    println!("Daemon stopped");
    Ok(())
}

/// Handles `bm daemon status`.
pub fn status(team_flag: Option<&str>) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;

    let pid_file = pid_path(&team.name)?;
    if !pid_file.exists() {
        println!("Daemon: not running");
        return Ok(());
    }

    let pid_str = fs::read_to_string(&pid_file)
        .context("Failed to read daemon PID file")?;
    let pid: u32 = match pid_str.trim().parse() {
        Ok(p) => p,
        Err(_) => {
            println!("Daemon: not running (corrupt PID file)");
            let _ = fs::remove_file(&pid_file);
            return Ok(());
        }
    };

    if !state::is_alive(pid) {
        println!("Daemon: not running (stale PID file)");
        // Clean up stale files
        let _ = fs::remove_file(&pid_file);
        let cfg_file = config_path(&team.name)?;
        let _ = fs::remove_file(&cfg_file);
        return Ok(());
    }

    // Read daemon config for details
    let cfg_file = config_path(&team.name)?;
    if cfg_file.exists() {
        let contents = fs::read_to_string(&cfg_file)
            .context("Failed to read daemon config")?;
        if let Ok(daemon_cfg) = serde_json::from_str::<DaemonConfig>(&contents) {
            println!("Daemon: running (PID {})", pid);
            match daemon_cfg.mode.as_str() {
                "webhook" => println!("Mode: webhook (port {})", daemon_cfg.port),
                "poll" => println!("Mode: poll (interval {}s)", daemon_cfg.interval_secs),
                other => println!("Mode: {}", other),
            }
            println!("Team: {}", daemon_cfg.team);
            println!("Started: {}", format_timestamp(&daemon_cfg.started_at));
            return Ok(());
        }
    }

    // Fallback: PID exists but no config
    println!("Daemon: running (PID {})", pid);
    println!("Team: {}", team.name);

    Ok(())
}

// ── Daemon event loop (called by hidden `bm daemon-run` command) ─────

/// Runs the daemon event loop. Called by the hidden `bm daemon-run` command.
/// This function does not return until the daemon is signaled to stop.
pub fn run_daemon(
    team_name: &str,
    mode: &str,
    port: u16,
    interval: u64,
) -> Result<()> {
    // Set up signal handling for graceful shutdown
    let shutdown = Arc::new(AtomicBool::new(false));
    {
        let shutdown = Arc::clone(&shutdown);
        unsafe {
            libc::signal(
                libc::SIGTERM,
                sigterm_handler as *const () as libc::sighandler_t,
            );
            libc::signal(
                libc::SIGINT,
                sigterm_handler as *const () as libc::sighandler_t,
            );
        }
        // Use a thread to poll for the signal flag
        SHUTDOWN_FLAG.store(false, Ordering::SeqCst);
        let s = shutdown;
        thread::spawn(move || {
            loop {
                if SHUTDOWN_FLAG.load(Ordering::SeqCst) {
                    s.store(true, Ordering::SeqCst);
                    break;
                }
                thread::sleep(Duration::from_millis(200));
            }
        });
    }

    daemon_log(team_name, "INFO", &format!("Daemon starting in {} mode", mode));

    match mode {
        "webhook" => run_webhook_mode(team_name, port, &shutdown),
        "poll" => run_poll_mode(team_name, interval, &shutdown),
        _ => bail!("Invalid daemon mode: {}", mode),
    }
}

// Global flag set by SIGTERM handler
static SHUTDOWN_FLAG: AtomicBool = AtomicBool::new(false);

extern "C" fn sigterm_handler(_sig: libc::c_int) {
    SHUTDOWN_FLAG.store(true, Ordering::SeqCst);
}

/// Runs the daemon in webhook mode using tiny_http.
fn run_webhook_mode(
    team_name: &str,
    port: u16,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    let addr = format!("0.0.0.0:{}", port);
    let server = tiny_http::Server::http(&addr)
        .map_err(|e| anyhow::anyhow!("Failed to bind to {}: {}", addr, e))?;

    daemon_log(team_name, "INFO", &format!("Webhook server listening on port {}", port));

    // Load webhook secret if configured
    let webhook_secret = load_webhook_secret(team_name);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            daemon_log(team_name, "INFO", "Received shutdown signal, stopping webhook server");
            break;
        }

        // Non-blocking accept with timeout
        match server.recv_timeout(Duration::from_secs(1)) {
            Ok(Some(mut request)) => {
                let path = request.url().to_string();
                let method = request.method().to_string();

                if method != "POST" || path != "/webhook" {
                    let response = tiny_http::Response::from_string("Not Found")
                        .with_status_code(404);
                    let _ = request.respond(response);
                    continue;
                }

                // Read body
                let mut body = String::new();
                if let Err(e) = request.as_reader().read_to_string(&mut body) {
                    daemon_log(team_name, "ERROR", &format!("Failed to read request body: {}", e));
                    let response = tiny_http::Response::from_string("Bad Request")
                        .with_status_code(400);
                    let _ = request.respond(response);
                    continue;
                }

                // Validate signature if webhook secret is configured
                if let Some(ref secret) = webhook_secret {
                    let sig_header = request
                        .headers()
                        .iter()
                        .find(|h| h.field.as_str() == "X-Hub-Signature-256"
                            || h.field.as_str() == "x-hub-signature-256")
                        .map(|h| h.value.as_str().to_string());

                    if !validate_webhook_signature(secret, &body, sig_header.as_deref()) {
                        daemon_log(team_name, "WARN", "Webhook signature validation failed");
                        let response = tiny_http::Response::from_string("Forbidden")
                            .with_status_code(403);
                        let _ = request.respond(response);
                        continue;
                    }
                }

                // Parse event type from header
                let event_type = request
                    .headers()
                    .iter()
                    .find(|h| h.field.as_str() == "X-GitHub-Event"
                        || h.field.as_str() == "x-github-event")
                    .map(|h| h.value.as_str().to_string());

                let response = tiny_http::Response::from_string("OK")
                    .with_status_code(200);
                let _ = request.respond(response);

                if let Some(event_type) = event_type {
                    if is_relevant_event(&event_type) {
                        daemon_log(
                            team_name,
                            "INFO",
                            &format!("Received relevant event: {}", event_type),
                        );
                        // Launch members one-shot (blocks until all exit)
                        handle_member_launch(team_name, shutdown);
                    } else {
                        daemon_log(
                            team_name,
                            "DEBUG",
                            &format!("Ignoring irrelevant event: {}", event_type),
                        );
                    }
                }
            }
            Ok(None) => {
                // Timeout — no request, check shutdown flag on next iteration
            }
            Err(e) => {
                daemon_log(team_name, "ERROR", &format!("Server error: {}", e));
                thread::sleep(Duration::from_secs(1));
            }
        }
    }

    daemon_log(team_name, "INFO", "Daemon stopped");
    Ok(())
}

/// Runs the daemon in poll mode using gh API.
fn run_poll_mode(
    team_name: &str,
    interval: u64,
    shutdown: &Arc<AtomicBool>,
) -> Result<()> {
    daemon_log(team_name, "INFO", &format!("Poll mode started, interval: {}s", interval));

    // Load poll state
    let poll_state_file = poll_state_path(team_name)?;
    let mut poll_state = load_poll_state(&poll_state_file);

    loop {
        if shutdown.load(Ordering::SeqCst) {
            daemon_log(team_name, "INFO", "Received shutdown signal, stopping poll loop");
            break;
        }

        // Resolve GitHub repo for this team
        let github_repo = match resolve_github_repo(team_name) {
            Ok(repo) => repo,
            Err(e) => {
                daemon_log(team_name, "ERROR", &format!("Failed to resolve GitHub repo: {}", e));
                sleep_interruptible(interval, shutdown);
                continue;
            }
        };

        // Poll for events
        match poll_github_events(&github_repo, &poll_state) {
            Ok(events) => {
                let relevant_count = events
                    .iter()
                    .filter(|e| is_relevant_event(&e.event_type))
                    .count();

                if relevant_count > 0 {
                    daemon_log(
                        team_name,
                        "INFO",
                        &format!("Found {} relevant event(s)", relevant_count),
                    );
                    // Launch members one-shot (blocks until all exit)
                    handle_member_launch(team_name, shutdown);
                }

                // Update poll state with latest event ID
                if let Some(latest) = events.first() {
                    poll_state.last_event_id = Some(latest.id.clone());
                }
                poll_state.last_poll_at = Some(chrono::Utc::now().to_rfc3339());
                save_poll_state(&poll_state_file, &poll_state);
            }
            Err(e) => {
                daemon_log(
                    team_name,
                    "ERROR",
                    &format!("Failed to poll GitHub events: {}", e),
                );
            }
        }

        sleep_interruptible(interval, shutdown);
    }

    daemon_log(team_name, "INFO", "Daemon stopped");
    Ok(())
}

/// Launches members one-shot with logging.
fn handle_member_launch(team_name: &str, shutdown: &Arc<AtomicBool>) {
    match launch_members_oneshot(team_name, shutdown) {
        Ok(count) => {
            daemon_log(
                team_name,
                "INFO",
                &format!("One-shot run complete: {} member(s) processed", count),
            );
        }
        Err(e) => {
            daemon_log(
                team_name,
                "ERROR",
                &format!("Member launch failed: {}", e),
            );
        }
    }
}

/// Sleeps for the given duration, checking the shutdown flag every second.
fn sleep_interruptible(seconds: u64, shutdown: &Arc<AtomicBool>) {
    for _ in 0..seconds {
        if shutdown.load(Ordering::SeqCst) {
            break;
        }
        thread::sleep(Duration::from_secs(1));
    }
}

/// Waits for a child process to exit, checking the shutdown flag every 500ms.
///
/// If the shutdown flag is set while the child is still running, sends SIGTERM
/// to the child, waits up to 5 seconds, then escalates to SIGKILL.
///
/// Returns `Some(status)` if the child exited normally, or `None` if it was
/// terminated due to shutdown.
fn wait_interruptible(
    child: &mut std::process::Child,
    shutdown: &Arc<AtomicBool>,
) -> Option<std::process::ExitStatus> {
    loop {
        match child.try_wait() {
            Ok(Some(status)) => return Some(status),
            Ok(None) => {
                // Child still running — check shutdown flag
                if shutdown.load(Ordering::SeqCst) {
                    // Graceful: SIGTERM first
                    let pid = child.id();
                    unsafe {
                        libc::kill(pid as i32, libc::SIGTERM);
                    }
                    // Wait up to 5 seconds for child to exit
                    for _ in 0..10 {
                        thread::sleep(Duration::from_millis(500));
                        if let Ok(Some(_)) = child.try_wait() {
                            return None;
                        }
                    }
                    // Escalate to SIGKILL
                    unsafe {
                        libc::kill(pid as i32, libc::SIGKILL);
                    }
                    let _ = child.wait();
                    return None;
                }
                thread::sleep(Duration::from_millis(500));
            }
            Err(_) => return None,
        }
    }
}

// ── One-shot member launch ──────────────────────────────────────────

/// Launches all team members one-shot and waits for them to exit.
/// Returns the number of members launched.
fn launch_members_oneshot(team_name: &str, shutdown: &Arc<AtomicBool>) -> Result<u32> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, Some(team_name))?;
    let team_repo = team.path.join("team");

    // Discover members
    let members_dir = team_repo.join("team");
    if !members_dir.is_dir() {
        daemon_log(team_name, "WARN", "No members directory found");
        return Ok(0);
    }

    let member_dirs = list_member_dirs(&members_dir)?;
    if member_dirs.is_empty() {
        daemon_log(team_name, "WARN", "No members found");
        return Ok(0);
    }

    // Get credentials
    let gh_token = team
        .credentials
        .gh_token
        .as_deref()
        .unwrap_or("");
    let telegram_token = team.credentials.telegram_bot_token.as_deref();

    let workzone = &cfg.workzone;
    let team_ws_base = workzone.join(team_name);

    let mut children: Vec<(String, std::process::Child)> = Vec::new();

    for member_dir_name in &member_dirs {
        let ws = find_workspace(&team_ws_base, member_dir_name);
        let ws = match ws {
            Some(ws) => ws,
            None => {
                daemon_log(
                    team_name,
                    "WARN",
                    &format!("{}: no workspace found, skipping", member_dir_name),
                );
                continue;
            }
        };

        match launch_ralph_oneshot(&ws, gh_token, telegram_token, team_name, member_dir_name) {
            Ok(child) => {
                daemon_log(
                    team_name,
                    "INFO",
                    &format!("{}: launched (PID {})", member_dir_name, child.id()),
                );
                children.push((member_dir_name.clone(), child));
            }
            Err(e) => {
                daemon_log(
                    team_name,
                    "ERROR",
                    &format!("{}: failed to launch — {}", member_dir_name, e),
                );
            }
        }
    }

    let launched = children.len() as u32;

    // Wait for all members to exit (interruptible by shutdown signal)
    for (name, mut child) in children {
        match wait_interruptible(&mut child, shutdown) {
            Some(status) => {
                daemon_log(
                    team_name,
                    "INFO",
                    &format!("{}: exited ({})", name, status),
                );
            }
            None => {
                daemon_log(
                    team_name,
                    "INFO",
                    &format!("{}: terminated due to shutdown", name),
                );
            }
        }
    }

    Ok(launched)
}

/// Lists member directory names under `team/team/`.
fn list_member_dirs(team_dir: &Path) -> Result<Vec<String>> {
    let mut dirs = Vec::new();
    for entry in fs::read_dir(team_dir)? {
        let entry = entry?;
        if !entry.file_type()?.is_dir() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().to_string();
        if name.starts_with('.') {
            continue;
        }
        dirs.push(name);
    }
    dirs.sort();
    Ok(dirs)
}

/// Finds the workspace path for a member (same logic as start.rs).
fn find_workspace(team_ws_base: &Path, member_dir_name: &str) -> Option<PathBuf> {
    let member_ws = team_ws_base.join(member_dir_name);
    if !member_ws.is_dir() {
        return None;
    }

    // Check for project subdirectories
    if let Ok(entries) = fs::read_dir(&member_ws) {
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                let name = entry.file_name().to_string_lossy().to_string();
                if !name.starts_with('.') && path.join(".botminter").is_dir() {
                    return Some(path);
                }
            }
        }
    }

    // No-project mode
    if member_ws.join(".botminter").is_dir() {
        return Some(member_ws);
    }

    None
}

/// Launches ralph one-shot (blocking child — we hold the Child handle to wait on it).
fn launch_ralph_oneshot(
    workspace: &Path,
    gh_token: &str,
    telegram_token: Option<&str>,
    team_name: &str,
    member_name: &str,
) -> Result<std::process::Child> {
    let mut cmd = Command::new("ralph");
    cmd.args(["run", "-p", "PROMPT.md"])
        .current_dir(workspace)
        .env("GH_TOKEN", gh_token)
        .env_remove("CLAUDECODE");

    if let Some(token) = telegram_token {
        cmd.env("RALPH_TELEGRAM_BOT_TOKEN", token);
    }

    // One-shot: null stdin
    cmd.stdin(std::process::Stdio::null());

    // Redirect stdout/stderr to per-member log file
    let log_file_path = member_log_path(team_name, member_name)?;
    daemon_log(
        team_name,
        "INFO",
        &format!("{}: log file at {}", member_name, log_file_path.display()),
    );
    let log_file = fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&log_file_path)
        .with_context(|| {
            format!(
                "Failed to open member log file at {}",
                log_file_path.display()
            )
        })?;
    let log_file_err = log_file
        .try_clone()
        .context("Failed to clone member log file handle")?;
    cmd.stdout(log_file).stderr(log_file_err);

    let child = cmd.spawn().with_context(|| {
        format!("Failed to spawn ralph in {}", workspace.display())
    })?;

    Ok(child)
}

// ── GitHub event types ──────────────────────────────────────────────

/// A GitHub event from the events API.
#[derive(Debug, Deserialize)]
pub struct GitHubEvent {
    pub id: String,
    #[serde(rename = "type")]
    pub event_type: String,
}

/// Checks if an event type is relevant for triggering member launches.
pub fn is_relevant_event(event_type: &str) -> bool {
    // The events API uses PascalCase type names, webhook headers use snake_case
    let normalized = event_type.to_lowercase();
    RELEVANT_EVENTS.iter().any(|&re| {
        normalized == re
            || normalized == re.replace('_', "")
            // Events API format: IssuesEvent, IssueCommentEvent, PullRequestEvent
            || normalized == format!("{}event", re.replace('_', ""))
    })
}

/// Polls the GitHub events API for new events.
fn poll_github_events(
    github_repo: &str,
    poll_state: &PollState,
) -> Result<Vec<GitHubEvent>> {
    let output = Command::new("gh")
        .args([
            "api",
            &format!("repos/{}/events", github_repo),
            "--paginate",
            "--jq",
            "[.[] | {id: .id, type: .type}]",
        ])
        .output()
        .context("Failed to run gh api command")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        bail!("gh api failed: {}", stderr.trim());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    if stdout.trim().is_empty() {
        return Ok(Vec::new());
    }

    let events: Vec<GitHubEvent> = serde_json::from_str(&stdout)
        .context("Failed to parse GitHub events response")?;

    // Filter to events newer than last_event_id
    if let Some(ref last_id) = poll_state.last_event_id {
        let new_events: Vec<GitHubEvent> = events
            .into_iter()
            .take_while(|e| &e.id != last_id)
            .collect();
        Ok(new_events)
    } else {
        Ok(events)
    }
}

/// Resolves the GitHub repo (owner/name) for a team.
fn resolve_github_repo(team_name: &str) -> Result<String> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, Some(team_name))?;
    if team.github_repo.is_empty() {
        bail!("No GitHub repo configured for team '{}'", team_name);
    }
    Ok(team.github_repo.clone())
}

// ── Webhook signature validation ────────────────────────────────────

/// Validates a GitHub webhook signature using HMAC-SHA256.
pub fn validate_webhook_signature(
    secret: &str,
    body: &str,
    signature_header: Option<&str>,
) -> bool {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;

    let sig = match signature_header {
        Some(s) => s,
        None => return false,
    };

    // GitHub sends "sha256=<hex>"
    let hex_sig = match sig.strip_prefix("sha256=") {
        Some(h) => h,
        None => return false,
    };

    let expected = match hex::decode(hex_sig) {
        Ok(b) => b,
        Err(_) => return false,
    };

    let mut mac = match Hmac::<Sha256>::new_from_slice(secret.as_bytes()) {
        Ok(m) => m,
        Err(_) => return false,
    };
    mac.update(body.as_bytes());

    mac.verify_slice(&expected).is_ok()
}

/// Loads the webhook secret from the team's credentials.
fn load_webhook_secret(team_name: &str) -> Option<String> {
    let cfg = config::load().ok()?;
    let team = config::resolve_team(&cfg, Some(team_name)).ok()?;
    team.credentials.webhook_secret.clone()
}

// ── Poll state persistence ──────────────────────────────────────────

fn load_poll_state(path: &Path) -> PollState {
    if !path.exists() {
        return PollState::default();
    }
    match fs::read_to_string(path) {
        Ok(contents) => serde_json::from_str(&contents).unwrap_or_default(),
        Err(_) => PollState::default(),
    }
}

fn save_poll_state(path: &Path, state: &PollState) {
    if let Ok(contents) = serde_json::to_string_pretty(state) {
        let _ = fs::write(path, contents);
    }
}

// ── Logging ─────────────────────────────────────────────────────────

/// Writes a log entry to the daemon's log file.
pub fn daemon_log(team_name: &str, level: &str, message: &str) {
    let timestamp = chrono::Utc::now().format("%Y-%m-%dT%H:%M:%SZ");
    let line = format!("[{}] [{}] {}\n", timestamp, level, message);

    // Also print to stdout/stderr (redirected to log file by the parent)
    eprint!("{}", line);

    // Try direct file write as backup (in case stderr isn't redirected)
    if let Ok(log_file) = log_path(team_name) {
        // Rotate if too large
        if let Ok(meta) = fs::metadata(&log_file) {
            if meta.len() > MAX_LOG_SIZE {
                let rotated = log_file.with_extension("log.old");
                let _ = fs::rename(&log_file, rotated);
            }
        }
        if let Ok(mut f) = fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&log_file)
        {
            let _ = f.write_all(line.as_bytes());
        }
    }
}

/// Reads the schema version from the team's botminter.yml.
fn read_team_schema(team_repo: &Path) -> Result<String> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        bail!(
            "Team repo at {} has no botminter.yml",
            team_repo.display()
        );
    }
    let contents = fs::read_to_string(&manifest_path)
        .context("Failed to read team botminter.yml")?;
    let val: serde_yml::Value =
        serde_yml::from_str(&contents).context("Failed to parse team botminter.yml")?;
    Ok(val["schema_version"]
        .as_str()
        .unwrap_or("")
        .to_string())
}

/// Formats an ISO 8601 timestamp for display.
fn format_timestamp(ts: &str) -> String {
    if let Ok(dt) = chrono::DateTime::parse_from_rfc3339(ts) {
        dt.format("%Y-%m-%d %H:%M:%S UTC").to_string()
    } else {
        ts.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn daemon_config_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("daemon.json");

        let cfg = DaemonConfig {
            team: "my-team".to_string(),
            mode: "webhook".to_string(),
            port: 8484,
            interval_secs: 60,
            pid: 12345,
            started_at: "2026-02-21T10:00:00Z".to_string(),
        };

        let contents = serde_json::to_string_pretty(&cfg).unwrap();
        fs::write(&path, &contents).unwrap();

        let loaded_str = fs::read_to_string(&path).unwrap();
        let loaded: DaemonConfig = serde_json::from_str(&loaded_str).unwrap();

        assert_eq!(loaded.team, "my-team");
        assert_eq!(loaded.mode, "webhook");
        assert_eq!(loaded.port, 8484);
        assert_eq!(loaded.interval_secs, 60);
        assert_eq!(loaded.pid, 12345);
    }

    #[test]
    fn poll_state_round_trip() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("poll.json");

        let state = PollState {
            last_event_id: Some("12345678".to_string()),
            last_poll_at: Some("2026-02-21T10:00:00Z".to_string()),
        };

        let contents = serde_json::to_string_pretty(&state).unwrap();
        fs::write(&path, &contents).unwrap();

        let loaded_str = fs::read_to_string(&path).unwrap();
        let loaded: PollState = serde_json::from_str(&loaded_str).unwrap();

        assert_eq!(loaded.last_event_id, Some("12345678".to_string()));
    }

    #[test]
    fn poll_state_default_is_empty() {
        let state = PollState::default();
        assert!(state.last_event_id.is_none());
        assert!(state.last_poll_at.is_none());
    }

    #[test]
    fn format_timestamp_rfc3339() {
        let result = format_timestamp("2026-02-21T10:30:00+00:00");
        assert_eq!(result, "2026-02-21 10:30:00 UTC");
    }

    #[test]
    fn format_timestamp_unparseable() {
        let result = format_timestamp("not-a-timestamp");
        assert_eq!(result, "not-a-timestamp");
    }

    // ── Event filtering tests ────────────────────────────────────────

    #[test]
    fn relevant_event_types_webhook_format() {
        assert!(is_relevant_event("issues"));
        assert!(is_relevant_event("issue_comment"));
        assert!(is_relevant_event("pull_request"));
    }

    #[test]
    fn relevant_event_types_api_format() {
        // The events API uses PascalCase like "IssuesEvent"
        assert!(is_relevant_event("IssuesEvent"));
        assert!(is_relevant_event("IssueCommentEvent"));
        assert!(is_relevant_event("PullRequestEvent"));
    }

    #[test]
    fn irrelevant_event_types() {
        assert!(!is_relevant_event("push"));
        assert!(!is_relevant_event("PushEvent"));
        assert!(!is_relevant_event("create"));
        assert!(!is_relevant_event("delete"));
        assert!(!is_relevant_event("fork"));
        assert!(!is_relevant_event("watch"));
        assert!(!is_relevant_event("star"));
    }

    // ── Webhook signature tests ──────────────────────────────────────

    #[test]
    fn webhook_signature_valid() {
        use hmac::{Hmac, Mac};
        use sha2::Sha256;

        let secret = "mysecret";
        let body = r#"{"action":"opened"}"#;

        // Compute expected signature
        let mut mac = Hmac::<Sha256>::new_from_slice(secret.as_bytes()).unwrap();
        mac.update(body.as_bytes());
        let result = mac.finalize();
        let hex_sig = hex::encode(result.into_bytes());
        let header = format!("sha256={}", hex_sig);

        assert!(validate_webhook_signature(secret, body, Some(&header)));
    }

    #[test]
    fn webhook_signature_invalid() {
        let secret = "mysecret";
        let body = r#"{"action":"opened"}"#;
        let bad_sig = "sha256=0000000000000000000000000000000000000000000000000000000000000000";

        assert!(!validate_webhook_signature(secret, body, Some(bad_sig)));
    }

    #[test]
    fn webhook_signature_missing_header() {
        assert!(!validate_webhook_signature("secret", "body", None));
    }

    #[test]
    fn webhook_signature_wrong_prefix() {
        assert!(!validate_webhook_signature("secret", "body", Some("sha1=abcd")));
    }

    #[test]
    fn webhook_signature_invalid_hex() {
        assert!(!validate_webhook_signature("secret", "body", Some("sha256=not-hex!!")));
    }

    // ── Poll state persistence tests ─────────────────────────────────

    #[test]
    fn poll_state_save_and_load() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("poll-state.json");

        let state = PollState {
            last_event_id: Some("99999".to_string()),
            last_poll_at: Some("2026-02-21T12:00:00Z".to_string()),
        };

        save_poll_state(&path, &state);
        let loaded = load_poll_state(&path);

        assert_eq!(loaded.last_event_id, Some("99999".to_string()));
        assert_eq!(loaded.last_poll_at, Some("2026-02-21T12:00:00Z".to_string()));
    }

    #[test]
    fn poll_state_load_missing_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("nonexistent.json");

        let state = load_poll_state(&path);
        assert!(state.last_event_id.is_none());
        assert!(state.last_poll_at.is_none());
    }

    #[test]
    fn poll_state_load_corrupt_file() {
        let tmp = tempfile::tempdir().unwrap();
        let path = tmp.path().join("corrupt.json");
        fs::write(&path, "not valid json!!!").unwrap();

        let state = load_poll_state(&path);
        assert!(state.last_event_id.is_none());
    }

    // ── GitHub event deserialization tests ────────────────────────────

    #[test]
    fn github_event_deser() {
        let json = r#"[{"id":"12345","type":"IssuesEvent"},{"id":"12346","type":"PushEvent"}]"#;
        let events: Vec<GitHubEvent> = serde_json::from_str(json).unwrap();

        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "12345");
        assert_eq!(events[0].event_type, "IssuesEvent");
        assert_eq!(events[1].id, "12346");
        assert_eq!(events[1].event_type, "PushEvent");
    }

    // ── Member discovery tests (daemon-internal) ─────────────────────

    #[test]
    fn list_member_dirs_returns_sorted() {
        let tmp = tempfile::tempdir().unwrap();
        fs::create_dir(tmp.path().join("bob")).unwrap();
        fs::create_dir(tmp.path().join("alice")).unwrap();
        fs::create_dir(tmp.path().join(".hidden")).unwrap();
        fs::write(tmp.path().join("file.txt"), "").unwrap();

        let result = list_member_dirs(tmp.path()).unwrap();
        assert_eq!(result, vec!["alice", "bob"]);
    }

    #[test]
    fn find_workspace_project_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let project_dir = tmp.path().join("member").join("my-project");
        fs::create_dir_all(project_dir.join(".botminter")).unwrap();

        let result = find_workspace(tmp.path(), "member");
        assert_eq!(result, Some(project_dir));
    }

    #[test]
    fn find_workspace_no_project_mode() {
        let tmp = tempfile::tempdir().unwrap();
        let member_dir = tmp.path().join("member");
        fs::create_dir_all(member_dir.join(".botminter")).unwrap();

        let result = find_workspace(tmp.path(), "member");
        assert_eq!(result, Some(member_dir));
    }

    #[test]
    fn find_workspace_missing() {
        let tmp = tempfile::tempdir().unwrap();
        let result = find_workspace(tmp.path(), "nonexistent");
        assert_eq!(result, None);
    }

    // ── Per-member log path tests ─────────────────────────────────────

    #[test]
    fn member_log_path_format() {
        let path = member_log_path("my-team", "alice").unwrap();
        let filename = path.file_name().unwrap().to_str().unwrap();
        assert_eq!(filename, "member-my-team-alice.log");
        assert!(path.to_str().unwrap().contains("logs"));
    }

    // ── wait_interruptible tests ──────────────────────────────────────

    #[test]
    fn wait_interruptible_returns_on_child_exit() {
        let mut child = Command::new("sleep")
            .arg("0.1")
            .spawn()
            .unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));

        let result = wait_interruptible(&mut child, &shutdown);
        assert!(result.is_some(), "Should return Some(status) on normal exit");
        assert!(result.unwrap().success(), "sleep 0.1 should exit 0");
    }

    #[test]
    fn wait_interruptible_terminates_on_shutdown() {
        let mut child = Command::new("sleep")
            .arg("999")
            .spawn()
            .unwrap();
        let shutdown = Arc::new(AtomicBool::new(false));

        // Set shutdown flag after a brief delay
        let shutdown_clone = Arc::clone(&shutdown);
        thread::spawn(move || {
            thread::sleep(Duration::from_millis(200));
            shutdown_clone.store(true, Ordering::SeqCst);
        });

        let result = wait_interruptible(&mut child, &shutdown);
        assert!(result.is_none(), "Should return None when shutdown triggered");
        // Verify the child is actually dead
        assert!(
            child.try_wait().unwrap().is_some(),
            "Child should be dead after shutdown"
        );
    }
}
