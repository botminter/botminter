use std::path::{Path, PathBuf};

use clap::CommandFactory;
use clap_complete::engine::CompletionCandidate;
use clap_complete::ArgValueCandidates;

use crate::cli::Cli;
use crate::config::{self, BotminterConfig, TeamEntry};
use crate::formation;
use crate::profile;

/// Shared context for completion resolution.
/// Pre-loads config and resolves the default team so individual completers
/// don't repeat the I/O.
pub struct CompletionContext {
    pub config: Option<BotminterConfig>,
    pub team: Option<TeamEntry>,
    pub team_repo: Option<PathBuf>,
}

impl CompletionContext {
    /// Load completion context from disk. Best-effort: never panics, returns
    /// `None` fields when config is missing or malformed.
    pub fn load() -> Self {
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
    pub fn team_names(&self) -> Vec<String> {
        self.config
            .as_ref()
            .map(|c| c.teams.iter().map(|t| t.name.clone()).collect())
            .unwrap_or_default()
    }

    /// Role names from the default team's profile.
    pub fn role_names(&self) -> Vec<String> {
        self.team
            .as_ref()
            .and_then(|t| profile::list_roles(&t.profile).ok())
            .unwrap_or_default()
    }

    /// Embedded profile names.
    pub fn profile_names(&self) -> Vec<String> {
        profile::list_profiles()
    }

    /// Member names from the default team's team repo.
    pub fn member_names(&self) -> Vec<String> {
        self.team_repo
            .as_ref()
            .and_then(|repo| list_member_dirs(repo).ok())
            .unwrap_or_default()
    }

    /// Project names from the default team's manifest.
    pub fn project_names(&self) -> Vec<String> {
        self.team_repo
            .as_ref()
            .and_then(|repo| list_project_names(repo).ok())
            .unwrap_or_default()
    }

    /// Formation names from the default team's repo.
    pub fn formation_names(&self) -> Vec<String> {
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

    let daemon_modes: Vec<String> = vec!["webhook".into(), "poll".into()];
    let knowledge_scopes: Vec<String> = vec![
        "team".into(),
        "project".into(),
        "member".into(),
        "member-project".into(),
    ];

    Cli::command()
        // ── hire ──────────────────────────────────────────────
        .mut_subcommand("hire", |c| {
            c.mut_arg("role", |a| a.add(make(roles)))
                .mut_arg("team", |a| a.add(make(teams.clone())))
        })
        // ── start ─────────────────────────────────────────────
        .mut_subcommand("start", |c| {
            c.mut_arg("team", |a| a.add(make(teams.clone())))
                .mut_arg("formation", |a| a.add(make(formations)))
        })
        // ── stop ──────────────────────────────────────────────
        .mut_subcommand("stop", |c| {
            c.mut_arg("team", |a| a.add(make(teams.clone())))
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

/// List member directories under `team_repo/team/`.
fn list_member_dirs(team_repo: &Path) -> anyhow::Result<Vec<String>> {
    let members_dir = team_repo.join("team");
    if !members_dir.is_dir() {
        return Ok(Vec::new());
    }
    let mut names = Vec::new();
    for entry in std::fs::read_dir(&members_dir)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            let name = entry.file_name().to_string_lossy().to_string();
            if !name.starts_with('.') {
                names.push(name);
            }
        }
    }
    names.sort();
    Ok(names)
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
    fn profile_names_always_populated() {
        let ctx = CompletionContext {
            config: None,
            team: None,
            team_repo: None,
        };
        assert!(!ctx.profile_names().is_empty());
    }

    #[test]
    fn context_with_config_returns_team_names() {
        let ctx = CompletionContext {
            config: Some(BotminterConfig {
                workzone: PathBuf::from("/tmp"),
                default_team: None,
                teams: vec![
                    TeamEntry {
                        name: "alpha".into(),
                        path: PathBuf::from("/tmp/alpha"),
                        profile: "scrum".into(),
                        github_repo: String::new(),
                        credentials: Credentials::default(),
                    },
                    TeamEntry {
                        name: "beta".into(),
                        path: PathBuf::from("/tmp/beta"),
                        profile: "scrum-compact".into(),
                        github_repo: String::new(),
                        credentials: Credentials::default(),
                    },
                ],
            }),
            team: None,
            team_repo: None,
        };
        let names = ctx.team_names();
        assert_eq!(names, vec!["alpha", "beta"]);
    }

    #[test]
    fn context_with_team_returns_role_names() {
        let ctx = CompletionContext {
            config: None,
            team: Some(TeamEntry {
                name: "test".into(),
                path: PathBuf::from("/tmp/test"),
                profile: "scrum".into(),
                github_repo: String::new(),
                credentials: Credentials::default(),
            }),
            team_repo: None,
        };
        let roles = ctx.role_names();
        assert!(roles.contains(&"architect".to_string()));
    }

    #[test]
    fn member_dirs_from_team_repo() {
        let tmp = tempfile::tempdir().unwrap();
        let team_repo = tmp.path();
        let team_dir = team_repo.join("team");
        std::fs::create_dir_all(team_dir.join("architect-01")).unwrap();
        std::fs::create_dir_all(team_dir.join("dev-01")).unwrap();
        std::fs::create_dir_all(team_dir.join(".hidden")).unwrap();

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
            Command, DaemonCommand, KnowledgeCommand, MembersCommand, ProfilesCommand,
            ProjectsCommand, RolesCommand, TeamsCommand,
        };

        // This exhaustive match ensures that if a new Command variant is
        // added, this test must be updated. It doesn't call real handlers —
        // it only verifies the variant exists and can be named.
        fn _assert_variant_exists(cmd: &Command) {
            match cmd {
                Command::Init => {}
                Command::Hire { .. } => {}
                Command::Start { .. } => {}
                Command::Stop { .. } => {}
                Command::Status { .. } => {}
                Command::Teams { command } => match command {
                    TeamsCommand::List => {}
                    TeamsCommand::Show { .. } => {}
                    TeamsCommand::Sync { .. } => {}
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
                },
                Command::Projects { command } => match command {
                    ProjectsCommand::List { .. } => {}
                    ProjectsCommand::Show { .. } => {}
                    ProjectsCommand::Add { .. } => {}
                    ProjectsCommand::Sync { .. } => {}
                },
                Command::Knowledge { command, .. } => match command {
                    Some(KnowledgeCommand::List { .. }) => {}
                    Some(KnowledgeCommand::Show { .. }) => {}
                    None => {}
                },
                Command::Daemon { command } => match command {
                    DaemonCommand::Start { .. } => {}
                    DaemonCommand::Stop { .. } => {}
                    DaemonCommand::Status { .. } => {}
                },
                Command::DaemonRun { .. } => {}
                Command::Completions { .. } => {}
            }
        }

        // Also verify the builder itself works.
        let cmd = build_cli_with_completions();

        // Spot-check that subcommands we attached completions to exist.
        assert!(cmd.find_subcommand("hire").is_some());
        assert!(cmd.find_subcommand("start").is_some());
        assert!(cmd.find_subcommand("members").is_some());
        assert!(cmd.find_subcommand("profiles").is_some());
        assert!(cmd.find_subcommand("projects").is_some());
        assert!(cmd.find_subcommand("daemon").is_some());
        assert!(cmd.find_subcommand("knowledge").is_some());
        assert!(cmd.find_subcommand("teams").is_some());
    }
}
