//! Shared helpers for E2E tests.

use std::fs;
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

/// Creates a `Command` for the `bm` binary.
pub fn bm_cmd() -> Command {
    Command::new(env!("CARGO_BIN_EXE_bm"))
}

/// Polls a TCP port until it accepts connections or the timeout expires.
pub fn wait_for_port(port: u16, timeout: Duration) {
    let start = Instant::now();
    let addr = format!("127.0.0.1:{}", port);

    while start.elapsed() < timeout {
        if TcpStream::connect(&addr).is_ok() {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }

    panic!(
        "timeout waiting for port {} after {:?}",
        port, timeout
    );
}

/// Runs a command, asserts exit 0, returns stdout.
pub fn assert_cmd_success(cmd: &mut Command) -> String {
    let output = cmd.output().expect("failed to run command");
    assert!(
        output.status.success(),
        "command failed with exit {}: stderr={}",
        output.status.code().unwrap_or(-1),
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).to_string()
}

/// Runs a command, asserts non-zero exit, returns stderr.
#[allow(dead_code)] // Infrastructure for task-08 (start-to-stop lifecycle)
pub fn assert_cmd_fails(cmd: &mut Command) -> String {
    let output = cmd.output().expect("failed to run command");
    assert!(
        !output.status.success(),
        "command succeeded unexpectedly: stdout={}",
        String::from_utf8_lossy(&output.stdout)
    );
    String::from_utf8_lossy(&output.stderr).to_string()
}

// ── Process helpers ─────────────────────────────────────────────────

/// Checks if a process is alive using kill(pid, 0).
pub fn is_alive(pid: u32) -> bool {
    unsafe { libc::kill(pid as i32, 0) == 0 }
}

/// Force-kills a process. Used for test cleanup.
pub fn force_kill(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGKILL);
    }
}

/// Waits until a process exits, with timeout.
pub fn wait_for_exit(pid: u32, timeout: Duration) {
    let start = Instant::now();
    while start.elapsed() < timeout {
        if !is_alive(pid) {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
    panic!(
        "Process {} did not exit within {:?}",
        pid, timeout
    );
}

// ── DaemonGuard ─────────────────────────────────────────────────────

/// RAII guard that stops and cleans up a daemon process on drop.
///
/// Use this in E2E tests that start a daemon to ensure cleanup even if
/// the test panics.
pub struct DaemonGuard {
    team_name: String,
    home: PathBuf,
    stub_dir: Option<PathBuf>,
}

impl DaemonGuard {
    /// Create a guard for a daemon with optional stub PATH override.
    pub fn new(home: &Path, team_name: &str, stub_dir: Option<&Path>) -> Self {
        DaemonGuard {
            team_name: team_name.to_string(),
            home: home.to_path_buf(),
            stub_dir: stub_dir.map(|p| p.to_path_buf()),
        }
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        // Try graceful stop via bm daemon stop
        let mut cmd = bm_cmd();
        cmd.args(["daemon", "stop", "-t", &self.team_name])
            .env("HOME", &self.home);
        if let Some(ref stub_dir) = self.stub_dir {
            cmd.env(
                "PATH",
                format!(
                    "{}:{}",
                    stub_dir.display(),
                    std::env::var("PATH").unwrap_or_default()
                ),
            );
        }
        let _ = cmd.output();

        // Force-kill via PID file if still alive
        let pid_file = self
            .home
            .join(format!(".botminter/daemon-{}.pid", self.team_name));
        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
            if let Ok(pid) = pid_str.trim().parse::<u32>() {
                if is_alive(pid) {
                    force_kill(pid);
                    std::thread::sleep(Duration::from_millis(200));
                }
            }
        }

        // Clean up files
        let _ = fs::remove_file(
            self.home
                .join(format!(".botminter/daemon-{}.pid", self.team_name)),
        );
        let _ = fs::remove_file(
            self.home
                .join(format!(".botminter/daemon-{}.json", self.team_name)),
        );
        let _ = fs::remove_file(
            self.home
                .join(format!(".botminter/daemon-{}-poll.json", self.team_name)),
        );
    }
}
