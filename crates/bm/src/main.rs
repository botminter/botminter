use anyhow::Result;
use clap::{CommandFactory, FromArgMatches, Parser};
use clap_complete::CompleteEnv;

use bm::cli::{
    BridgeCommand, BridgeIdentityCommand, BridgeRoomCommand, Cli, Command, CredentialsCommand,
    DaemonCommand, DebugCommand, EnvCommand, KnowledgeCommand, MembersCommand, ProfilesCommand,
    ProjectsCommand, RolesCommand, RuntimeCommand, TeamsCommand,
};
use bm::commands;

fn main() -> Result<()> {
    CompleteEnv::with_factory(commands::completions::build_cli_with_completions).complete();

    let meetings = bm::profile::detect_meetings();

    let cli = if meetings.is_empty() {
        Cli::parse()
    } else {
        let mut meetings_cmd = clap::Command::new("meetings")
            .about("Meet with a team member for a specific purpose");
        for m in &meetings {
            meetings_cmd = meetings_cmd.subcommand(commands::meeting::build_meeting_subcommand(m));
        }

        let cmd = Cli::command().subcommand(meetings_cmd);
        let matches = cmd.get_matches();

        if let Some(("meetings", meetings_matches)) = matches.subcommand() {
            if let Some((sub_name, sub_matches)) = meetings_matches.subcommand() {
                if let Some(m) = meetings.iter().find(|m| m.name == sub_name) {
                    return commands::meeting::run_meeting(m, sub_matches);
                }
            }
            // `bm meetings` with no subcommand — show help
            eprintln!("Available meetings:");
            for m in &meetings {
                eprintln!("  {:<20} {}", m.name, m.description);
            }
            eprintln!("\nRun `bm meetings <name> --help` for details.");
            std::process::exit(0);
        }

        Cli::from_arg_matches(&matches).map_err(|e| e.exit()).unwrap()
    };

    match cli.command {
        Command::Init {
            non_interactive,
            profile,
            team_name,
            org,
            repo,
            project,
            github_project_board,
            bridge,
            skip_github,
            workzone,
            credentials_file,
        } => {
            if non_interactive {
                commands::init::run_non_interactive(
                    profile,
                    team_name,
                    org,
                    repo,
                    project,
                    github_project_board,
                    bridge,
                    skip_github,
                    workzone,
                    credentials_file,
                )?;
            } else {
                commands::init::run()?;
            }
        }

        Command::Profiles { command } => match command {
            ProfilesCommand::List => commands::profiles::list()?,
            ProfilesCommand::Describe { profile, show_tags } => {
                commands::profiles::describe(&profile, show_tags)?
            }
            ProfilesCommand::Init { force } => commands::profiles_init::run(force)?,
        },

        Command::Teams { command } => match command {
            TeamsCommand::List => commands::teams::list()?,
            TeamsCommand::Show { name, team } => {
                commands::teams::show(name.as_deref(), team.as_deref())?;
            }
            TeamsCommand::Sync { repos, bridge, all, verbose, team } => {
                let effective_repos = repos || all;
                let effective_bridge = bridge || all;
                commands::teams::sync(effective_repos, effective_bridge, verbose, team.as_deref())?;
            }
        },

        Command::Hire {
            role,
            name,
            team,
            reuse_app,
            app_id,
            client_id,
            private_key_file,
            installation_id,
            save_credentials,
        } => {
            let app_flags = commands::hire::AppCredentialFlags {
                reuse_app,
                app_id: app_id.as_deref(),
                client_id: client_id.as_deref(),
                private_key_file: private_key_file.as_deref(),
                installation_id: installation_id.as_deref(),
                save_credentials: save_credentials.as_deref(),
            };
            commands::hire::run(&role, name.as_deref(), team.as_deref(), app_flags)?;
        }

        Command::Fire {
            member,
            team,
            keep_app,
            yes,
            delete_repo,
        } => {
            commands::fire::run(&member, team.as_deref(), keep_app, yes, delete_repo)?;
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

        Command::Credentials { command } => match command {
            CredentialsCommand::Export { output, team } => {
                commands::credentials::export(&output, team.as_deref())?;
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

        Command::Bridge { command } => match command {
            BridgeCommand::Start { team } => commands::bridge::start(team.as_deref())?,
            BridgeCommand::Stop { team } => commands::bridge::stop(team.as_deref())?,
            BridgeCommand::Status { team, reveal } => commands::bridge::status(team.as_deref(), reveal)?,
            BridgeCommand::Identity { command } => match command {
                BridgeIdentityCommand::Add { username, team } => {
                    commands::bridge::identity_add(&username, team.as_deref())?
                }
                BridgeIdentityCommand::Rotate { username, team } => {
                    commands::bridge::identity_rotate(&username, team.as_deref())?
                }
                BridgeIdentityCommand::Remove { username, team } => {
                    commands::bridge::identity_remove(&username, team.as_deref())?
                }
                BridgeIdentityCommand::Show { username, reveal, team } => {
                    commands::bridge::identity_show(&username, reveal, team.as_deref())?
                }
                BridgeIdentityCommand::List { team } => {
                    commands::bridge::identity_list(team.as_deref())?
                }
            },
            BridgeCommand::Room { command } => match command {
                BridgeRoomCommand::Create { name, team } => {
                    commands::bridge::room_create(&name, team.as_deref())?
                }
                BridgeRoomCommand::CreateDm { member, team } => {
                    commands::bridge::room_create_dm(&member, team.as_deref())?
                }
                BridgeRoomCommand::List { team } => {
                    commands::bridge::room_list(team.as_deref())?
                }
            },
        },

        Command::Daemon { command } => match command {
            DaemonCommand::Start {
                team,
                mode,
                port,
                interval,
                bind,
            } => {
                commands::daemon::start(team.as_deref(), &mode, port, interval, &bind)?;
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
            bind,
        } => {
            commands::daemon::run_daemon(&team, &mode, port, interval, &bind)?;
        }

        Command::Chat {
            member,
            team,
            hat,
            render_system_prompt,
            autonomous,
        } => {
            commands::chat::run(
                &member,
                team.as_deref(),
                hat.as_deref(),
                render_system_prompt,
                autonomous,
            )?;
        }

        Command::Minty { team, autonomous } => {
            commands::minty::run(team.as_deref(), autonomous)?;
        }

        Command::Start {
            member,
            team,
            formation,
            no_bridge,
            bridge_only,
        } => {
            commands::start::run(team.as_deref(), formation.as_deref(), no_bridge, bridge_only, member.as_deref())?;
        }
        Command::Stop { member, team, force, bridge, all } => {
            commands::stop::run(team.as_deref(), force, member.as_deref(), bridge, all)?;
        }
        Command::Enable { member, team, now } => {
            commands::enable::run(member.as_deref(), team.as_deref(), now)?;
        }
        Command::Disable { member, team, now } => {
            commands::disable::run(member.as_deref(), team.as_deref(), now)?;
        }
        Command::Status { team, verbose } => {
            commands::status::run(team.as_deref(), verbose)?;
        }
        Command::BrainRun {
            workspace,
            system_prompt,
            acp_binary,
        } => {
            commands::brain_run::run(&workspace, &system_prompt, &acp_binary)?;
        }

        Command::Env { command } => match command {
            EnvCommand::Create { team, formation } => {
                commands::env::create(team.as_deref(), formation.as_deref())?;
            }
            EnvCommand::Delete { name, force, team } => {
                commands::env::delete(name.as_deref(), force, team.as_deref())?;
            }
        },
        Command::Runtime { command } => match command {
            RuntimeCommand::Create {
                non_interactive,
                render,
                name,
                cpus,
                memory,
                disk,
                env_vars,
                team,
            } => {
                if render {
                    commands::bootstrap::render(name, cpus, &memory, &disk, team.as_deref());
                } else {
                    commands::bootstrap::run(non_interactive, name, cpus, &memory, &disk, &env_vars, team.as_deref())?;
                }
            }
            RuntimeCommand::Delete { name, force } => {
                commands::bootstrap::delete(&name, force)?;
            }
        },
        Command::Attach { team } => {
            commands::attach::run(team.as_deref())?;
        }
        Command::Debug { command } => match command {
            DebugCommand::BrainLogs {
                member,
                team,
                lines,
                entries,
            } => {
                commands::debug::brain_logs(&member, team.as_deref(), lines, entries)?;
            }
        },
        Command::External(args) => {
            commands::meeting::run_external(args)?;
        }
        Command::Completions { shell } => {
            commands::completions::run(shell)?;
        }
    }

    Ok(())
}
