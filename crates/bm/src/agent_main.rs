use std::path::Path;
use std::process;

use anyhow::Context as _;
use clap::Parser;

use bm::agent_cli::{AgentCli, AgentCommand, ClaudeCommand, ClaudeHookCommand, InboxCommand, InboxFormat, LockCommand, LoopCommand};
use bm::daemon::sessions_api::{AcquireLockResponse, ReleaseLockResponse};
use bm::brain::inbox;
use bm::daemon::DaemonClient;
use bm::daemon::sessions_api::StartSessionRequest;

fn main() {
    let cli = AgentCli::parse();

    let result = match cli.command {
        AgentCommand::Inbox { command } => run_inbox(command),
        AgentCommand::Claude { command } => run_claude(command),
        AgentCommand::Loop { command } => run_loop(command),
        AgentCommand::Lock { command } => run_lock(command),
    };

    if let Err(e) = result {
        eprintln!("Error: {e}");
        process::exit(1);
    }
}

fn run_inbox(command: InboxCommand) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let root = inbox::discover_workspace_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Not in a BotMinter workspace (no .botminter.workspace found)"))?;
    let path = inbox::inbox_path(&root);

    match command {
        InboxCommand::Write { message, from } => {
            inbox::write_message(&path, &from, &message)?;
            eprintln!("Message written to inbox.");
        }
        InboxCommand::Read { format } => {
            let result = inbox::read_messages(&path, true)?;
            match format {
                InboxFormat::Json => {
                    let json = serde_json::to_string(&result.messages)?;
                    println!("{json}");
                }
                InboxFormat::Hook => {
                    if let Some(response) = inbox::format_hook_response(&result.messages) {
                        println!("{response}");
                    }
                }
            }
        }
        InboxCommand::Peek => {
            let result = inbox::read_messages(&path, false)?;
            if result.messages.is_empty() {
                println!("No pending messages.");
            } else {
                for msg in &result.messages {
                    println!("[{}] ({}): {}", msg.ts, msg.from, msg.message);
                }
            }
        }
    }

    Ok(())
}

fn connect_daemon() -> anyhow::Result<DaemonClient> {
    let team_name = std::env::var("BM_TEAM_NAME")
        .map_err(|_| anyhow::anyhow!("BM_TEAM_NAME not set. This command must be run from a BotMinter member workspace."))?;
    DaemonClient::connect(&team_name)
}

fn run_loop(command: LoopCommand) -> anyhow::Result<()> {
    match command {
        LoopCommand::Start { prompt: _, member } => {
            let client = connect_daemon()?;
            let member_name = member.ok_or_else(|| {
                anyhow::anyhow!("--member is required in the ephemeral sessions model")
            })?;
            let req = StartSessionRequest {
                member_name,
                session_type: "Loop".to_string(),
                work_item_id: None,
            };
            let resp = client.start_session(&req)?;

            if resp.ok {
                let session_id = resp.session_id.as_deref().unwrap_or("unknown");
                eprintln!("Loop session started (ID {})", session_id);
                println!("{}", session_id);
            } else {
                let err = resp.error.unwrap_or_else(|| "unknown error".to_string());
                anyhow::bail!("Failed to start loop session: {}", err);
            }

            Ok(())
        }
    }
}

fn run_claude(command: ClaudeCommand) -> anyhow::Result<()> {
    match command {
        ClaudeCommand::Hook { command } => run_claude_hook(command),
    }
}

fn run_claude_hook(command: ClaudeHookCommand) -> anyhow::Result<()> {
    match command {
        ClaudeHookCommand::PostToolUse => {
            // This command NEVER fails — always exits 0.
            // Errors are silently swallowed.
            let _ = try_post_tool_use();
            Ok(())
        }
    }
}

/// Nudge injected after every tool use via the PostToolUse hook.
///
/// Reminds the LLM to check whether the user is waiting for a response.
/// Without this, the brain tends to run background tools and then keep
/// making more tool calls without ever sending a text response to the
/// chat, leaving the user waiting indefinitely.
const POST_TOOL_NUDGE: &str =
    "If the user is waiting for a response, respond to them now.";

fn try_post_tool_use() -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let root = match inbox::discover_workspace_root(&cwd) {
        Some(r) => r,
        None => return Ok(()),
    };
    let path = inbox::inbox_path(&root);
    let result = inbox::read_messages(&path, true)?;

    if let Some(response) = inbox::format_hook_response(&result.messages) {
        // Inbox has messages — they take priority
        println!("{response}");
    } else {
        // No inbox messages — inject the response nudge
        let json = serde_json::json!({
            "additionalContext": POST_TOOL_NUDGE
        });
        println!("{json}");
    }
    Ok(())
}

fn run_lock(command: LockCommand) -> anyhow::Result<()> {
    let cwd = std::env::current_dir()?;
    let root = inbox::discover_workspace_root(&cwd)
        .ok_or_else(|| anyhow::anyhow!("Not in a BotMinter workspace (no .botminter.workspace found)"))?;
    let session_id = read_session_id_from_marker(&root)?;
    let client = connect_daemon()?;
    match command {
        LockCommand::Acquire { work_item_id } => {
            let resp = client.acquire_lock(&session_id, &work_item_id)?;
            let code = lock_acquire_exit_code(&resp);
            if code != 0 {
                if let Some(holder) = &resp.holder {
                    eprintln!("Work item {} held by session {}", work_item_id, holder);
                }
                process::exit(code);
            }
        }
        LockCommand::Release { work_item_id } => {
            let resp = client.release_lock(&session_id, &work_item_id)?;
            let code = lock_release_exit_code(&resp);
            if code != 0 {
                process::exit(code);
            }
        }
    }
    Ok(())
}

fn read_session_id_from_marker(workspace_root: &Path) -> anyhow::Result<String> {
    let marker_path = workspace_root.join(".botminter.workspace");
    let content = std::fs::read_to_string(&marker_path)
        .with_context(|| format!("Failed to read {}", marker_path.display()))?;
    for line in content.lines() {
        if let Some(value) = line.strip_prefix("session_id:") {
            let id = value.trim().to_string();
            if !id.is_empty() {
                return Ok(id);
            }
        }
    }
    anyhow::bail!("No session_id in .botminter.workspace — is this session managed by the daemon?")
}

/// Maps an acquire response to a CLI exit code.
/// 0 = acquired, 1 = contended, 2+ = error.
fn lock_acquire_exit_code(response: &AcquireLockResponse) -> i32 {
    if response.acquired { 0 } else { 1 }
}

/// Maps a release response to a CLI exit code.
/// 0 = released.
fn lock_release_exit_code(_response: &ReleaseLockResponse) -> i32 {
    0
}

#[cfg(test)]
mod tests {
    use super::*;

    // CT-154-04 AC-5: bm-agent lock acquire exit code contract

    #[test]
    fn lock_acquire_exit_code_is_0_on_success() {
        let resp = AcquireLockResponse { acquired: true, holder: None };
        assert_eq!(lock_acquire_exit_code(&resp), 0, "exit 0 when lock is acquired");
    }

    #[test]
    fn lock_acquire_exit_code_is_1_on_contention() {
        let resp = AcquireLockResponse {
            acquired: false,
            holder: Some("other-session".to_string()),
        };
        assert_eq!(lock_acquire_exit_code(&resp), 1, "exit 1 when lock is held by another session");
    }

    // CT-154-04 AC-6: bm-agent lock release exit code contract

    #[test]
    fn lock_release_exit_code_is_0_on_success() {
        let resp = ReleaseLockResponse { released: true };
        assert_eq!(lock_release_exit_code(&resp), 0, "exit 0 when lock is released");
    }
}
