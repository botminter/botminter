use clap::{Parser, Subcommand};

/// botminter — manage agentic teams
#[derive(Parser)]
#[command(name = "bm", version, about)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Interactive wizard — create a new team
    Init,

    /// Hire a member into a role
    Hire {
        /// Role to hire (e.g. architect, dev)
        role: String,

        /// Member name (auto-generated if omitted)
        #[arg(long)]
        name: Option<String>,

        /// Team to operate on (defaults to default team)
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Start all members
    #[command(alias = "up")]
    Start {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Formation to deploy with (default: local)
        #[arg(long)]
        formation: Option<String>,
    },

    /// Stop all members
    Stop {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Force-kill via SIGTERM instead of graceful stop
        #[arg(short, long)]
        force: bool,
    },

    /// Status dashboard
    Status {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Show verbose Ralph runtime details
        #[arg(short, long)]
        verbose: bool,
    },

    /// Team management commands
    Teams {
        #[command(subcommand)]
        command: TeamsCommand,
    },

    /// Member management commands
    Members {
        #[command(subcommand)]
        command: MembersCommand,
    },

    /// Role listing commands
    Roles {
        #[command(subcommand)]
        command: RolesCommand,
    },

    /// Profile management commands
    Profiles {
        #[command(subcommand)]
        command: ProfilesCommand,
    },

    /// Project management commands
    Projects {
        #[command(subcommand)]
        command: ProjectsCommand,
    },

    /// Knowledge and invariant management
    Knowledge {
        #[command(subcommand)]
        command: Option<KnowledgeCommand>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Filter by scope: team, project, member, or member-project
        #[arg(long)]
        scope: Option<String>,
    },

    /// Event-driven daemon management
    Daemon {
        #[command(subcommand)]
        command: DaemonCommand,
    },

    /// Internal: run the daemon event loop (not user-facing)
    #[command(hide = true)]
    DaemonRun {
        /// Team name
        #[arg(long)]
        team: String,

        /// Daemon mode: webhook or poll
        #[arg(long)]
        mode: String,

        /// HTTP listener port for webhook mode
        #[arg(long)]
        port: u16,

        /// Polling interval in seconds for poll mode
        #[arg(long)]
        interval: u64,
    },

    /// Generate dynamic shell completions
    ///
    /// Completions are dynamic: tab suggestions include real team names, roles,
    /// members, profiles, formations, and projects from your configuration.
    #[command(after_long_help = "\
Examples:
  Bash (add to ~/.bashrc):
    echo 'eval \"$(bm completions bash)\"' >> ~/.bashrc

  Zsh (add to ~/.zshrc):
    echo 'eval \"$(bm completions zsh)\"' >> ~/.zshrc

  Fish (save to completions directory):
    bm completions fish > ~/.config/fish/completions/bm.fish

  PowerShell (add to $PROFILE):
    echo 'bm completions powershell | Invoke-Expression' >> $PROFILE

  Elvish (add to ~/.elvish/rc.elv):
    echo 'eval (bm completions elvish | slurp)' >> ~/.elvish/rc.elv

The generated script delegates to the bm binary at tab-time, so completions
always reflect your current configuration (teams, roles, members, etc.).")]
    Completions {
        /// Shell to generate completions for
        shell: clap_complete::Shell,
    },
}

#[derive(Subcommand)]
pub enum TeamsCommand {
    /// List all registered teams
    List,

    /// Show detailed information about a team
    Show {
        /// Team name (uses default team if omitted)
        name: Option<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Reconcile workspaces with team repo state
    Sync {
        /// Push team repo to GitHub before syncing
        #[arg(long)]
        push: bool,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum MembersCommand {
    /// List hired members for a team
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Show detailed information about a member
    Show {
        /// Member name (e.g., architect-01)
        member: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RolesCommand {
    /// List available roles from the team's profile
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum ProfilesCommand {
    /// List all embedded profiles
    List,

    /// Show detailed profile information
    Describe {
        /// Profile name to describe
        profile: String,
    },
}

#[derive(Subcommand)]
pub enum ProjectsCommand {
    /// List projects configured for the team
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Show detailed information about a project
    Show {
        /// Project name
        project: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Add a project to the team
    Add {
        /// Git URL of the project fork
        url: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Sync GitHub Project board status options and print view setup instructions
    Sync {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum KnowledgeCommand {
    /// List knowledge/invariant files grouped by scope
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Filter by scope: team, project, member, or member-project
        #[arg(long)]
        scope: Option<String>,
    },

    /// Show the contents of a knowledge/invariant file
    Show {
        /// Path to the file (relative to team repo root)
        path: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum DaemonCommand {
    /// Start the event-driven daemon
    Start {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Daemon mode: webhook or poll
        #[arg(long, default_value = "webhook")]
        mode: String,

        /// HTTP listener port for webhook mode
        #[arg(long, default_value = "8484")]
        port: u16,

        /// Polling interval in seconds for poll mode
        #[arg(long, default_value = "60")]
        interval: u64,
    },

    /// Stop the running daemon
    Stop {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Show daemon status
    Status {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}
