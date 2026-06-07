//! Shared helpers for E2E tests.

use std::fs;
use std::net::TcpStream;
use std::ops::Range;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use libtest_mimic::Trial;

// ── E2eConfig + Progressive Mode ────────────────────────────────────

/// Configuration for E2E tests, parsed from CLI arguments.
#[derive(Clone)]
pub struct E2eConfig {
    pub gh_token: String,
    pub gh_org: String,
    pub progressive: Option<ProgressiveMode>,
    /// GitHub App credentials for --reuse-app (pre-provisioned test App).
    pub app_id: String,
    pub app_client_id: String,
    pub app_installation_id: String,
    pub app_private_key_file: String,
}

#[derive(Clone, Debug)]
pub enum ProgressiveMode {
    Step(Option<String>),
    Reset(Option<String>),
}

/// Persisted state for a progressive suite run.
#[derive(serde::Serialize, serde::Deserialize)]
pub struct ProgressState {
    pub suite_name: String,
    pub repo_full_name: String,
    pub home_dir: String,
    pub next_case: usize,
    pub total_cases: usize,
    pub setup_done: bool,
    pub tg_mock_container_id: Option<String>,
    pub tg_mock_port: Option<u16>,
    #[serde(default)]
    pub rc_pod_name: Option<String>,
    #[serde(default)]
    pub rc_pod_port: Option<u16>,
}

impl ProgressState {
    pub fn progress_base() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("target/e2e-progress")
    }

    pub fn state_dir(suite_name: &str) -> PathBuf {
        Self::progress_base().join(suite_name)
    }

    fn state_file(suite_name: &str) -> PathBuf {
        Self::progress_base().join(format!("{}.json", suite_name))
    }

    pub fn load(suite_name: &str) -> Option<Self> {
        let content = fs::read_to_string(Self::state_file(suite_name)).ok()?;
        serde_json::from_str(&content).ok()
    }

    pub fn save(&self) {
        fs::create_dir_all(Self::progress_base()).unwrap();
        let json = serde_json::to_string_pretty(self).unwrap();
        fs::write(Self::state_file(&self.suite_name), json).unwrap();
    }

    pub fn delete(suite_name: &str) {
        let _ = fs::remove_file(Self::state_file(suite_name));
        let _ = fs::remove_dir_all(Self::state_dir(suite_name));
    }

    pub fn list_all() -> Vec<String> {
        let Ok(entries) = fs::read_dir(Self::progress_base()) else {
            return Vec::new();
        };
        entries
            .filter_map(|e| e.ok())
            .filter_map(|e| {
                let name = e.file_name().to_string_lossy().to_string();
                name.strip_suffix(".json").map(|s| s.to_string())
            })
            .collect()
    }
}

// ── Command helpers ─────────────────────────────────────────────────

pub fn run_test<F: FnOnce() + std::panic::UnwindSafe>(f: F) -> Result<(), libtest_mimic::Failed> {
    match std::panic::catch_unwind(f) {
        Ok(()) => Ok(()),
        Err(e) => Err(panic_to_string(e).into()),
    }
}

/// Finds a free TCP port by binding to port 0.
pub fn find_free_port() -> u16 {
    std::net::TcpListener::bind("127.0.0.1:0")
        .expect("failed to bind to a free port")
        .local_addr()
        .expect("failed to get local address")
        .port()
}

pub fn wait_for_port(port: u16, timeout: Duration) {
    let start = Instant::now();
    let addr = format!("127.0.0.1:{}", port);
    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("timeout waiting for port {} after {:?}", port, timeout);
}

pub fn preflight_gh_auth() {
    // Check GH_TOKEN first — it takes priority over `gh auth login` sessions.
    // `gh auth status` returns non-zero if ANY configured account is invalid,
    // even when GH_TOKEN provides a valid session. Validate GH_TOKEN directly.
    if std::env::var("GH_TOKEN").map(|t| !t.is_empty()).unwrap_or(false) {
        let output = Command::new("gh")
            .args(["api", "user", "--jq", ".login"])
            .output()
            .expect("failed to run `gh api user` — is gh installed?");
        assert!(
            output.status.success(),
            "GH_TOKEN is set but invalid. Check your token.\nstderr: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    }

    let output = Command::new("gh")
        .args(["auth", "status"])
        .output()
        .expect("failed to run `gh auth status` — is gh installed?");
    assert!(
        output.status.success(),
        "GitHub CLI is not authenticated. Run `gh auth login` or set GH_TOKEN.\nstderr: {}",
        String::from_utf8_lossy(&output.stderr)
    );
}

// ── Process helpers ─────────────────────────────────────────────────

pub fn is_alive(pid: u32) -> bool {
    bm::state::is_alive(pid)
}

pub fn force_kill(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

pub fn wait_for_exit(pid: u32, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !is_alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!("Process {} did not exit within {:?}", pid, timeout);
}

// ── Session workspace helpers ───────────────────────────────────────

/// Find the most-recently-created ephemeral session workspace for a member.
///
/// Session workspaces live at `~/.botminter/sessions/<team>/<member>/<session_id>/`.
pub fn find_session_workspace(home: &Path, team_name: &str, member_name: &str) -> Option<PathBuf> {
    let sessions_dir = home
        .join(".botminter")
        .join("sessions")
        .join(team_name)
        .join(member_name);
    let mut entries: Vec<_> = fs::read_dir(&sessions_dir)
        .ok()?
        .filter_map(|e| e.ok())
        .filter(|e| e.path().is_dir())
        .collect();
    // Sort by modification time ascending — last element is most recent
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entries.last().map(|e| e.path())
}

/// Poll until the session workspace appears on disk, up to `timeout`.
pub fn wait_for_session_workspace(
    home: &Path,
    team_name: &str,
    member_name: &str,
    timeout: Duration,
) -> Option<PathBuf> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(ws) = find_session_workspace(home, team_name, member_name) {
            return Some(ws);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Read the PID written by stub-ralph.sh to `<session_ws>/.ralph-stub-pid`.
pub fn read_stub_pid(session_ws: &Path) -> Option<u32> {
    fs::read_to_string(session_ws.join(".ralph-stub-pid"))
        .ok()?
        .trim()
        .parse()
        .ok()
}

/// Poll until stub-ralph writes its PID file, up to `timeout`.
pub fn wait_for_stub_pid(session_ws: &Path, timeout: Duration) -> Option<u32> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if let Some(pid) = read_stub_pid(session_ws) {
            return Some(pid);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

/// Returns all session workspace directories for a member, sorted by modification time ascending.
pub fn list_session_workspaces(home: &Path, team_name: &str, member_name: &str) -> Vec<PathBuf> {
    let sessions_dir = home.join(".botminter").join("sessions").join(team_name).join(member_name);
    let mut entries: Vec<_> = fs::read_dir(&sessions_dir)
        .ok()
        .into_iter()
        .flat_map(|rd| rd.filter_map(|e| e.ok()))
        .filter(|e| e.path().is_dir())
        .collect();
    entries.sort_by_key(|e| e.metadata().and_then(|m| m.modified()).ok());
    entries.into_iter().map(|e| e.path()).collect()
}

/// Wait for a NEW session workspace that is NOT in the excluded set to appear.
pub fn wait_for_new_session_workspace(
    home: &Path,
    team_name: &str,
    member_name: &str,
    excluded: &std::collections::HashSet<PathBuf>,
    timeout: Duration,
) -> Option<PathBuf> {
    let start = Instant::now();
    while start.elapsed() < timeout {
        let workspaces = list_session_workspaces(home, team_name, member_name);
        if let Some(ws) = workspaces.into_iter().find(|p| !excluded.contains(p)) {
            return Some(ws);
        }
        std::thread::sleep(Duration::from_millis(200));
    }
    None
}

pub struct ProcessGuard {
    pub pid: Option<u32>,
    team_name: String,
    /// Captured resolved env for "bm" at construction time (Drop can't access TestEnv).
    bm_env: std::collections::HashMap<String, String>,
}

impl ProcessGuard {
    pub fn new(env: &super::test_env::TestEnv, team_name: &str) -> Self {
        ProcessGuard {
            pid: None,
            team_name: team_name.to_string(),
            bm_env: env.resolved_env("bm"),
        }
    }

    pub fn set_pid(&mut self, pid: u32) {
        self.pid = Some(pid);
    }
}

impl Drop for ProcessGuard {
    fn drop(&mut self) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bm"));
        cmd.env_clear()
            .envs(&self.bm_env)
            .args(["stop", "--force", "-t", &self.team_name]);
        let _ = cmd.output();
        if let Some(pid) = self.pid {
            if is_alive(pid) {
                force_kill(pid);
                std::thread::sleep(Duration::from_millis(200));
            }
        }
    }
}

// ── DaemonGuard ─────────────────────────────────────────────────────

pub struct DaemonGuard {
    team_name: String,
    home: PathBuf,
    /// Captured resolved env for "bm" at construction time.
    bm_env: std::collections::HashMap<String, String>,
}

impl DaemonGuard {
    pub fn new(env: &super::test_env::TestEnv, team_name: &str) -> Self {
        DaemonGuard {
            team_name: team_name.to_string(),
            home: env.home.clone(),
            bm_env: env.resolved_env("bm"),
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        let mut cmd = Command::new(env!("CARGO_BIN_EXE_bm"));
        cmd.env_clear()
            .envs(&self.bm_env)
            .args(["daemon", "stop", "-t", &self.team_name]);
        let _ = cmd.output();

        let pid_file = self.home.join(format!(".botminter/daemon-{}.pid", self.team_name));
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if is_alive(pid) {
                    force_kill(pid);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }
        for suffix in ["pid", "json"] {
            let _ = fs::remove_file(self.home.join(format!(".botminter/daemon-{}.{}", self.team_name, suffix)));
        }
        let _ = fs::remove_file(self.home.join(format!(".botminter/daemon-{}-poll.json", self.team_name)));
    }
}

// ── Cleanup helpers ─────────────────────────────────────────────────

pub fn cleanup_project_boards(gh_org: &str, gh_token: &str, title_match: &str) {
    let output = Command::new("gh")
        .args(["project", "list", "--owner", gh_org, "--format", "json", "--limit", "100"])
        .env("GH_TOKEN", gh_token)
        .output()
        .expect("failed to list projects");

    if output.status.success() {
        let json: serde_json::Value =
            serde_json::from_slice(&output.stdout).unwrap_or(serde_json::Value::Null);
        if let Some(projects) = json["projects"].as_array() {
            for project in projects {
                let title = project["title"].as_str().unwrap_or("");
                if title.contains(title_match) {
                    if let Some(number) = project["number"].as_u64() {
                        eprintln!("Cleaning up project board #{}: {}", number, title);
                        let _ = Command::new("gh")
                            .args(["project", "delete", "--owner", gh_org, &number.to_string(), "--format", "json"])
                            .env("GH_TOKEN", gh_token)
                            .output();
                    }
                }
            }
        }
    }
}

/// Reads the GitHub repo name from a bm config file.
pub fn repo_from_config(home: &Path) -> String {
    let config_path = home.join(".botminter").join("config.yml");
    let config = bm::config::load_from(&config_path)
        .expect("failed to load bm config from home");
    config.teams[0].github_repo.clone()
}

// ── GithubSuite ─────────────────────────────────────────────────────

use super::test_env::TestEnv;

type CaseFn = Box<dyn Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe>;
type ErrorVerifier = Box<dyn Fn(&str) -> bool + Send + Sync>;

struct CaseEntry {
    name: String,
    func: CaseFn,
    /// When Some, the case is expected to panic. The verifier receives the panic message
    /// and returns true if the error is the expected one.
    expect_error: Option<ErrorVerifier>,
}

pub struct GithubSuite {
    name: String,
    repo_full_name: String,
    setup_fn: Option<CaseFn>,
    cases: Vec<CaseEntry>,
    groups: Vec<Range<usize>>,
}

impl GithubSuite {
    /// Create a suite where the test manages its own repo lifecycle.
    pub fn new_self_managed(name: &str, repo_full_name: &str) -> Self {
        GithubSuite {
            name: name.to_string(),
            repo_full_name: repo_full_name.to_string(),
            setup_fn: None,
            cases: Vec::new(),
            groups: Vec::new(),
        }
    }

    pub fn setup<F>(mut self, f: F) -> Self
    where
        F: Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static,
    {
        self.setup_fn = Some(Box::new(f));
        self
    }

    pub fn case<F>(mut self, name: &str, f: F) -> Self
    where
        F: Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static,
    {
        self.cases.push(CaseEntry {
            name: name.to_string(),
            func: Box::new(f),
            expect_error: None,
        });
        self
    }

    /// Register a case that is expected to fail (panic). The verifier receives the
    /// panic message and should return true if the error is the expected one.
    pub fn case_expect_error<F, V>(mut self, name: &str, f: F, verifier: V) -> Self
    where
        F: Fn(&mut TestEnv) + Send + std::panic::UnwindSafe + std::panic::RefUnwindSafe + 'static,
        V: Fn(&str) -> bool + Send + Sync + 'static,
    {
        self.cases.push(CaseEntry {
            name: name.to_string(),
            func: Box::new(f),
            expect_error: Some(Box::new(verifier)),
        });
        self
    }

    pub fn group(mut self, start: usize, end: usize) -> Self {
        self.groups.push(start..end + 1);
        self
    }

    fn case_range_at(&self, cursor: usize) -> Range<usize> {
        for g in &self.groups {
            if g.contains(&cursor) {
                return g.clone();
            }
        }
        cursor..cursor + 1
    }

    /// Build a progressive-mode Trial.
    pub fn build_progressive(self, config: &E2eConfig) -> Trial {
        let cfg = config.clone();
        let name = self.name.clone();
        Trial::test(name, move || {
            let suite_name = &self.name;
            let total = self.cases.len();

            let mut state = if let Some(s) = ProgressState::load(suite_name) {
                eprintln!("  [{}] resuming from case {}/{}", suite_name, s.next_case + 1, s.total_cases);
                s
            } else {
                let home_dir = ProgressState::state_dir(suite_name).join("home");
                fs::create_dir_all(&home_dir).unwrap();

                let s = ProgressState {
                    suite_name: suite_name.clone(),
                    repo_full_name: self.repo_full_name.clone(),
                    home_dir: home_dir.to_string_lossy().to_string(),
                    next_case: 0,
                    total_cases: total,
                    setup_done: false,
                    tg_mock_container_id: None,
                    tg_mock_port: None,
                    rc_pod_name: None,
                    rc_pod_port: None,
                };
                s.save();
                eprintln!("  [{}] created home", suite_name);
                s
            };

            let home_dir = PathBuf::from(&state.home_dir);

            // TestEnv::resume reuses existing HOME and restores exports
            let mut env = TestEnv::resume(
                home_dir,
                &cfg.gh_token,
                &cfg.gh_org,
                &state.repo_full_name,
            );

            if !state.setup_done {
                if let Some(ref setup) = self.setup_fn {
                    if let Err(e) = catch_unwind(AssertUnwindSafe(|| setup(&mut env))) {
                        let msg = panic_to_string(e);
                        eprintln!("  SETUP FAILED: {}", msg);
                        return Err(format!("suite setup failed: {}", msg).into());
                    }
                }
                state.setup_done = true;
                env.save();
                state.save();
                eprintln!("  setup complete");
            }

            let range = self.case_range_at(state.next_case);
            let range_end = range.end;
            let is_final_step = range_end >= total;

            let mut failures = Vec::new();
            for i in range {
                let entry = &self.cases[i];
                let next_name = if i + 1 < total {
                    format!(", next: {}", self.cases[i + 1].name)
                } else {
                    ", suite complete".to_string()
                };
                let result = if entry.expect_error.is_some() {
                    catch_unwind_silent(AssertUnwindSafe(|| (entry.func)(&mut env)))
                } else {
                    catch_unwind(AssertUnwindSafe(|| (entry.func)(&mut env)))
                };
                match result {
                    Ok(()) => {
                        if entry.expect_error.is_some() {
                            eprintln!("  FAILED {} (expected error but succeeded)", entry.name);
                            failures.push((entry.name.clone(), "expected error but case succeeded".to_string()));
                        } else {
                            eprintln!("  ok {}  ({}/{} done{})", entry.name, i + 1, total, next_name);
                        }
                    }
                    Err(e) => {
                        let msg = panic_to_string(e);
                        if let Some(ref verifier) = entry.expect_error {
                            if verifier(&msg) {
                                eprintln!("  ok {} (expected error verified)  ({}/{} done{})", entry.name, i + 1, total, next_name);
                            } else {
                                eprintln!("  FAILED {} (wrong error): {}", entry.name, msg);
                                failures.push((entry.name.clone(), format!("wrong error: {}", msg)));
                            }
                        } else {
                            eprintln!("  FAILED {}: {}", entry.name, msg);
                            failures.push((entry.name.clone(), msg));
                        }
                    }
                }

                // Save exports after each case for progressive resume
                env.save();
            }

            if !failures.is_empty() {
                eprintln!("  Case(s) failed, will retry on next run (cursor stays at {})", state.next_case);
                state.save();
                let msgs: Vec<String> = failures.iter().map(|(n, m)| format!("  {}: {}", n, m)).collect();
                return Err(format!("{} case(s) failed:\n{}", failures.len(), msgs.join("\n")).into());
            }

            state.next_case = range_end;

            if is_final_step {
                eprintln!("  [{}] suite complete — cleaning up", suite_name);
                ProgressState::delete(suite_name);
            } else {
                state.save();
            }

            Ok(())
        })
    }

    /// Build into a single libtest-mimic Trial (normal mode — runs all cases).
    pub fn build(self, config: &E2eConfig) -> Trial {
        let cfg = config.clone();
        let name = self.name.clone();
        Trial::test(name, move || {
            // TestEnv::fresh creates tempdir, bootstraps profiles, stub ralph,
            // git auth, isolated dbus + keyring — everything in one shot.
            let mut env = TestEnv::fresh(
                &cfg.gh_token,
                &cfg.gh_org,
                &self.repo_full_name,
            );

            if let Some(ref setup) = self.setup_fn {
                if let Err(e) = catch_unwind(AssertUnwindSafe(|| setup(&mut env))) {
                    let msg = panic_to_string(e);
                    eprintln!("  SETUP FAILED: {}", msg);
                    return Err(format!("suite setup failed: {}", msg).into());
                }
                eprintln!("  setup complete");
            }

            let total = self.cases.len();
            let mut failures = Vec::new();

            for entry in &self.cases {
                let result = if entry.expect_error.is_some() {
                    catch_unwind_silent(AssertUnwindSafe(|| (entry.func)(&mut env)))
                } else {
                    catch_unwind(AssertUnwindSafe(|| (entry.func)(&mut env)))
                };
                match result {
                    Ok(()) => {
                        if entry.expect_error.is_some() {
                            eprintln!("  FAILED {} (expected error but succeeded)", entry.name);
                            failures.push((entry.name.clone(), "expected error but case succeeded".to_string()));
                        } else {
                            eprintln!("  ok {}", entry.name);
                        }
                    }
                    Err(e) => {
                        let msg = panic_to_string(e);
                        if let Some(ref verifier) = entry.expect_error {
                            if verifier(&msg) {
                                eprintln!("  ok {} (expected error verified)", entry.name);
                            } else {
                                eprintln!("  FAILED {} (wrong error): {}", entry.name, msg);
                                failures.push((entry.name.clone(), format!("wrong error: {}", msg)));
                            }
                        } else {
                            eprintln!("  FAILED {}: {}", entry.name, msg);
                            failures.push((entry.name.clone(), msg));
                        }
                    }
                }
            }

            let passed = total - failures.len();
            eprintln!("  ({}/{} passed)", passed, total);

            if failures.is_empty() {
                Ok(())
            } else {
                let msgs: Vec<String> = failures.iter().map(|(n, m)| format!("  {}: {}", n, m)).collect();
                Err(format!("{} case(s) failed:\n{}", failures.len(), msgs.join("\n")).into())
            }
        })
    }
}

fn panic_to_string(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "unknown panic".to_string()
    }
}

/// Runs a closure with panic output suppressed, then restores the default hook.
///
/// This prevents `thread 'main' panicked at...` noise from `case_expect_error`
/// tests. The panic is still caught by `catch_unwind` — only stderr output is suppressed.
fn catch_unwind_silent<F: FnOnce() + std::panic::UnwindSafe>(
    f: F,
) -> Result<(), Box<dyn std::any::Any + Send>> {
    let prev = std::panic::take_hook();
    std::panic::set_hook(Box::new(|_| {}));
    let result = catch_unwind(f);
    std::panic::set_hook(prev);
    result
}
