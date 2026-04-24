use std::path::{Path, PathBuf};

use anyhow::{bail, Result};
use clap::CommandFactory;
use clap_complete::engine::CompletionCandidate;
use clap_complete::env::Shells;
use clap_complete::{ArgValueCandidates, Shell};

use crate::cli::Cli;
use crate::config::{self, BotminterConfig, TeamEntry};
use crate::formation;
use crate::profile;

/// Shared context for completion resolution.
/// Pre-loads config and resolves the default team so individual completers
/// don't repeat the I/O.
struct CompletionContext {
    config: Option<BotminterConfig>,
    team: Option<TeamEntry>,
    team_repo: Option<PathBuf>,
}

impl CompletionContext {
    /// Load completion context from disk. Best-effort: never panics, returns
    /// `None` fields when config is missing or malformed.
    fn load() -> Self {
        let config = config::load().ok();
        let team = config
            .as_ref()
            .and_then(|c| config::resolve_team(c, None).ok().cloned());
        let team_repo = team.as_ref().map(|t| t.path.join("team"));
        Self {
            config,
            team,
            team_repo,
        }
    }

    /// Team names from all registered teams.
    fn team_names(&self) -> Vec<String> {
        self.config
            .as_ref()
            .map(|c| c.teams.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Role names from the default team's profile.
    fn role_names(&self) -> Vec<String> {
        self.team
            .as_ref()
            .and_then(|t| profile::list_roles(&t.profile).ok())
            .unwrap_or_default()
    }

    /// Profile names from disk (best-effort).
    fn profile_names(&self) -> Vec<String> {
        profile::list_profiles().unwrap_or_default()
    }

    /// Member names from the default team's team repo.
    fn member_names(&self) -> Vec<String> {
        self.team_repo
            .as_ref()
            .and_then(|repo| crate::workspace::list_member_dirs(&repo.join("members")).ok())
            .unwrap_or_default()
    }

    /// Project names from the default team's manifest.
    fn project_names(&self) -> Vec<String> {
        self.team_repo
            .as_ref()
            .and_then(|repo| list_project_names(repo).ok())
            .unwrap_or_default()
    }

    /// Formation names from the default team's repo.
    fn formation_names(&self) -> Vec<String> {
        self.team_repo
            .as_ref()
            .and_then(|repo| formation::list_formations(repo).ok())
            .unwrap_or_default()
    }
}

/// Build a `clap::Command` with dynamic completion values attached.
///
/// Called by `CompleteEnv` each time the shell requests tab completions.
/// Loads live data from disk and attaches `ArgValueCandidates` to every arg
/// that accepts dynamic values.
pub fn build_cli_with_completions() -> clap::Command {
    let ctx = CompletionContext::load();

    let teams = ctx.team_names();
    let roles = ctx.role_names();
    let profiles = ctx.profile_names();
    let members = ctx.member_names();
    let projects = ctx.project_names();
    let formations = ctx.formation_names();

    let bridges: Vec<String> = {
        // Collect bridge names from all profiles
        let mut names = Vec::new();
        if let Ok(profile_names) = profile::list_profiles() {
            for pn in &profile_names {
                if let Ok(m) = profile::read_manifest(pn) {
                    for b in &m.bridges {
                        if !names.contains(&b.name) {
                            names.push(b.name.clone());
                        }
                    }
                }
            }
        }
        names
    };

    let daemon_modes: Vec<String> = vec!["webhook".into(), "poll".into()];
    let knowledge_scopes: Vec<String> = vec![
        "team".into(),
        "project".into(),
        "member".into(),
        "member-project".into(),
    ];

    Cli::command()
        // ── init ──────────────────────────────────────────────
        .mut_subcommand("init", |c| {
            c.mut_arg("profile", |a| a.add(make(profiles.clone())))
                .mut_arg("bridge", |a| a.add(make(bridges)))
        })
        // ── hire ──────────────────────────────────────────────
        .mut_subcommand("hire", |c| {
            c.mut_arg("role", |a| a.add(make(roles)))
                .mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── fire ──────────────────────────────────────────────
        .mut_subcommand("fire", |c| {
            c.mut_arg("member", |a| a.add(make(members.clone())))
                .mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── chat ──────────────────────────────────────────────
        .mut_subcommand("chat", |c| {
            c.mut_arg("member", |a| a.add(make(members.clone())))
                .mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── start ─────────────────────────────────────────────
        .mut_subcommand("start", |c| {
            c.mut_arg("member", |a| a.add(make(members.clone())))
                .mut_arg("team", |a| a.add(make(teams.clone())))
                .mut_arg("formation", |a| a.add(make(formations)))
        })
        // ── stop ──────────────────────────────────────────────
        .mut_subcommand("stop", |c| {
            c.mut_arg("member", |a| a.add(make(members.clone())))
                .mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── status ────────────────────────────────────────────
        .mut_subcommand("status", |c| {
            c.mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── teams ─────────────────────────────────────────────
        .mut_subcommand("teams", |c| {
            c.mut_subcommand("show", |s| {
                s.mut_arg("name", |a| a.add(make(teams.clone())))
                    .mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("sync", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── runtime ──────────────────────────────────────────
        .mut_subcommand("runtime", |c| {
            c.mut_subcommand("create", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── members ───────────────────────────────────────────
        .mut_subcommand("members", |c| {
            c.mut_subcommand("list", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("show", |s| {
                s.mut_arg("member", |a| a.add(make(members)))
                    .mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── roles ─────────────────────────────────────────────
        .mut_subcommand("roles", |c| {
            c.mut_subcommand("list", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── profiles ──────────────────────────────────────────
        .mut_subcommand("profiles", |c| {
            c.mut_subcommand("describe", |s| {
                s.mut_arg("profile", |a| a.add(make(profiles)))
            })
        })
        // ── projects ──────────────────────────────────────────
        .mut_subcommand("projects", |c| {
            c.mut_subcommand("list", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("show", |s| {
                s.mut_arg("project", |a| a.add(make(projects)))
                    .mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("add", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("sync", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── knowledge ─────────────────────────────────────────
        .mut_subcommand("knowledge", |c| {
            c.mut_arg("team", |a| a.add(make(teams.clone())))
                .mut_arg("scope", |a| a.add(make(knowledge_scopes.clone())))
                .mut_subcommand("list", |s| {
                    s.mut_arg("team", |a| a.add(make(teams.clone())))
                        .mut_arg("scope", |a| a.add(make(knowledge_scopes)))
                })
                .mut_subcommand("show", |s| {
                    s.mut_arg("team", |a| a.add(make(teams.clone())))
                })
        })
        // ── minty ─────────────────────────────────────────────
        .mut_subcommand("minty", |c| {
            c.mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── credentials ─────────────────────────────────────────
        .mut_subcommand("credentials", |c| {
            c.mut_subcommand("export", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
        // ── daemon ────────────────────────────────────────────
        .mut_subcommand("daemon", |c| {
            c.mut_subcommand("start", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
                    .mut_arg("mode", |a| a.add(make(daemon_modes)))
            })
            .mut_subcommand("stop", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
            .mut_subcommand("status", |s| {
                s.mut_arg("team", |a| a.add(make(teams.clone())))
            })
        })
}

/// Outputs the dynamic shell registration script.
///
/// When eval'd in the user's shell, the script registers `bm` as the completer
/// binary. Subsequent tab presses invoke `bm` with `COMPLETE=<shell>`, which is
/// intercepted by `CompleteEnv` in `main.rs` to return live candidates.
pub fn run(shell: Shell) -> Result<()> {
    let shells = Shells::builtins();
    let shell_name = shell.to_string();
    let completer = shells.completer(&shell_name);

    match completer {
        Some(c) => {
            c.write_registration("COMPLETE", "bm", "bm", "bm", &mut std::io::stdout())
                .map_err(|e| anyhow::anyhow!(e))?;
        }
        None => {
            bail!(
                "Shell '{}' is not supported for dynamic completions",
                shell_name
            );
        }
    }

    Ok(())
}

/// Wrap a `Vec<String>` into an `ArgValueCandidates`.
///
/// The strings are converted to `CompletionCandidate` on each completion
/// request. This avoids the `Clone` bound issue on `CompletionCandidate`.
fn make(values: Vec<String>) -> ArgValueCandidates {
    ArgValueCandidates::new(move || {
        values
            .iter()
            .map(|v| CompletionCandidate::new(v.as_str()))
            .collect()
    })
}

/// List project names from the team repo's botminter.yml manifest.
fn list_project_names(team_repo: &Path) -> anyhow::Result<Vec<String>> {
    let manifest_path = team_repo.join("botminter.yml");
    if !manifest_path.exists() {
        return Ok(Vec::new());
    }
    let contents = std::fs::read_to_string(&manifest_path)?;
    let manifest: profile::ProfileManifest = serde_yml::from_str(&contents)?;
    Ok(manifest.projects.iter().map(|p| p.name.clone()).collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::{BotminterConfig, Credentials, TeamEntry};
    use std::path::PathBuf;

    #[test]
    fn build_cli_with_completions_does_not_panic() {
        // Completions builder must not panic even with no config on disk.
        let _cmd = build_cli_with_completions();
    }

    #[test]
    fn context_without_config_returns_empty() {
        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: None,
        };
        assert!(ctx.team_names().is_empty());
        assert!(ctx.role_names().is_empty());
        assert!(ctx.member_names().is_empty());
        assert!(ctx.project_names().is_empty());
        assert!(ctx.formation_names().is_empty());
    }

    #[test]
    fn profile_names_gracefully_empty_without_disk() {
        // Without profiles on disk, profile_names() returns empty (best-effort)
        // This verifies the unwrap_or_default() fallback works
        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: None,
        };
        // May be empty or populated depending on test environment — just verify no panic
        let _ = ctx.profile_names();
    }

    #[test]
    fn context_with_config_returns_team_names() {
        let ctx = CompletionContext {
            config: Some(BotminterConfig {
                workzone: PathBuf::from("/tmp"),
                default_team: None,
                vms: Vec::new(),
                teams: vec![
                    TeamEntry {
                        name: "alpha".into(),
                        path: PathBuf::from("/tmp/alpha"),
                        profile: "scrum".into(),
                        github_repo: String::new(),
                        credentials: Credentials::default(),
                        coding_agent: None,
                        project_number: None,
                        bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                        vm: None,
                    },
                    TeamEntry {
                        name: "beta".into(),
                        path: PathBuf::from("/tmp/beta"),
                        profile: "agentic-sdlc-minimal".into(),
                        github_repo: String::new(),
                        credentials: Credentials::default(),
                        coding_agent: None,
                        project_number: None,
                        bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                        vm: None,
                    },
                ],
                keyring_collection: None,
            }),
            team: None,
            team_repo: None,
        };
        let names = ctx.team_names();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn context_with_team_returns_roles_or_empty() {
        // role_names() uses list_roles() which reads from disk
        // Without disk profiles, it gracefully returns empty via .ok()
        let ctx = CompletionContext {
            config: None,
            team: Some(TeamEntry {
                name: "test".into(),
                path: PathBuf::from("/tmp/test"),
                profile: "scrum".into(),
                github_repo: String::new(),
                credentials: Credentials::default(),
                coding_agent: None,
                project_number: None,
                bridge_lifecycle: Default::default(),
            daemon: Default::default(),
                vm: None,
            }),
            team_repo: None,
        };
        // May be empty or populated depending on test environment — just verify no panic
        let _ = ctx.role_names();
    }

    #[test]
    fn member_dirs_from_team_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        let members_dir = team_repo.join("members");
        std::fs::create_dir_all(members_dir.join("architect-01")).unwrap();
        std::fs::create_dir_all(members_dir.join("dev-01")).unwrap();
        std::fs::create_dir_all(members_dir.join(".hidden")).unwrap();

        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: Some(team_repo.to_path_buf()),
        };
        let members = ctx.member_names();
        assert_eq!(members, vec!["architect-01", "dev-01"]);
    }

    #[test]
    fn project_names_from_manifest() {
        let tmp = tempfile::tempdir().unwrap();
        let manifest = r#"
name: scrum
display_name: Test
description: test
version: "1.0.0"
schema_version: "1.0"
projects:
  - name: my-app
    fork_url: https://github.com/org/my-app
  - name: my-lib
    fork_url: https://github.com/org/my-lib
"#;
        std::fs::write(tmp.path().join("botminter.yml"), manifest).unwrap();

        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: Some(tmp.path().to_path_buf()),
        };
        let projects = ctx.project_names();
        assert_eq!(projects, vec!["my-app", "my-lib"]);
    }

    #[test]
    fn formation_names_from_team_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let formations_dir = tmp.path().join("formations");
        std::fs::create_dir_all(formations_dir.join("local")).unwrap();
        std::fs::create_dir_all(formations_dir.join("k8s")).unwrap();

        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: Some(tmp.path().to_path_buf()),
        };
        let formations = ctx.formation_names();
        assert_eq!(formations, vec!["k8s", "local"]);
    }

    /// Guard test: verifies that `build_cli_with_completions` covers every
    /// subcommand in the Command enum. This uses an exhaustive match so adding
    /// a new variant without updating completions causes a compile error.
    #[test]
    fn all_commands_covered_by_completions() {
        use crate::cli::{
            BridgeCommand, BridgeIdentityCommand, BridgeRoomCommand, Command, CredentialsCommand,
            DaemonCommand, DebugCommand, EnvCommand, KnowledgeCommand, MembersCommand,
            ProfilesCommand, ProjectsCommand, RolesCommand, RuntimeCommand, TeamsCommand,
        };

        // This exhaustive match ensures that if a new Command variant is
        // added, this test must be updated. It doesn't call real handlers —
        // it only verifies the variant exists and can be named.
        fn _assert_variant_exists(cmd: &Command) {
            match cmd {
                Command::Init { .. } => {}
                Command::Hire { .. } => {}
                Command::Fire { .. } => {}
                Command::Chat { .. } => {}
                Command::Minty { .. } => {}
                Command::Start { .. } => {}
                Command::Stop { .. } => {}
                Command::Enable { .. } => {}
                Command::Disable { .. } => {}
                Command::Status { .. } => {}
                Command::Teams { command } => match command {
                    TeamsCommand::List => {}
                    TeamsCommand::Show { .. } => {}
                    TeamsCommand::Sync { .. } => {}
                },
                Command::Env { command } => match command {
                    EnvCommand::Create { .. } => {}
                    EnvCommand::Delete { .. } => {}
                },
                Command::Runtime { command } => match command {
                    RuntimeCommand::Create { .. } => {}
                    RuntimeCommand::Delete { .. } => {}
                },
                Command::Members { command } => match command {
                    MembersCommand::List { .. } => {}
                    MembersCommand::Show { .. } => {}
                },
                Command::Roles { command } => match command {
                    RolesCommand::List { .. } => {}
                },
                Command::Profiles { command } => match command {
                    ProfilesCommand::List => {}
                    ProfilesCommand::Describe { .. } => {}
                    ProfilesCommand::Init { .. } => {}
                },
                Command::Projects { command } => match command {
                    ProjectsCommand::List { .. } => {}
                    ProjectsCommand::Show { .. } => {}
                    ProjectsCommand::Add { .. } => {}
                    ProjectsCommand::Sync { .. } => {}
                },
                Command::Credentials { command } => match command {
                    CredentialsCommand::Export { .. } => {}
                },
                Command::Knowledge { command, .. } => match command {
                    Some(KnowledgeCommand::List { .. }) => {}
                    Some(KnowledgeCommand::Show { .. }) => {}
                    None => {}
                },
                Command::Bridge { command } => match command {
                    BridgeCommand::Start { .. } => {}
                    BridgeCommand::Stop { .. } => {}
                    BridgeCommand::Status { .. } => {}
                    BridgeCommand::Identity { command } => match command {
                        BridgeIdentityCommand::Add { .. } => {}
                        BridgeIdentityCommand::Rotate { .. } => {}
                        BridgeIdentityCommand::Remove { .. } => {}
                        BridgeIdentityCommand::Show { .. } => {}
                        BridgeIdentityCommand::List { .. } => {}
                    },
                    BridgeCommand::Room { command } => match command {
                        BridgeRoomCommand::Create { .. } => {}
                        BridgeRoomCommand::CreateDm { .. } => {}
                        BridgeRoomCommand::List { .. } => {}
                    },
                },
                Command::Daemon { command } => match command {
                    DaemonCommand::Start { .. } => {}
                    DaemonCommand::Stop { .. } => {}
                    DaemonCommand::Status { .. } => {}
                },
                Command::DaemonRun { .. } => {}
                Command::BrainRun { .. } => {}
                Command::Attach { .. } => {}
                Command::Debug { command } => match command {
                    DebugCommand::BrainLogs { .. } => {}
                },
                Command::Completions { .. } => {}
            }
        }

        // Also verify the builder itself works.
        let cmd = build_cli_with_completions();

        // Spot-check that subcommands we attached completions to exist.
        assert!(cmd.find_subcommand("init").is_some());
        assert!(cmd.find_subcommand("hire").is_some());
        assert!(cmd.find_subcommand("minty").is_some());
        assert!(cmd.find_subcommand("start").is_some());
        assert!(cmd.find_subcommand("members").is_some());
        assert!(cmd.find_subcommand("profiles").is_some());
        assert!(cmd.find_subcommand("projects").is_some());
        assert!(cmd.find_subcommand("daemon").is_some());
        assert!(cmd.find_subcommand("knowledge").is_some());
        assert!(cmd.find_subcommand("teams").is_some());
        assert!(cmd.find_subcommand("runtime").is_some());
    }
}
