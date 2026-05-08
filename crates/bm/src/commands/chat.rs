use anyhow::Result;

use crate::chat;
use crate::config;
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

    let session = chat::prepare_chat_session(&team_repo, &team.name, &team.path, member, hat)?;

    if render_system_prompt {
        println!("{}", session.meta_prompt);
        return Ok(());
    }

    chat::launch_session(&session, team, &team_repo, None, autonomous)
}
