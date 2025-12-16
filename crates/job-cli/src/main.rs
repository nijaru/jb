mod client;
mod commands;

use anyhow::Result;
use clap::{Parser, Subcommand};

#[derive(Parser)]
#[command(name = "jb")]
#[command(about = "Background job manager for AI agents", long_about = None)]
#[command(version)]
struct Cli {
    #[command(subcommand)]
    command: Commands,

    /// Output as JSON
    #[arg(long, global = true)]
    json: bool,
}

#[derive(Subcommand)]
enum Commands {
    /// Start a background job
    Run {
        /// Command to execute
        command: String,

        /// Human-readable job name
        #[arg(short, long)]
        name: Option<String>,

        /// Timeout duration (e.g., 30s, 5m, 1h)
        #[arg(short, long)]
        timeout: Option<String>,

        /// JSON context metadata
        #[arg(short, long)]
        context: Option<String>,

        /// Idempotency key (skip if job with key exists)
        #[arg(short = 'k', long)]
        key: Option<String>,

        /// Wait for job to complete
        #[arg(short, long)]
        wait: bool,
    },

    /// List jobs
    List {
        /// Filter by status (pending, running, completed, failed, stopped, interrupted)
        #[arg(short, long)]
        status: Option<String>,

        /// Only jobs from current project
        #[arg(long)]
        here: bool,

        /// Show all jobs across all projects
        #[arg(long)]
        all: bool,
    },

    /// Show job or system status
    Status {
        /// Job ID or name (omit for system status)
        id: Option<String>,
    },

    /// Show job output
    Logs {
        /// Job ID or name
        id: String,

        /// Show last N lines (default: 50 if flag present)
        #[arg(short, long, num_args = 0..=1, default_missing_value = "50")]
        tail: Option<usize>,

        /// Follow output as it's written
        #[arg(short, long)]
        follow: bool,
    },

    /// Stop a running job
    Stop {
        /// Job ID or name
        id: String,

        /// Force kill (SIGKILL instead of SIGTERM)
        #[arg(short, long)]
        force: bool,
    },

    /// Wait for a job to complete
    Wait {
        /// Job ID or name
        id: String,

        /// Timeout duration (e.g., 5m, 1h)
        #[arg(short, long)]
        timeout: Option<String>,
    },

    /// Re-run a job
    Retry {
        /// Job ID or name
        id: String,
    },

    /// Remove old jobs
    Clean {
        /// Remove jobs older than duration (default: 7d)
        #[arg(long, default_value = "7d")]
        older_than: String,

        /// Only remove jobs with specific status
        #[arg(long)]
        status: Option<String>,

        /// Remove all non-running jobs
        #[arg(long)]
        all: bool,
    },

    /// Install Claude skills
    Skills {
        #[command(subcommand)]
        action: Option<SkillsAction>,
    },
}

#[derive(Subcommand)]
enum SkillsAction {
    /// Install skills to ~/.claude/skills/job/
    Install,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::WARN.into()),
        )
        .init();

    let cli = Cli::parse();

    match cli.command {
        Commands::Run {
            command,
            name,
            timeout,
            context,
            key,
            wait,
        } => commands::run::execute(command, name, timeout, context, key, wait, cli.json).await,
        Commands::List { status, here, all } => {
            commands::list::execute(status, here, all, cli.json).await
        }
        Commands::Status { id } => commands::status::execute(id, cli.json).await,
        Commands::Logs { id, tail, follow } => commands::logs::execute(id, tail, follow).await,
        Commands::Stop { id, force } => commands::stop::execute(id, force, cli.json).await,
        Commands::Wait { id, timeout } => commands::wait::execute(id, timeout).await,
        Commands::Retry { id } => commands::retry::execute(id, cli.json).await,
        Commands::Clean {
            older_than,
            status,
            all,
        } => commands::clean::execute(older_than, status, all).await,
        Commands::Skills { action } => commands::skills::execute(action).await,
    }
}
