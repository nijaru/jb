mod server;
mod spawner;
mod state;

use anyhow::Result;
use jb_core::Paths;
use std::sync::Arc;
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

    let state = Arc::new(state::DaemonState::new(&paths)?);

    // Write PID file
    std::fs::write(paths.pid_file(), std::process::id().to_string())?;

    // Clean up stale socket
    if paths.socket().exists() {
        std::fs::remove_file(paths.socket())?;
    }

    // Run the server
    let result = server::run(paths, state.clone()).await;

    // Cleanup
    let _ = std::fs::remove_file(Paths::new().pid_file());
    let _ = std::fs::remove_file(Paths::new().socket());

    result
}
