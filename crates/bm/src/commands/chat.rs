use std::io::Write;

use anyhow::{bail, Context, Result};

use crate::chat;
use crate::config;
use crate::formation;
use crate::profile;
use crate::state;

/// Handles `bm chat <member> [-t team] [--hat <hat>] [--render-system-prompt] [-a]`.
pub fn run(
    member: &str,
    team_flag: Option<&str>,
    hat: Option<&str>,
    render_system_prompt: bool,
    autonomous: bool,
) -> Result<()> {
    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    // Note if brain mode is active — chat runs independently alongside it
    let runtime_state = state::load()?;
    let state_key = format!("{}/{}", team.name, member);
    if let Some(rt) = runtime_state.members.get(&state_key) {
        if rt.brain_mode && state::is_alive(rt.pid) {
            eprintln!(
                "Note: member '{}' is also running in brain mode (PID {}). Starting independent chat session.",
                member, rt.pid
            );
        }
    }

    // Prepare session (validates member, workspace, hat, builds meta-prompt)
    let session = chat::prepare_chat_session(
        &team_repo,
        &team.name,
        &team.path,
        member,
        hat,
    )?;

    if render_system_prompt {
        println!("{}", session.meta_prompt);
        return Ok(());
    }

    // Resolve coding agent for launch
    let manifest = profile::read_team_repo_manifest(&team_repo)?;
    let coding_agent = profile::resolve_coding_agent(team, &manifest)?;

    // Write meta-prompt to temp file
    let mut tmp_file = tempfile::Builder::new()
        .prefix("bm-chat-")
        .suffix(".md")
        .tempfile()
        .context("Failed to create temp file for meta-prompt")?;
    tmp_file
        .write_all(session.meta_prompt.as_bytes())
        .context("Failed to write meta-prompt to temp file")?;
    let tmp_path = tmp_file.into_temp_path();

    // Build command arguments for the coding agent
    let tmp_path_str = tmp_path.to_str().context("Temp path is not valid UTF-8")?;
    let prompt_flag = coding_agent.system_prompt_flag.as_deref().with_context(|| {
        format!(
            "Coding agent '{}' ({}) does not define a system_prompt_flag",
            coding_agent.display_name, coding_agent.binary
        )
    })?;
    let mut args: Vec<&str> = vec![prompt_flag, tmp_path_str];
    if autonomous {
        if let Some(flag) = coding_agent.skip_permissions_flag.as_deref() {
            args.push(flag);
        }
    }

    // Resolve formation: v2 teams delegate through formation.exec_in(),
    // v1 teams (no formations dir) use direct process exec for backward compat.
    let resolved_formation = formation::resolve_formation(&team_repo, None)?;

    if resolved_formation.is_some() {
        // v2 team — delegate to formation.exec_in()
        let local_formation = formation::create_local_formation(&team.name)?;
        let mut cmd_parts: Vec<&str> = vec![&coding_agent.binary];
        cmd_parts.extend(&args);
        local_formation.exec_in(&session.ws_path, &cmd_parts)?;
        Ok(())
    } else {
        // v1 team (no formations dir) — legacy path: exec() replaces this process
        use std::os::unix::process::CommandExt;
        let mut cmd = std::process::Command::new(&coding_agent.binary);
        cmd.current_dir(&session.ws_path)
            .arg(prompt_flag)
            .arg(&tmp_path);
        if autonomous {
            if let Some(flag) = coding_agent.skip_permissions_flag.as_deref() {
                cmd.arg(flag);
            }
        }
        let err = cmd.exec();
        bail!("Failed to launch {}: {}", coding_agent.binary, err);
    }
}
