use anyhow::Result;
use job_core::Paths;
use tracing::info;

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let paths = Paths::new();
    paths.ensure_dirs()?;

    info!("Starting job daemon");
    info!("Socket: {}", paths.socket().display());
    info!("Database: {}", paths.database().display());

    // TODO: Implement daemon
    // - Listen on Unix socket
    // - Handle IPC requests
    // - Spawn and monitor jobs
    // - Auto-clean old jobs on startup
    // - Recover orphaned jobs

    eprintln!("Daemon not yet implemented");

    Ok(())
}
