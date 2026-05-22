use std::ffi::OsString;

use clap::{Parser, Subcommand};

fn build_version() -> &'static str {
    concat!(env!("CARGO_PKG_VERSION"), env!("BM_VERSION_META"))
}

/// botminter — lead your own Claude Code agents
#[derive(Parser)]
#[command(name = "bm", version = build_version(), about, next_display_order = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Command,
}

#[derive(Subcommand)]
pub enum Command {
    /// Interactive wizard — create a new team
    Init {
        /// Run without interactive prompts (requires --profile, --team-name, --org, --repo)
        #[arg(long)]
        non_interactive: bool,

        /// Profile to use (required with --non-interactive)
        #[arg(long)]
        profile: Option<String>,

        /// Team name (required with --non-interactive)
        #[arg(long)]
        team_name: Option<String>,

        /// GitHub org or user (required with --non-interactive)
        #[arg(long)]
        org: Option<String>,

        /// GitHub repo name (required with --non-interactive)
        #[arg(long)]
        repo: Option<String>,

        /// Project fork URL to add (optional)
        #[arg(long)]
        project: Option<String>,

        /// GitHub Project board title (required with --non-interactive). Creates if not found.
        #[arg(long)]
        github_project_board: Option<String>,

        /// Bridge name to configure (optional)
        #[arg(long)]
        bridge: Option<String>,

        /// Skip GitHub API calls (for testing)
        #[arg(long, hide = true)]
        skip_github: bool,

        /// Override workzone directory
        #[arg(long)]
        workzone: Option<String>,

        /// Import App credentials from a YAML file (for machine migration)
        #[arg(long)]
        credentials_file: Option<String>,
    },

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

        /// Reuse an existing GitHub App instead of creating a new one.
        /// Requires --app-id, --client-id, --private-key-file, and --installation-id.
        #[arg(long)]
        reuse_app: bool,

        /// GitHub App ID (used with --reuse-app)
        #[arg(long, requires = "reuse_app")]
        app_id: Option<String>,

        /// GitHub App Client ID (used with --reuse-app)
        #[arg(long, requires = "reuse_app")]
        client_id: Option<String>,

        /// Path to PEM file with the GitHub App private key (used with --reuse-app)
        #[arg(long, requires = "reuse_app")]
        private_key_file: Option<String>,

        /// GitHub App installation ID (used with --reuse-app)
        #[arg(long, requires = "reuse_app")]
        installation_id: Option<String>,

        /// Save created App credentials to a file
        #[arg(long)]
        save_credentials: Option<String>,
    },

    /// Fire a member (stop, uninstall App, remove credentials and directories)
    Fire {
        /// Member name to fire
        member: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Preserve the GitHub App installation for reuse
        #[arg(long)]
        keep_app: bool,

        /// Skip interactive confirmation
        #[arg(short, long)]
        yes: bool,

        /// Also delete the member's GitHub workspace repo
        #[arg(long)]
        delete_repo: bool,
    },

    /// Start members (all, or a specific one)
    #[command(alias = "up")]
    Start {
        /// Optional member to start (starts all if omitted)
        member: Option<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Formation to deploy with (default: local)
        #[arg(long)]
        formation: Option<String>,

        /// Skip bridge start even if configured
        #[arg(long)]
        no_bridge: bool,

        /// Start bridge only, do not launch members
        #[arg(long)]
        bridge_only: bool,
    },

    /// Stop members (all, or a specific one)
    Stop {
        /// Optional member to stop (stops all if omitted)
        member: Option<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Force-kill via SIGTERM instead of graceful stop
        #[arg(short, long)]
        force: bool,

        /// Also stop the bridge service
        #[arg(long)]
        bridge: bool,

        /// Stop members, daemon, and bridge (full teardown)
        #[arg(long)]
        all: bool,
    },

    /// Enable members for event-driven restart by the daemon
    Enable {
        /// Optional member to enable (enables all if omitted)
        member: Option<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Also start the member(s) immediately
        #[arg(long)]
        now: bool,
    },

    /// Disable members from event-driven restart by the daemon
    Disable {
        /// Optional member to disable (disables all if omitted)
        member: Option<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Also stop the member(s) immediately
        #[arg(long)]
        now: bool,
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

    /// Interactive chat session with a team member
    Chat {
        /// Member name (e.g., architect-01)
        member: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Restrict to a specific hat (e.g., executor, designer)
        #[arg(long)]
        hat: Option<String>,

        /// Print the generated system prompt and exit (no chat session)
        #[arg(long)]
        render_system_prompt: bool,

        /// Run the coding agent in autonomous mode (skip permission prompts)
        #[arg(short, long)]
        autonomous: bool,
    },

    /// Launch Minty, the BotMinter interactive assistant
    Minty {
        /// Team to operate on (gives Minty team-specific context)
        #[arg(short, long)]
        team: Option<String>,

        /// Run the coding agent in autonomous mode (skip permission prompts)
        #[arg(short, long)]
        autonomous: bool,
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

    /// Credential management (export/import for machine migration)
    Credentials {
        #[command(subcommand)]
        command: CredentialsCommand,
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

    /// Bridge service management
    Bridge {
        #[command(subcommand)]
        command: BridgeCommand,
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

        /// Bind address for the HTTP server
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
    },

    /// Internal: run the brain multiplexer event loop (not user-facing)
    #[command(hide = true)]
    BrainRun {
        /// Workspace directory for the brain
        #[arg(long)]
        workspace: String,

        /// Path to the rendered brain system prompt
        #[arg(long)]
        system_prompt: String,

        /// ACP agent binary
        #[arg(long, default_value = "claude-agent-acp")]
        acp_binary: String,
    },

    /// Environment management (prepare or tear down the runtime environment)
    Env {
        #[command(subcommand)]
        command: EnvCommand,
    },

    /// Runtime infrastructure management (VMs)
    #[command(hide = true)]
    Runtime {
        #[command(subcommand)]
        command: RuntimeCommand,
    },

    /// Attach to a running Lima VM
    Attach {
        /// Team to operate on (resolves VM from team config)
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Debugging and diagnostic commands
    Debug {
        #[command(subcommand)]
        command: DebugCommand,
    },


    /// Unknown subcommand (caught when not matching any static or dynamic command)
    #[command(external_subcommand)]
    External(Vec<OsString>),

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
        /// Sync git repositories (push team repo)
        #[arg(long)]
        repos: bool,

        /// Provision bridge identities and rooms
        #[arg(long)]
        bridge: bool,

        /// Equivalent to --repos --bridge (all remote operations)
        #[arg(short = 'a', long)]
        all: bool,

        /// Show detailed sync status per workspace
        #[arg(short, long)]
        verbose: bool,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum RuntimeCommand {
    /// Provision an isolated Fedora VM for a team
    Create {
        /// Run without interactive prompts (requires --name)
        #[arg(long)]
        non_interactive: bool,

        /// Print the rendered Lima template and exit (does not create a VM)
        #[arg(long)]
        render: bool,

        /// VM name (e.g. bm-alpha)
        #[arg(long)]
        name: Option<String>,

        /// Number of CPUs to allocate
        #[arg(long, default_value = "4")]
        cpus: u32,

        /// Memory to allocate (e.g. "8GiB")
        #[arg(long, default_value = "8GiB")]
        memory: String,

        /// Disk size (e.g. "100GiB")
        #[arg(long, default_value = "100GiB")]
        disk: String,

        /// Environment variables to set in the VM (repeatable, e.g. --env ANTHROPIC_API_KEY=sk-...)
        #[arg(long = "env", value_name = "KEY=VALUE")]
        env_vars: Vec<String>,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Delete a Lima VM and remove it from config
    Delete {
        /// VM name to delete
        name: String,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,
    },
}

#[derive(Subcommand)]
pub enum EnvCommand {
    /// Prepare the runtime environment (verify prerequisites, provision infrastructure)
    Create {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Formation to use (default: local)
        #[arg(long)]
        formation: Option<String>,
    },

    /// Tear down the runtime environment
    Delete {
        /// VM name to delete (required for Lima environments)
        name: Option<String>,

        /// Skip confirmation prompt
        #[arg(long)]
        force: bool,

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

        /// Show which files contain agent-specific tags and which agents they reference
        #[arg(long)]
        show_tags: bool,
    },

    /// Extract embedded profiles to ~/.config/botminter/profiles/
    Init {
        /// Overwrite existing profiles without prompting
        #[arg(long)]
        force: bool,
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
pub enum CredentialsCommand {
    /// Export all members' credentials to a YAML file (for machine migration)
    Export {
        /// Output file path
        #[arg(short, long)]
        output: String,

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

        /// Bind address for the HTTP server
        #[arg(long, default_value = "0.0.0.0")]
        bind: String,
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

#[derive(Subcommand)]
pub enum DebugCommand {
    /// Show brain member logs (stderr + LLM conversation)
    BrainLogs {
        /// Member name (e.g., superman-alice)
        member: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Number of stderr lines to show
        #[arg(short = 'n', long, default_value = "20")]
        lines: usize,

        /// Number of recent LLM entries to show
        #[arg(long, default_value = "30")]
        entries: usize,
    },
}

#[derive(Subcommand)]
pub enum BridgeCommand {
    /// Start the bridge service
    Start {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Stop the bridge service
    Stop {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Show bridge status
    Status {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,

        /// Show sensitive information (operator credentials)
        #[arg(long)]
        reveal: bool,
    },

    /// Bridge identity management
    Identity {
        #[command(subcommand)]
        command: BridgeIdentityCommand,
    },

    /// Bridge room management
    Room {
        #[command(subcommand)]
        command: BridgeRoomCommand,
    },
}

#[derive(Subcommand)]
pub enum BridgeIdentityCommand {
    /// Add a new identity to the bridge
    Add {
        /// Username to onboard
        username: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Rotate credentials for an identity
    Rotate {
        /// Username to rotate credentials for
        username: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Remove an identity from the bridge
    Remove {
        /// Username to remove
        username: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Show stored credentials for an identity
    Show {
        /// Username to show credentials for
        username: String,

        /// Show full token (default: masked)
        #[arg(long)]
        reveal: bool,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// List registered identities
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum BridgeRoomCommand {
    /// Create a new room
    Create {
        /// Room name
        name: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// Create a private DM room for a brain member
    #[command(name = "create-dm")]
    CreateDm {
        /// Member name
        member: String,

        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },

    /// List rooms
    List {
        /// Team to operate on
        #[arg(short, long)]
        team: Option<String>,
    },
}
