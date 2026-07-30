pub mod server;
pub mod spawner;
pub mod state;

use crate::core::Paths;
use anyhow::{Result, bail};
use std::fs::{File, OpenOptions};
use std::sync::Arc;
use tracing::info;

pub async fn run() -> Result<()> {
    let paths = Paths::new()?;
    paths.ensure_dirs()?;
    let guard = DaemonGuard::acquire(&paths)?;

    // The lock is held before stale artifact cleanup and orphan recovery.
    if paths.socket().exists() {
        std::fs::remove_file(paths.socket())?;
    }

    info!("Starting job daemon");
    info!("Socket: {}", paths.socket().display());
    info!("Database: {}", paths.database().display());

    let state = Arc::new(state::DaemonState::new(&paths)?);
    std::fs::write(paths.pid_file(), std::process::id().to_string())?;
    paths.secure_file(&paths.pid_file())?;

    let result = server::run(paths.clone(), state).await;
    drop(guard);
    result
}

struct DaemonGuard {
    #[cfg(unix)]
    _lock: nix::fcntl::Flock<File>,
    #[cfg(not(unix))]
    _lock: File,
    paths: Paths,
    pid: u32,
}

impl DaemonGuard {
    fn acquire(paths: &Paths) -> Result<Self> {
        let file = OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(paths.lock_file())?;
        paths.secure_file(&paths.lock_file())?;

        #[cfg(unix)]
        let lock = {
            use nix::errno::Errno;
            use nix::fcntl::{Flock, FlockArg};

            match Flock::lock(file, FlockArg::LockExclusiveNonblock) {
                Ok(lock) => lock,
                Err((_file, Errno::EWOULDBLOCK)) => {
                    bail!("daemon is already running")
                }
                Err((_file, error)) => return Err(error.into()),
            }
        };

        #[cfg(not(unix))]
        let lock = file;

        Ok(Self {
            _lock: lock,
            paths: paths.clone(),
            pid: std::process::id(),
        })
    }
}

impl Drop for DaemonGuard {
    fn drop(&mut self) {
        if std::fs::read_to_string(self.paths.pid_file())
            .ok()
            .and_then(|pid| pid.trim().parse::<u32>().ok())
            == Some(self.pid)
        {
            let _ = std::fs::remove_file(self.paths.pid_file());
        }
        let _ = std::fs::remove_file(self.paths.socket());
    }
}
