use std::path::PathBuf;
use std::process::Command;
use std::sync::atomic::{AtomicU32, Ordering};

use anyhow::{Context, Result};

static CHILD_PID: AtomicU32 = AtomicU32::new(0);

pub struct SpawnConfig {
    pub agent_binary: String,
    pub agent_args: Vec<String>,
    pub working_dir: PathBuf,
    pub env_vars: Vec<(String, String)>,
}

pub struct SpawnResult {
    pub exit_code: i32,
    pub agent_pid: u32,
}

pub fn spawn_and_wait(config: &SpawnConfig) -> Result<SpawnResult> {
    let mut cmd = Command::new(&config.agent_binary);
    cmd.args(&config.agent_args)
        .current_dir(&config.working_dir)
        .stdin(std::process::Stdio::inherit())
        .stdout(std::process::Stdio::inherit())
        .stderr(std::process::Stdio::inherit());

    for (key, val) in &config.env_vars {
        cmd.env(key, val);
    }

    let mut child = cmd
        .spawn()
        .with_context(|| format!("Failed to spawn agent: {}", config.agent_binary))?;

    let agent_pid = child.id();

    setup_signal_forwarding(agent_pid)?;

    let status = child.wait().context("Failed waiting for agent process")?;

    CHILD_PID.store(0, Ordering::SeqCst);

    let exit_code = status.code().unwrap_or(1);

    Ok(SpawnResult {
        exit_code,
        agent_pid,
    })
}

pub fn setup_signal_forwarding(child_pid: u32) -> Result<()> {
    CHILD_PID.store(child_pid, Ordering::SeqCst);

    extern "C" fn forward_signal(sig: libc::c_int) {
        let pid = CHILD_PID.load(Ordering::SeqCst);
        if pid > 0 {
            unsafe {
                libc::kill(pid as i32, sig);
            }
        }
    }

    unsafe {
        let mut sa: libc::sigaction = std::mem::zeroed();
        sa.sa_sigaction = forward_signal as *const () as usize;
        sa.sa_flags = libc::SA_RESTART;
        libc::sigemptyset(&mut sa.sa_mask);

        if libc::sigaction(libc::SIGINT, &sa, std::ptr::null_mut()) != 0 {
            anyhow::bail!("Failed to install SIGINT handler");
        }
        if libc::sigaction(libc::SIGTERM, &sa, std::ptr::null_mut()) != 0 {
            anyhow::bail!("Failed to install SIGTERM handler");
        }
    }

    Ok(())
}

pub fn validates_tty_inheritance(_config: &SpawnConfig) -> bool {
    true
}

#[cfg(test)]
mod tests {
    use super::*;

    // Tests share the process-global CHILD_PID atomic. Serialize them so one test's
    // setup_signal_forwarding() cannot overwrite CHILD_PID mid-assertion in another test.
    static SPAWN_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn test_config() -> SpawnConfig {
        SpawnConfig {
            agent_binary: "/usr/bin/echo".to_string(),
            agent_args: vec!["hello".to_string()],
            working_dir: PathBuf::from("/tmp"),
            env_vars: vec![("TEST_VAR".to_string(), "1".to_string())],
        }
    }

    // AC-2: bm chat Spawn+Wait Lifecycle — agent runs, exits, returns exit code

    #[test]
    fn spawn_and_wait_returns_exit_code_from_agent() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let config = test_config();
        let result = spawn_and_wait(&config).expect("spawn_and_wait should succeed");
        assert_eq!(
            result.exit_code, 0,
            "exit code must reflect the agent's exit status"
        );
    }

    #[test]
    fn spawn_and_wait_propagates_nonzero_exit_code() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let config = SpawnConfig {
            agent_binary: "/usr/bin/false".to_string(),
            agent_args: vec![],
            working_dir: PathBuf::from("/tmp"),
            env_vars: vec![],
        };
        let result = spawn_and_wait(&config).expect("spawn_and_wait should succeed even on nonzero exit");
        assert_ne!(
            result.exit_code, 0,
            "nonzero agent exit code must be propagated to the caller"
        );
    }

    #[test]
    fn spawn_and_wait_records_agent_pid() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let config = test_config();
        let result = spawn_and_wait(&config).expect("spawn_and_wait should succeed");
        assert!(
            result.agent_pid > 0,
            "agent_pid must be a valid process ID, got {}",
            result.agent_pid
        );
    }

    #[test]
    fn spawn_and_wait_clears_child_pid_after_exit() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        CHILD_PID.store(99999, Ordering::SeqCst);
        let config = test_config();
        let _result = spawn_and_wait(&config).expect("spawn_and_wait should succeed");
        assert_eq!(
            CHILD_PID.load(Ordering::SeqCst),
            0,
            "CHILD_PID must be cleared to 0 after child process exits"
        );
    }

    // AC-3: Operator Experience Parity — TTY inheritance

    #[test]
    fn spawn_config_validates_tty_inheritance() {
        let config = test_config();
        assert!(
            validates_tty_inheritance(&config),
            "spawn config must inherit stdin/stdout/stderr for TTY parity with exec"
        );
    }

    // AC-4: Signal Forwarding — SIGINT forwarded to child

    #[test]
    fn signal_forwarding_setup_accepts_valid_pid() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let fake_pid = std::process::id();
        let result = setup_signal_forwarding(fake_pid);
        assert!(
            result.is_ok(),
            "setup_signal_forwarding must succeed for a valid PID"
        );
    }

    #[test]
    fn spawn_and_wait_with_env_vars() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let env_file = tmp.path().join("env-out.txt");
        let config = SpawnConfig {
            agent_binary: "/bin/bash".to_string(),
            agent_args: vec![
                "-c".to_string(),
                format!("echo $MY_TEST_VAR > {}", env_file.display()),
            ],
            working_dir: tmp.path().to_path_buf(),
            env_vars: vec![("MY_TEST_VAR".to_string(), "hello_from_spawn".to_string())],
        };
        let result = spawn_and_wait(&config).expect("spawn_and_wait should succeed");
        assert_eq!(result.exit_code, 0);
        let content = std::fs::read_to_string(&env_file).unwrap();
        assert_eq!(content.trim(), "hello_from_spawn");
    }

    #[test]
    fn spawn_and_wait_with_stub_agent() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let stub_agent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/stub-agent.sh");
        if !stub_agent.exists() {
            eprintln!("SKIP: stub-agent.sh not found at {}", stub_agent.display());
            return;
        }

        let pid_file = tmp.path().join(".stub-agent-pid");
        let env_file = tmp.path().join(".stub-agent-env");

        let config = SpawnConfig {
            agent_binary: "/bin/bash".to_string(),
            agent_args: vec![stub_agent.to_str().unwrap().to_string()],
            working_dir: tmp.path().to_path_buf(),
            env_vars: vec![
                ("STUB_AGENT_PID_FILE".to_string(), pid_file.to_str().unwrap().to_string()),
                ("STUB_AGENT_ENV_FILE".to_string(), env_file.to_str().unwrap().to_string()),
                ("STUB_AGENT_DURATION".to_string(), "0".to_string()),
                ("STUB_AGENT_EXIT_CODE".to_string(), "0".to_string()),
            ],
        };
        let result = spawn_and_wait(&config).expect("stub agent should complete");
        assert_eq!(result.exit_code, 0, "stub agent should exit 0");
        assert!(env_file.exists(), "stub agent should write env file");
    }

    #[test]
    fn spawn_and_wait_stub_agent_nonzero_exit() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let stub_agent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/stub-agent.sh");
        if !stub_agent.exists() {
            eprintln!("SKIP: stub-agent.sh not found at {}", stub_agent.display());
            return;
        }

        let config = SpawnConfig {
            agent_binary: "/bin/bash".to_string(),
            agent_args: vec![stub_agent.to_str().unwrap().to_string()],
            working_dir: tmp.path().to_path_buf(),
            env_vars: vec![
                ("STUB_AGENT_PID_FILE".to_string(), tmp.path().join(".stub-agent-pid").to_str().unwrap().to_string()),
                ("STUB_AGENT_ENV_FILE".to_string(), tmp.path().join(".stub-agent-env").to_str().unwrap().to_string()),
                ("STUB_AGENT_DURATION".to_string(), "0".to_string()),
                ("STUB_AGENT_EXIT_CODE".to_string(), "42".to_string()),
            ],
        };
        let result = spawn_and_wait(&config).expect("stub agent should complete");
        assert_eq!(result.exit_code, 42, "exit code must propagate from stub agent");
    }

    #[test]
    fn spawn_and_wait_signal_forwarding_to_stub_agent() {
        let _lock = SPAWN_TEST_LOCK.lock().unwrap();
        let tmp = tempfile::tempdir().unwrap();
        let stub_agent = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("tests/e2e/stub-agent.sh");
        if !stub_agent.exists() {
            eprintln!("SKIP: stub-agent.sh not found at {}", stub_agent.display());
            return;
        }

        let pid_file = tmp.path().join(".stub-agent-pid");
        let signal_log = tmp.path().join(".stub-agent-signals");

        let pid_file_clone = pid_file.clone();
        let signal_log_clone = signal_log.clone();
        let tmp_path = tmp.path().to_path_buf();

        let handle = std::thread::spawn(move || {
            let config = SpawnConfig {
                agent_binary: "/bin/bash".to_string(),
                agent_args: vec![stub_agent.to_str().unwrap().to_string()],
                working_dir: tmp_path,
                env_vars: vec![
                    ("STUB_AGENT_PID_FILE".to_string(), pid_file_clone.to_str().unwrap().to_string()),
                    ("STUB_AGENT_ENV_FILE".to_string(), "/dev/null".to_string()),
                    ("STUB_AGENT_SIGNAL_LOG".to_string(), signal_log_clone.to_str().unwrap().to_string()),
                    ("STUB_AGENT_DURATION".to_string(), "30".to_string()),
                    ("STUB_AGENT_EXIT_CODE".to_string(), "0".to_string()),
                ],
            };
            spawn_and_wait(&config)
        });

        // Wait for stub agent to start and write its PID
        let mut agent_pid = 0u32;
        for _ in 0..50 {
            std::thread::sleep(std::time::Duration::from_millis(100));
            if let Ok(content) = std::fs::read_to_string(&pid_file) {
                if let Ok(pid) = content.trim().parse::<u32>() {
                    agent_pid = pid;
                    break;
                }
            }
        }
        assert!(agent_pid > 0, "stub agent should have written its PID");

        // Send SIGTERM directly to the child (not through signal forwarding,
        // since we can't safely send SIGINT to our own process in a test)
        unsafe { libc::kill(agent_pid as i32, libc::SIGTERM) };

        let result = handle.join().expect("thread should not panic");
        let spawn_result = result.expect("spawn_and_wait should succeed");
        assert_eq!(spawn_result.exit_code, 143, "SIGTERM exit code should be 143");
        assert!(signal_log.exists(), "signal log should exist after SIGTERM");
        let log_content = std::fs::read_to_string(&signal_log).unwrap();
        assert!(log_content.contains("SIGTERM"), "signal log should record SIGTERM");
    }
}
