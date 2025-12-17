pub mod server;
pub mod spawner;
pub mod state;

use crate::core::Paths;
use anyhow::{Result, bail};
use std::sync::Arc;
use tracing::info;

pub async fn run() -> Result<()> {
    let paths = Paths::new();
    paths.ensure_dirs()?;

    // Check if another daemon is running
    if let Some(existing_pid) = check_existing_daemon(&paths) {
        bail!("Daemon already running with PID {existing_pid}");
    }

    info!("Starting job daemon");
    info!("Socket: {}", paths.socket().display());
    info!("Database: {}", paths.database().display());

    // Write PID file
    std::fs::write(paths.pid_file(), std::process::id().to_string())?;

    // Clean up stale socket
    if paths.socket().exists() {
        std::fs::remove_file(paths.socket())?;
    }

    let state = Arc::new(state::DaemonState::new(&paths)?);

    // Run the server
    let result = server::run(paths.clone(), state.clone()).await;

    // Cleanup
    let _ = std::fs::remove_file(paths.pid_file());
    let _ = std::fs::remove_file(paths.socket());

    result
}

/// Check if an existing daemon is running. Returns the PID if so.
fn check_existing_daemon(paths: &Paths) -> Option<u32> {
    let pid_file = paths.pid_file();
    if !pid_file.exists() {
        return None;
    }

    let pid_str = std::fs::read_to_string(&pid_file).ok()?;
    let pid: u32 = pid_str.trim().parse().ok()?;

    // Check if process is running
    if is_process_running(pid) {
        Some(pid)
    } else {
        // Stale PID file, clean it up
        let _ = std::fs::remove_file(&pid_file);
        None
    }
}

#[cfg(unix)]
fn is_process_running(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;
    // Signal 0 (None) doesn't send a signal but checks if process exists
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
fn is_process_running(_pid: u32) -> bool {
    // On non-Unix, assume not running (conservative)
    false
}
