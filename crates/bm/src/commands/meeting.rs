use std::ffi::OsString;

use anyhow::{bail, Result};

use crate::chat;
use crate::config;
use crate::profile::Meeting;

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
    let session = chat::prepare_meeting_session(
        &team.path,
        &member,
        &meeting.instructions,
    )?;

    chat::launch_session(&session, team, &team_repo, initial_prompt.as_deref(), autonomous)
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
