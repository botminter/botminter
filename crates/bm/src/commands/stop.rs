use std::process::Command;
use std::thread;
use std::time::Duration;

use anyhow::{bail, Result};

use crate::config;
use crate::state;
use crate::topology;

/// Maximum seconds to wait for graceful stop before giving up per member.
const GRACEFUL_TIMEOUT_SECS: u64 = 60;

/// Handles `bm stop [-t team] [--force]`.
pub fn run(team_flag: Option<&str>, force: bool) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_name = &team.name;

    let mut runtime_state = state::load()?;

    // Find running members for this team
    let team_prefix = format!("{}/", team_name);
    let running: Vec<(String, u32, std::path::PathBuf)> = runtime_state
        .members
        .iter()
        .filter(|(key, _)| key.starts_with(&team_prefix))
        .map(|(key, rt)| (key.clone(), rt.pid, rt.workspace.clone()))
        .collect();

    if running.is_empty() {
        println!("No members running for team '{}'.", team_name);
        return Ok(());
    }

    let mut stopped = 0u32;
    let mut errors = 0u32;

    for (key, pid, workspace) in &running {
        let member_name = key.strip_prefix(&team_prefix).unwrap_or(key);

        if !state::is_alive(*pid) {
            eprint!("{}... already exited", member_name);
            eprintln!();
            runtime_state.members.remove(key);
            state::save(&runtime_state)?;
            stopped += 1;
            continue;
        }

        if force {
            eprint!("Stopping {} (force)... ", member_name);
            force_stop(*pid);
            runtime_state.members.remove(key);
            state::save(&runtime_state)?;
            eprintln!("done");
            stopped += 1;
        } else {
            eprint!("Stopping {}... ", member_name);
            match graceful_stop(workspace, *pid) {
                Ok(()) => {
                    runtime_state.members.remove(key);
                    state::save(&runtime_state)?;
                    eprintln!("done");
                    stopped += 1;
                }
                Err(e) => {
                    eprintln!("failed: {}", e);
                    eprintln!(
                        "  Hint: try `bm stop -f` to force-kill, or check workspace at {}",
                        workspace.display()
                    );
                    errors += 1;
                }
            }
        }
    }

    println!(
        "\nStopped {} member(s), {} error(s).",
        stopped, errors
    );

    if errors > 0 {
        bail!(
            "Some members could not be stopped gracefully. \
             Use `bm stop -f` to force-kill."
        );
    }

    // Remove topology file after all members stopped
    let topo_path = topology::topology_path(&cfg.workzone, team_name);
    if topo_path.exists() {
        topology::remove(&topo_path)?;
    }

    Ok(())
}

/// Graceful stop: run `ralph loops stop` in the workspace, then poll for exit.
fn graceful_stop(workspace: &std::path::Path, pid: u32) -> Result<()> {
    // Try ralph loops stop
    let output = Command::new("ralph")
        .args(["loops", "stop"])
        .current_dir(workspace)
        .output();

    match output {
        Ok(o) if !o.status.success() => {
            let stderr = String::from_utf8_lossy(&o.stderr);
            bail!("ralph loops stop failed: {}", stderr.trim());
        }
        Err(e) => {
            bail!("Failed to run ralph loops stop: {}", e);
        }
        Ok(_) => {}
    }

    // Poll for process exit
    for _ in 0..GRACEFUL_TIMEOUT_SECS {
        if !state::is_alive(pid) {
            return Ok(());
        }
        thread::sleep(Duration::from_secs(1));
    }

    bail!(
        "Process {} did not exit after {}s. Use `bm stop -f` to force-kill.",
        pid,
        GRACEFUL_TIMEOUT_SECS
    );
}

/// Force stop: send SIGTERM to the process.
fn force_stop(pid: u32) {
    unsafe {
        libc::kill(pid as i32, libc::SIGTERM);
    }
    // Brief wait for cleanup
    thread::sleep(Duration::from_millis(500));
}
