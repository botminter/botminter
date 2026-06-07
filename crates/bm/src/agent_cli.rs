use clap::{Parser, Subcommand, ValueEnum};

/// Agent-facing tools for BotMinter workspaces.
#[derive(Parser)]
#[command(name = "bm-agent", version, about)]
pub struct AgentCli {
    #[command(subcommand)]
    pub command: AgentCommand,
}

#[derive(Subcommand)]
pub enum AgentCommand {
    /// Brain-to-loop messaging
    Inbox {
        #[command(subcommand)]
        command: InboxCommand,
    },
    /// Claude Code specific tools
    Claude {
        #[command(subcommand)]
        command: ClaudeCommand,
    },
    /// Loop management via daemon
    Loop {
        #[command(subcommand)]
        command: LoopCommand,
    },
    /// Work-item locking via daemon
    Lock {
        #[command(subcommand)]
        command: LockCommand,
    },
}

#[derive(Subcommand)]
pub enum LoopCommand {
    /// Start a new Ralph loop via the daemon
    Start {
        /// The prompt for the Ralph loop
        prompt: String,
        /// Member to run the loop as (defaults to first member)
        #[arg(long)]
        member: Option<String>,
    },
}

#[derive(Subcommand)]
pub enum InboxCommand {
    /// Send a message to the loop's inbox
    Write {
        /// Message text
        message: String,
        /// Sender identity
        #[arg(long, default_value = "brain")]
        from: String,
    },
    /// Read and consume pending messages
    Read {
        /// Output format
        #[arg(long, default_value = "hook")]
        format: InboxFormat,
    },
    /// View pending messages without consuming
    Peek,
}

#[derive(Subcommand)]
pub enum ClaudeCommand {
    /// Claude Code hook handlers
    Hook {
        #[command(subcommand)]
        command: ClaudeHookCommand,
    },
}

#[derive(Subcommand)]
pub enum ClaudeHookCommand {
    /// PostToolUse hook — checks inbox, returns additionalContext
    PostToolUse,
}

#[derive(Subcommand)]
pub enum LockCommand {
    /// Acquire a work-item lock for this session
    Acquire {
        /// Work item ID to lock (e.g. ISSUE-42)
        work_item_id: String,
    },
    /// Release a work-item lock held by this session
    Release {
        /// Work item ID to unlock (e.g. ISSUE-42)
        work_item_id: String,
    },
}

#[derive(Clone, ValueEnum)]
pub enum InboxFormat {
    Json,
    Hook,
}
