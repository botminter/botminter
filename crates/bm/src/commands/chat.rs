use anyhow::{bail, Context, Result};

use crate::chat;
use crate::config;
use crate::daemon::{self, DaemonClient};
use crate::daemon::sessions_api::StartSessionRequest;

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

    // Ensure daemon is running, auto-starting if needed
    let client = match DaemonClient::connect(&team.name) {
        Ok(c) => c,
        Err(_) => {
            eprintln!("Starting daemon for team '{}'...", team.name);
            let mode = if team.daemon.polling { "poll" } else { "webhook" };
            daemon::start_daemon(
                &team.name,
                &team_repo,
                mode,
                0,
                team.daemon.interval,
                "127.0.0.1",
                "info",
            )?;
            DaemonClient::connect(&team.name)?
        }
    };

    // Start an Interactive session via the daemon — this hydrates the workspace
    let req = StartSessionRequest {
        member_name: member.to_string(),
        session_type: "Interactive".to_string(),
        work_item_id: None,
    };
    let resp = client.start_session(&req)?;
    if !resp.ok {
        bail!(
            "Failed to start interactive session for '{}': {}",
            member,
            resp.error.as_deref().unwrap_or("unknown error")
        );
    }

    let session_id = resp.session_id.context("daemon returned no session_id")?;
    let workspace_path_str = resp
        .workspace_path
        .context("daemon returned no workspace_path — is workspace hydration configured?")?;
    let workspace_path = std::path::PathBuf::from(&workspace_path_str);

    let session =
        chat::prepare_chat_session_from_path(&team_repo, &team.name, member, &workspace_path, hat)?;

    if render_system_prompt {
        // Deactivate the session immediately — user only wanted the prompt text
        let _ = client.stop_session(&session_id, false);
        println!("{}", session.meta_prompt);
        return Ok(());
    }

    let exit_code = chat::launch_session(&session, team, &team_repo, member, None, autonomous)?;

    // Deactivate the session now that the user has exited the coding agent
    let _ = client.stop_session(&session_id, false);

    std::process::exit(exit_code);
}
