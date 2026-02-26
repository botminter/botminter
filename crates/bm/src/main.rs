use anyhow::Result;
use clap::Parser;
use clap_complete::CompleteEnv;

use bm::cli::{
    Cli, Command, DaemonCommand, KnowledgeCommand, MembersCommand, ProfilesCommand,
    ProjectsCommand, RolesCommand, TeamsCommand,
};
use bm::commands;
use bm::completions;

fn main() -> Result<()> {
    CompleteEnv::with_factory(completions::build_cli_with_completions).complete();

    let cli = Cli::parse();

    match cli.command {
        Command::Init => commands::init::run()?,

        Command::Profiles { command } => match command {
            ProfilesCommand::List => commands::profiles::list()?,
            ProfilesCommand::Describe { profile } => commands::profiles::describe(&profile)?,
        },

        Command::Teams { command } => match command {
            TeamsCommand::List => commands::teams::list()?,
            TeamsCommand::Show { name, team } => {
                commands::teams::show(name.as_deref(), team.as_deref())?;
            }
            TeamsCommand::Sync { push, team } => {
                commands::teams::sync(push, team.as_deref())?;
            }
        },

        Command::Hire { role, name, team } => {
            commands::hire::run(&role, name.as_deref(), team.as_deref())?;
        }

        Command::Members { command } => match command {
            MembersCommand::List { team } => {
                commands::members::list(team.as_deref())?;
            }
            MembersCommand::Show { member, team } => {
                commands::members::show(&member, team.as_deref())?;
            }
        },

        Command::Roles { command } => match command {
            RolesCommand::List { team } => {
                commands::roles::list(team.as_deref())?;
            }
        },

        Command::Projects { command } => match command {
            ProjectsCommand::List { team } => {
                commands::projects::list(team.as_deref())?;
            }
            ProjectsCommand::Show { project, team } => {
                commands::projects::show(&project, team.as_deref())?;
            }
            ProjectsCommand::Add { url, team } => {
                commands::projects::add(&url, team.as_deref())?;
            }
            ProjectsCommand::Sync { team } => {
                commands::projects::sync(team.as_deref())?;
            }
        },

        Command::Knowledge {
            command,
            team,
            scope,
        } => match command {
            Some(KnowledgeCommand::List { team: t, scope: s }) => {
                let team_flag = t.as_deref().or(team.as_deref());
                let scope_flag = s.as_deref().or(scope.as_deref());
                commands::knowledge::list(team_flag, scope_flag)?;
            }
            Some(KnowledgeCommand::Show { path, team: t }) => {
                let team_flag = t.as_deref().or(team.as_deref());
                commands::knowledge::show(&path, team_flag)?;
            }
            None => {
                commands::knowledge::interactive(team.as_deref(), scope.as_deref())?;
            }
        },

        Command::Daemon { command } => match command {
            DaemonCommand::Start {
                team,
                mode,
                port,
                interval,
            } => {
                commands::daemon::start(team.as_deref(), &mode, port, interval)?;
            }
            DaemonCommand::Stop { team } => {
                commands::daemon::stop(team.as_deref())?;
            }
            DaemonCommand::Status { team } => {
                commands::daemon::status(team.as_deref())?;
            }
        },

        Command::DaemonRun {
            team,
            mode,
            port,
            interval,
        } => {
            commands::daemon::run_daemon(&team, &mode, port, interval)?;
        }

        Command::Start { team, formation } => {
            commands::start::run(team.as_deref(), formation.as_deref())?;
        }
        Command::Stop { team, force } => {
            commands::stop::run(team.as_deref(), force)?;
        }
        Command::Status { team, verbose } => {
            commands::status::run(team.as_deref(), verbose)?;
        }
        Command::Completions { shell } => {
            commands::completions::run(shell)?;
        }
    }

    Ok(())
}
