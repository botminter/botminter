//! TestEnv and TestCommand — ADR-005 environment management for E2E tests.
//!
//! TestEnv is the test equivalent of a shell session. It holds environment state
//! and produces commands. All command execution in E2E tests goes through it.
//!
//! TestEnv applies three layers when producing a command:
//! 1. **Base** — applied to every command: test HOME, GH_TOKEN, PATH, git identity.
//! 2. **Include** — vars added only for specific binaries (e.g., `bm` gets BM_KEYRING_DBUS).
//! 3. **Override** — replaces a base var for specific binaries (e.g., `podman` gets real HOME).
//!
//! Like a shell, TestEnv supports two scopes:
//! - `env.export("KEY", val)` — all future commands get it (like `export KEY=val`).
//! - `env.command("bm").env("KEY", val)` — only this command gets it (like `KEY=val bm start`).

use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};
use std::time::Duration;

/// Path to the stub ralph script.
const STUB_RALPH_SCRIPT: &str = concat!(env!("CARGO_MANIFEST_DIR"), "/tests/e2e/stub-ralph.sh");

// ── TestEnv ─────────────────────────────────────────────────────────

/// Ownership mode determines Drop behavior for the HOME directory.
enum Ownership {
    /// TestEnv created the tempdir — Drop deletes it.
    Fresh(PathBuf),
    /// TestEnv reuses an existing HOME — Drop leaves it alone.
    Resume,
}

pub struct TestEnv {
    /// Test HOME directory.
    pub home: PathBuf,
    ownership: Ownership,

    /// Captured real system environment (full snapshot at construction time).
    originals: HashMap<String, String>,

    /// Base env applied to every command.
    base: HashMap<String, String>,

    /// Per-binary include rules: binary_name -> { key -> value }.
    includes: HashMap<String, HashMap<String, String>>,

    /// Per-binary override rules: binary_name -> { key -> value }.
    overrides: HashMap<String, HashMap<String, String>>,

    /// Cross-case exports (like `export KEY=val` in a shell).
    exports: HashMap<String, String>,

    /// Isolated D-Bus daemon PID (killed on Drop).
    dbus_pid: Option<u32>,

    /// Isolated D-Bus address (for keyring operations only).
    dbus_addr: String,

    /// D-Bus tmpdir — contains runtime/ and data/ for isolated keyring.
    dbus_tmpdir: Option<PathBuf>,

    /// GitHub context (gh_token and gh_org are in base env; kept for future direct access).
    #[allow(dead_code)]
    pub gh_token: String,
    #[allow(dead_code)]
    pub gh_org: String,
    pub repo_full_name: String,
}

impl TestEnv {
    /// Create a fresh test environment with a new tempdir as HOME.
    pub fn fresh(gh_token: &str, gh_org: &str, repo_full_name: &str) -> Self {
        let home_td = tempfile::tempdir().unwrap();
        let home = home_td.keep(); // keep() equivalent

        let env = Self::build(home.clone(), gh_token, gh_org, repo_full_name, Ownership::Fresh(home.clone()));
        env.clear(); // delete any stale snapshot
        env
    }

    /// Resume a test environment from an existing HOME directory.
    pub fn resume(home: PathBuf, gh_token: &str, gh_org: &str, repo_full_name: &str) -> Self {
        let mut env = Self::build(home, gh_token, gh_org, repo_full_name, Ownership::Resume);

        // Load exports from snapshot
        let snapshot_path = env.home.join(".test-env-exports.json");
        if let Ok(content) = fs::read_to_string(&snapshot_path) {
            if let Ok(exports) = serde_json::from_str::<HashMap<String, String>>(&content) {
                env.exports = exports;
            }
        }

        env
    }

    /// Internal constructor — sets up the full environment.
    fn build(
        home: PathBuf,
        gh_token: &str,
        gh_org: &str,
        repo_full_name: &str,
        ownership: Ownership,
    ) -> Self {
        // a. Ensure HOME exists
        fs::create_dir_all(&home).unwrap();

        // b. Capture originals (full env snapshot)
        let originals: HashMap<String, String> = std::env::vars().collect();

        // c. Bootstrap profiles into HOME
        let profiles_base = home.join(".config").join("botminter").join("profiles");
        fs::create_dir_all(&profiles_base).unwrap();
        bm::profile::extract_embedded_to_disk(&profiles_base)
            .expect("Failed to extract embedded profiles to temp dir");

        // d. Install stub ralph, build PATH
        let stub_dir = home.join("stub-bin");
        fs::create_dir_all(&stub_dir).unwrap();
        let stub_path = stub_dir.join("ralph");
        fs::copy(STUB_RALPH_SCRIPT, &stub_path).expect("failed to copy stub-ralph.sh");
        fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();

        let path = format!(
            "{}:{}",
            stub_dir.display(),
            originals.get("PATH").cloned().unwrap_or_default()
        );

        // e. Setup git auth (.gitconfig)
        fs::write(
            home.join(".gitconfig"),
            "[user]\n\
             \temail = e2e@botminter.test\n\
             \tname = BM E2E\n\
             [credential]\n\
             \thelper = !gh auth git-credential\n",
        )
        .expect("failed to write .gitconfig");

        // f. Start isolated dbus-daemon
        let dbus_tmpdir_td = tempfile::tempdir().unwrap();
        let dbus_tmpdir = dbus_tmpdir_td.keep();
        let runtime_dir = dbus_tmpdir.join("runtime");
        let data_dir = dbus_tmpdir.join("data");
        fs::create_dir_all(&runtime_dir).unwrap();
        fs::create_dir_all(&data_dir).unwrap();

        let dbus_output = Command::new("dbus-daemon")
            .args(["--session", "--fork", "--print-address", "--print-pid"])
            .env("XDG_RUNTIME_DIR", &runtime_dir)
            .output()
            .expect("failed to start dbus-daemon — is dbus installed?");
        assert!(dbus_output.status.success(), "dbus-daemon failed to start");

        let stdout = String::from_utf8_lossy(&dbus_output.stdout);
        let lines: Vec<&str> = stdout.trim().lines().collect();
        assert!(
            lines.len() >= 2,
            "dbus-daemon output missing address/pid: {:?}",
            lines
        );
        let dbus_addr = lines[0].trim().to_string();
        let dbus_pid: u32 = lines[1]
            .trim()
            .parse()
            .expect("invalid dbus-daemon PID");

        // g. Start gnome-keyring-daemon on isolated D-Bus
        Self::start_keyring_daemon_with_env(&dbus_addr, &runtime_dir, &data_dir);

        // h. Build base env
        let mut base = HashMap::new();
        base.insert("HOME".to_string(), home.to_string_lossy().to_string());
        base.insert("GH_TOKEN".to_string(), gh_token.to_string());
        base.insert("PATH".to_string(), path);
        base.insert("GIT_AUTHOR_NAME".to_string(), "BM E2E".to_string());
        base.insert("GIT_AUTHOR_EMAIL".to_string(), "e2e@botminter.test".to_string());
        base.insert("GIT_COMMITTER_NAME".to_string(), "BM E2E".to_string());
        base.insert("GIT_COMMITTER_EMAIL".to_string(), "e2e@botminter.test".to_string());
        // No DBUS/XDG vars in base — commands inherit real values from originals.
        // Isolation is handled by BM_KEYRING_DBUS include for bm only.

        // i. includes: "bm" -> { BM_KEYRING_DBUS, BM_BRIDGE_HOME }
        let real_home = originals
            .get("HOME")
            .cloned()
            .unwrap_or_else(|| "/root".to_string());
        let mut bm_includes = HashMap::new();
        bm_includes.insert("BM_KEYRING_DBUS".to_string(), dbus_addr.clone());
        bm_includes.insert("BM_BRIDGE_HOME".to_string(), real_home.clone());
        let mut includes = HashMap::new();
        includes.insert("bm".to_string(), bm_includes);

        // j. overrides: "podman" -> real HOME (needs real container storage)
        //    DBUS and XDG_RUNTIME_DIR are already real from originals.
        let mut podman_overrides = HashMap::new();
        podman_overrides.insert("HOME".to_string(), real_home);
        let mut overrides = HashMap::new();
        overrides.insert("podman".to_string(), podman_overrides);

        eprintln!(
            "  TestEnv ready (home={}, dbus pid={})",
            home.display(),
            dbus_pid
        );

        TestEnv {
            home,
            ownership,
            originals,
            base,
            includes,
            overrides,
            exports: HashMap::new(),
            dbus_pid: Some(dbus_pid),
            dbus_addr,
            dbus_tmpdir: Some(dbus_tmpdir),
            gh_token: gh_token.to_string(),
            gh_org: gh_org.to_string(),
            repo_full_name: repo_full_name.to_string(),
        }
    }

    /// Start gnome-keyring-daemon with specific env vars (no process-wide mutation).
    fn start_keyring_daemon_with_env(dbus_addr: &str, runtime_dir: &Path, data_dir: &Path) {
        let mut gkd = Command::new("gnome-keyring-daemon")
            .args([
                "--replace",
                "--unlock",
                "--components=secrets,pkcs11",
                "--daemonize",
            ])
            .env("DBUS_SESSION_BUS_ADDRESS", dbus_addr)
            .env("XDG_RUNTIME_DIR", runtime_dir)
            .env("XDG_DATA_HOME", data_dir)
            .env_remove("GNOME_KEYRING_CONTROL")
            .stdin(std::process::Stdio::piped())
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
            .expect("failed to start gnome-keyring-daemon");
        if let Some(mut stdin) = gkd.stdin.take() {
            stdin.write_all(b"\n").unwrap();
        }
        let _ = gkd.wait();
        std::thread::sleep(Duration::from_secs(1));

        let check = Command::new("busctl")
            .args([
                "--user",
                "get-property",
                "org.freedesktop.secrets",
                "/org/freedesktop/secrets/collection/login",
                "org.freedesktop.Secret.Collection",
                "Locked",
            ])
            .env("DBUS_SESSION_BUS_ADDRESS", dbus_addr)
            .env("XDG_RUNTIME_DIR", runtime_dir)
            .output();
        if let Ok(out) = check {
            let result = String::from_utf8_lossy(&out.stdout);
            assert!(
                result.trim() == "b false",
                "Keyring collection is not unlocked: {}",
                result.trim()
            );
        } else {
            panic!("Failed to verify keyring -- busctl not available?");
        }
    }

    // ── Public API ─────────────────────────────────────────────────────

    /// The test HOME directory.
    pub fn home(&self) -> &Path {
        &self.home
    }

    /// Set a cross-case export (like `export KEY=val` in a shell).
    pub fn export(&mut self, key: &str, val: &str) {
        self.exports.insert(key.to_string(), val.to_string());
    }

    /// Get a previously exported value.
    pub fn get_export(&self, key: &str) -> Option<&str> {
        self.exports.get(key).map(|s| s.as_str())
    }

    /// Remove an export.
    pub fn remove_export(&mut self, key: &str) {
        self.exports.remove(key);
    }

    /// Resolve the full environment for a given binary.
    /// Used by `command()` and by guards that need to capture env at construction.
    pub fn resolved_env(&self, binary: &str) -> HashMap<String, String> {
        // Resolve: originals -> base -> exports -> includes[bin] -> overrides[bin]
        let mut resolved = self.originals.clone();

        for (k, v) in &self.base {
            resolved.insert(k.clone(), v.clone());
        }
        for (k, v) in &self.exports {
            resolved.insert(k.clone(), v.clone());
        }

        let binary_key = Path::new(binary)
            .file_name()
            .unwrap_or(binary.as_ref())
            .to_string_lossy()
            .to_string();
        if let Some(inc) = self.includes.get(&binary_key) {
            for (k, v) in inc {
                resolved.insert(k.clone(), v.clone());
            }
        }
        if let Some(ovr) = self.overrides.get(&binary_key) {
            for (k, v) in ovr {
                resolved.insert(k.clone(), v.clone());
            }
        }

        resolved
    }

    /// Produce a TestCommand for the given binary.
    pub fn command(&self, binary: &str) -> TestCommand {
        let resolved = self.resolved_env(binary);

        // Resolve actual binary path — for crate binaries use cargo-built paths
        let actual_binary = match binary {
            "bm" => env!("CARGO_BIN_EXE_bm").to_string(),
            "bm-agent" => env!("CARGO_BIN_EXE_bm-agent").to_string(),
            _ => binary.to_string(),
        };

        let mut cmd = Command::new(&actual_binary);
        cmd.env_clear();
        cmd.envs(&resolved);

        TestCommand { cmd }
    }

    /// Save current exports to disk for progressive mode.
    pub fn save(&self) {
        let snapshot_path = self.home.join(".test-env-exports.json");
        let json = serde_json::to_string_pretty(&self.exports).unwrap();
        fs::write(&snapshot_path, json).unwrap();
    }

    /// Delete the exports snapshot. Idempotent.
    pub fn clear(&self) {
        let snapshot_path = self.home.join(".test-env-exports.json");
        let _ = fs::remove_file(&snapshot_path);
    }

    /// Reset the keyring (wipe data, restart daemon).
    pub fn reset_keyring(&self) {
        let (runtime_dir, data_dir) = self.dbus_dirs();

        // Wipe keyring data
        let keyrings_dir = data_dir.join("keyrings");
        if keyrings_dir.exists() {
            fs::remove_dir_all(&keyrings_dir).unwrap();
        }

        // Restart gnome-keyring-daemon
        Self::start_keyring_daemon_with_env(&self.dbus_addr, &runtime_dir, &data_dir);
        eprintln!("  Keyring reset (fresh data)");
    }

    /// Returns (runtime_dir, data_dir) inside the dbus tmpdir.
    fn dbus_dirs(&self) -> (PathBuf, PathBuf) {
        let tmpdir = self.dbus_tmpdir.as_ref().expect("dbus_tmpdir not set");
        (tmpdir.join("runtime"), tmpdir.join("data"))
    }

    /// Wipe and rebuild HOME while preserving specified exports.
    /// Used by the reset_home case in operator_journey.
    pub fn reset_home(&mut self) {
        // Save state that must survive
        let exports = self.exports.clone();
        let (runtime_dir, data_dir) = self.dbus_dirs();

        // Wipe and recreate HOME
        fs::remove_dir_all(&self.home).unwrap();
        fs::create_dir_all(&self.home).unwrap();

        // Reinstall stub ralph
        let stub_dir = self.home.join("stub-bin");
        fs::create_dir_all(&stub_dir).unwrap();
        let stub_path = stub_dir.join("ralph");
        fs::copy(STUB_RALPH_SCRIPT, &stub_path).expect("failed to copy stub-ralph.sh");
        fs::set_permissions(&stub_path, fs::Permissions::from_mode(0o755)).unwrap();

        // Re-bootstrap profiles
        let profiles_base = self.home.join(".config").join("botminter").join("profiles");
        fs::create_dir_all(&profiles_base).unwrap();
        bm::profile::extract_embedded_to_disk(&profiles_base)
            .expect("Failed to extract profiles");

        // Re-setup git auth
        fs::write(
            self.home.join(".gitconfig"),
            "[user]\n\
             \temail = e2e@botminter.test\n\
             \tname = BM E2E\n\
             [credential]\n\
             \thelper = !gh auth git-credential\n",
        )
        .expect("failed to write .gitconfig");

        // Restore exports
        self.exports = exports;

        // Reset keyring
        Self::start_keyring_daemon_with_env(&self.dbus_addr, &runtime_dir, &data_dir);
        eprintln!("  HOME reset, keyring restarted");
    }
}

impl Drop for TestEnv {
    fn drop(&mut self) {
        // Kill any daemon processes spawned during the test. Daemons write
        // PID files to $HOME/.botminter/daemon-*.pid — find and kill them.
        let botminter_dir = self.home.join(".botminter");
        if botminter_dir.exists() {
            if let Ok(entries) = fs::read_dir(&botminter_dir) {
                for entry in entries.flatten() {
                    let path = entry.path();
                    if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                        if name.starts_with("daemon-") && name.ends_with(".pid") {
                            if let Ok(pid_str) = fs::read_to_string(&path) {
                                if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                    eprintln!("  TestEnv: killing daemon PID {}", pid);
                                    unsafe { libc::kill(pid, libc::SIGTERM); }
                                    // Brief wait for graceful shutdown
                                    std::thread::sleep(Duration::from_millis(500));
                                    unsafe { libc::kill(pid, libc::SIGKILL); }
                                }
                            }
                            let _ = fs::remove_file(&path);
                        }
                    }
                }
            }
        }

        // Kill any stub-ralph processes left behind by sessions.
        // Stub processes write their PID to <session_workspace>/.ralph-stub-pid.
        // Session workspaces live at $HOME/.botminter/sessions/<team>/<member>/<session_id>/.
        let sessions_root = botminter_dir.join("sessions");
        if sessions_root.exists() {
            if let Ok(teams) = fs::read_dir(&sessions_root) {
                for team_entry in teams.flatten() {
                    if let Ok(members) = fs::read_dir(team_entry.path()) {
                        for member_entry in members.flatten() {
                            if let Ok(session_ids) = fs::read_dir(member_entry.path()) {
                                for session_entry in session_ids.flatten() {
                                    let pid_file = session_entry.path().join(".ralph-stub-pid");
                                    if pid_file.exists() {
                                        if let Ok(pid_str) = fs::read_to_string(&pid_file) {
                                            if let Ok(pid) = pid_str.trim().parse::<i32>() {
                                                eprintln!(
                                                    "  TestEnv: killing stub-ralph PID {}",
                                                    pid
                                                );
                                                unsafe { libc::kill(pid, libc::SIGTERM); }
                                                std::thread::sleep(Duration::from_millis(200));
                                                unsafe { libc::kill(pid, libc::SIGKILL); }
                                            }
                                        }
                                        let _ = fs::remove_file(&pid_file);
                                    }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Clean up Lima VMs tracked via exports
        if let Some(vm_name) = self.exports.get("lima_vm_name").cloned() {
            eprintln!("  TestEnv: deleting Lima VM '{}'", vm_name);
            let _ = Command::new("limactl")
                .args(["delete", "--force", &vm_name])
                .output();
        }

        // Kill isolated D-Bus daemon (gnome-keyring-daemon dies with it)
        if let Some(pid) = self.dbus_pid {
            unsafe {
                libc::kill(pid as i32, libc::SIGTERM);
            }
        }

        // Clean up dbus tmpdir
        if let Some(ref tmpdir) = self.dbus_tmpdir {
            // Try podman unshare first (for overlayfs sub-UID files)
            let path = tmpdir.display().to_string();
            let unshare = Command::new("podman")
                .args(["unshare", "rm", "-rf", &path])
                .output();
            if unshare.is_err() || tmpdir.exists() {
                let _ = fs::remove_dir_all(tmpdir);
            }
        }

        // Catch-all: kill any processes still running with the test HOME in their cmdline.
        // This handles the race where the stub writes its PID file after the sessions walk
        // runs, and any other stragglers missed by the PID-file approach above.
        if let Ownership::Fresh(ref home) = self.ownership {
            let home_str = home.to_string_lossy();
            if let Ok(proc_entries) = fs::read_dir("/proc") {
                for entry in proc_entries.flatten() {
                    let cmdline_path = entry.path().join("cmdline");
                    if let Ok(cmdline) = fs::read_to_string(&cmdline_path) {
                        if cmdline.contains(home_str.as_ref()) {
                            if let Ok(pid_name) = entry.file_name().into_string() {
                                if let Ok(pid) = pid_name.parse::<i32>() {
                                    eprintln!("  TestEnv: killing stray process PID {}", pid);
                                    unsafe { libc::kill(pid, libc::SIGKILL); }
                                }
                            }
                        }
                    }
                }
            }
        }

        // Fresh mode: delete the test HOME
        if let Ownership::Fresh(ref home) = self.ownership {
            let _ = fs::remove_dir_all(home);
        }

        eprintln!("  TestEnv cleaned up");
    }
}

// ── TestCommand ─────────────────────────────────────────────────────

/// Wraps `std::process::Command` with a controlled API.
/// The underlying Command is never exposed.
pub struct TestCommand {
    cmd: Command,
}

impl TestCommand {
    /// Add command arguments.
    pub fn args<I, S>(&mut self, args: I) -> &mut Self
    where
        I: IntoIterator<Item = S>,
        S: AsRef<std::ffi::OsStr>,
    {
        self.cmd.args(args);
        self
    }

    /// Set a one-shot env var for this command only.
    pub fn env(&mut self, key: &str, val: &str) -> &mut Self {
        self.cmd.env(key, val);
        self
    }

    /// Set the working directory for this command.
    pub fn current_dir(&mut self, dir: &Path) -> &mut Self {
        self.cmd.current_dir(dir);
        self
    }

    /// Execute, assert success, return stdout as String.
    pub fn run(&mut self) -> String {
        let output = self.cmd.output().expect("failed to run command");
        assert!(
            output.status.success(),
            "command failed with exit {}: stderr={}\nstdout={}",
            output.status.code().unwrap_or(-1),
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&output.stdout)
        );
        String::from_utf8_lossy(&output.stdout).to_string()
    }

    /// Execute, assert failure, return stderr as String.
    pub fn run_fail(&mut self) -> String {
        let output = self.cmd.output().expect("failed to run command");
        assert!(
            !output.status.success(),
            "command succeeded unexpectedly: stdout={}",
            String::from_utf8_lossy(&output.stdout)
        );
        String::from_utf8_lossy(&output.stderr).to_string()
    }

    /// Execute and return raw Output without any assertions.
    pub fn output(&mut self) -> Output {
        self.cmd.output().expect("failed to run command")
    }
}
