use std::ffi::OsString;
use std::path::PathBuf;
#[cfg(test)]
use std::path::Path;

use anyhow::{bail, Context, Result};

use crate::chat;
use crate::config;
use crate::daemon::{self, DaemonClient};
use crate::daemon::sessions_api::{StartSessionRequest, StartSessionResponse, StopSessionResponse};
use crate::profile::Meeting;

/// Trait for session lifecycle operations — injected in tests to verify start/stop calls.
pub trait SessionLifecycleTrait {
    fn start_session(&self, req: &StartSessionRequest) -> Result<StartSessionResponse>;
    fn stop_session(&self, session_id: &str, force: bool) -> Result<StopSessionResponse>;
}

/// Testable inner function — calls start_session/stop_session and returns (exit_code, workspace_path).
#[cfg(test)]
pub(crate) fn run_meeting_with_client<C: SessionLifecycleTrait>(
    _meeting: &Meeting,
    member: &str,
    _team_path: &Path,
    client: &C,
) -> Result<(i32, PathBuf)> {
    let req = StartSessionRequest {
        member_name: member.to_string(),
        session_type: "Interactive".to_string(),
        work_item_id: None,
    };
    let resp = client.start_session(&req)?;
    if !resp.ok {
        bail!(
            "Failed to start session for '{}': {}",
            member,
            resp.error.as_deref().unwrap_or("unknown error")
        );
    }
    let session_id = resp.session_id.context("daemon returned no session_id")?;
    let workspace_path = PathBuf::from(
        resp.workspace_path
            .context("daemon returned no workspace_path")?,
    );

    let _ = client.stop_session(&session_id, false);

    Ok((0, workspace_path))
}

/// Leak a String into a &'static str. Used for dynamic Clap subcommand names
/// which require 'static lifetimes. Acceptable for a CLI process that runs once.
fn leak(s: &str) -> &'static str {
    Box::leak(s.to_string().into_boxed_str())
}

// These pub functions are consumed by main.rs and completions.rs for dynamic
// subcommand injection — not cross-command coupling (ADR-0007 exception).

/// Convert a profile Meeting into a Clap subcommand for dynamic injection.
/// Accepts free-form trailing input rather than custom named/positional args.
pub fn build_meeting_subcommand(meeting: &Meeting) -> clap::Command {
    clap::Command::new(leak(&meeting.name))
        .about(leak(&meeting.description))
        .trailing_var_arg(true)
        .arg(
            clap::Arg::new("user_input")
                .num_args(0..)
                .help("Free-form input passed to the meeting"),
        )
        .arg(
            clap::Arg::new("team")
                .short('t')
                .long("team")
                .help("Team name (defaults to default team)"),
        )
        .arg(
            clap::Arg::new("autonomous")
                .short('a')
                .long("autonomous")
                .action(clap::ArgAction::SetTrue)
                .help("Run with --dangerously-skip-permissions"),
        )
}

/// Dispatch a matched meeting subcommand.
pub fn run_meeting(meeting: &Meeting, matches: &clap::ArgMatches) -> Result<()> {
    let team_flag = matches.get_one::<String>("team").map(|s| s.as_str());
    let autonomous = matches.get_flag("autonomous");

    let cfg = config::load()?;
    let team = config::resolve_team(&cfg, team_flag)?;
    let team_repo = team.path.join("team");

    let member = chat::resolve_member_by_role(&team_repo, &meeting.member)?;

    let user_input = matches
        .get_many::<String>("user_input")
        .map(|vals| vals.cloned().collect::<Vec<_>>().join(" "));

    let initial_prompt = chat::build_meeting_prompt(
        meeting.prompt.as_deref(),
        user_input.as_deref(),
    );

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

    let req = StartSessionRequest {
        member_name: member.to_string(),
        session_type: "Interactive".to_string(),
        work_item_id: None,
    };
    let resp = client.start_session(&req)?;
    if !resp.ok {
        bail!(
            "Failed to start meeting session for '{}': {}",
            member,
            resp.error.as_deref().unwrap_or("unknown error")
        );
    }
    let session_id = resp.session_id.context("daemon returned no session_id")?;
    let workspace_path_str = resp
        .workspace_path
        .context("daemon returned no workspace_path — is workspace hydration configured?")?;
    let workspace_path = PathBuf::from(&workspace_path_str);

    let session = chat::prepare_meeting_session_from_path(&workspace_path, &meeting.instructions)?;

    let exit_code = chat::launch_session(&session, team, &team_repo, &member, initial_prompt.as_deref(), autonomous)?;

    let _ = client.stop_session(&session_id, false);

    std::process::exit(exit_code);
}

/// Handle `External(Vec<OsString>)` — unknown subcommands.
pub fn run_external(args: Vec<OsString>) -> Result<()> {
    let cmd_name = args
        .first()
        .and_then(|s| s.to_str())
        .unwrap_or("<unknown>");

    bail!(
        "Unknown command '{}'.\n\
         Run `bm --help` to see available commands.",
        cmd_name
    );
}

/// Session lifecycle unit tests — verify that run_meeting_with_client wires the daemon.
#[cfg(test)]
mod session_lifecycle_tests {
    use std::cell::Cell;
    use std::path::PathBuf;

    use super::{run_meeting_with_client, SessionLifecycleTrait};
    use crate::daemon::sessions_api::{StartSessionRequest, StartSessionResponse, StopSessionResponse};
    use crate::profile::Meeting;

    struct FakeSessionClient {
        start_called: Cell<bool>,
        stop_called: Cell<bool>,
        ephemeral_ws: String,
    }

    impl FakeSessionClient {
        fn new(ephemeral_ws: &str) -> Self {
            Self {
                start_called: Cell::new(false),
                stop_called: Cell::new(false),
                ephemeral_ws: ephemeral_ws.to_string(),
            }
        }
    }

    impl SessionLifecycleTrait for FakeSessionClient {
        fn start_session(&self, _req: &StartSessionRequest) -> anyhow::Result<StartSessionResponse> {
            self.start_called.set(true);
            Ok(StartSessionResponse {
                ok: true,
                session_id: Some("fake-session-abc".to_string()),
                workspace_path: Some(self.ephemeral_ws.clone()),
                error: None,
            })
        }

        fn stop_session(&self, _session_id: &str, _force: bool) -> anyhow::Result<StopSessionResponse> {
            self.stop_called.set(true);
            Ok(StopSessionResponse { ok: true, error: None })
        }
    }

    fn test_meeting() -> Meeting {
        Meeting {
            name: "planning".into(),
            description: "Planning meeting".into(),
            member: "engineer".into(),
            instructions: "You are an engineer in a planning meeting.\n".into(),
            prompt: None,
        }
    }

    #[test]
    fn meeting_calls_start_session() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = FakeSessionClient::new("/ephemeral/sessions/fake-abc");
        let meeting = test_meeting();

        run_meeting_with_client(&meeting, "engineer-carol", tmp.path(), &fake).unwrap();

        assert!(
            fake.start_called.get(),
            "run_meeting must call start_session on the daemon to create an ephemeral session"
        );
    }

    #[test]
    fn meeting_uses_ephemeral_workspace_from_daemon() {
        let tmp = tempfile::tempdir().unwrap();
        let ephemeral_ws = "/ephemeral/sessions/fake-abc";
        let fake = FakeSessionClient::new(ephemeral_ws);
        let meeting = test_meeting();

        let (_exit_code, ws_path) =
            run_meeting_with_client(&meeting, "engineer-carol", tmp.path(), &fake).unwrap();

        assert_eq!(
            ws_path,
            PathBuf::from(ephemeral_ws),
            "workspace path must be the ephemeral path from daemon start_session, \
             not the permanent workspace at team_path/member"
        );
    }

    #[test]
    fn meeting_stop_session_called_on_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let fake = FakeSessionClient::new("/ephemeral/sessions/fake-abc");
        let meeting = test_meeting();

        run_meeting_with_client(&meeting, "engineer-carol", tmp.path(), &fake).unwrap();

        assert!(
            fake.stop_called.get(),
            "stop_session must be called when the meeting exits to trigger finalization"
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::profile::Meeting;

    fn planning_meeting() -> Meeting {
        Meeting {
            name: "planning".into(),
            description: "Collaborative planning session".into(),
            member: "engineer".into(),
            instructions: "You are an engineer in a planning meeting.\n".into(),
            prompt: Some("start".into()),
        }
    }

    fn verification_meeting() -> Meeting {
        Meeting {
            name: "verification".into(),
            description: "Verify acceptance criteria for completed work".into(),
            member: "engineer".into(),
            instructions: "You are an engineer in a verification meeting.\n".into(),
            prompt: None,
        }
    }

    #[test]
    fn build_meeting_subcommand_creates_valid_clap_command() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        assert_eq!(cmd.get_name(), "planning");
    }

    #[test]
    fn build_meeting_subcommand_has_user_input_and_team_args() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let arg_names: Vec<&str> = cmd.get_arguments().map(|a| a.get_id().as_str()).collect();
        assert!(arg_names.contains(&"user_input"));
        assert!(arg_names.contains(&"team"));
        assert!(arg_names.contains(&"autonomous"));
    }

    #[test]
    fn trailing_args_parsed_as_user_input() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let matches = cmd
            .try_get_matches_from(vec!["planning", "plan", "the", "auth", "feature"])
            .unwrap();
        let input: Vec<String> = matches
            .get_many::<String>("user_input")
            .unwrap()
            .cloned()
            .collect();
        assert_eq!(input, vec!["plan", "the", "auth", "feature"]);
    }

    #[test]
    fn no_trailing_args_returns_none() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let matches = cmd.try_get_matches_from(vec!["planning"]).unwrap();
        let input = matches.get_many::<String>("user_input");
        assert!(input.is_none());
    }

    #[test]
    fn team_and_autonomous_flags_work() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let matches = cmd
            .try_get_matches_from(vec!["planning", "-t", "my-team", "-a"])
            .unwrap();
        assert_eq!(
            matches.get_one::<String>("team").map(|s| s.as_str()),
            Some("my-team")
        );
        assert!(matches.get_flag("autonomous"));
    }

    #[test]
    fn trailing_args_with_team_flag() {
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let matches = cmd
            .try_get_matches_from(vec!["planning", "-t", "my-team", "plan", "something"])
            .unwrap();
        let input: Vec<String> = matches
            .get_many::<String>("user_input")
            .unwrap()
            .cloned()
            .collect();
        assert_eq!(input, vec!["plan", "something"]);
        assert_eq!(
            matches.get_one::<String>("team").map(|s| s.as_str()),
            Some("my-team")
        );
    }

    #[test]
    fn flags_after_positional_args_are_captured_as_input() {
        // trailing_var_arg(true) means flags after the first positional value
        // are NOT parsed — they become part of user_input. This is by design:
        // users must place -t and -a BEFORE free-form text.
        let m = planning_meeting();
        let cmd = build_meeting_subcommand(&m);
        let matches = cmd
            .try_get_matches_from(vec!["planning", "some", "text", "-t", "my-team"])
            .unwrap();
        let input: Vec<String> = matches
            .get_many::<String>("user_input")
            .unwrap()
            .cloned()
            .collect();
        assert_eq!(input, vec!["some", "text", "-t", "my-team"]);
        assert!(matches.get_one::<String>("team").is_none());
    }

    #[test]
    fn prompt_combined_with_user_input() {
        let m = planning_meeting();
        let prompt = chat::build_meeting_prompt(
            m.prompt.as_deref(),
            Some("plan the auth feature"),
        );
        assert_eq!(prompt, Some("start plan the auth feature".into()));
    }

    #[test]
    fn prompt_alone_when_no_user_input() {
        let m = planning_meeting();
        let prompt = chat::build_meeting_prompt(m.prompt.as_deref(), None);
        assert_eq!(prompt, Some("start".into()));
    }

    #[test]
    fn user_input_alone_when_no_prompt() {
        let m = verification_meeting();
        let prompt = chat::build_meeting_prompt(m.prompt.as_deref(), Some("plan something"));
        assert_eq!(prompt, Some("plan something".into()));
    }

    #[test]
    fn none_when_neither_prompt_nor_input() {
        let m = verification_meeting();
        let prompt = chat::build_meeting_prompt(m.prompt.as_deref(), None);
        assert!(prompt.is_none());
    }
}
