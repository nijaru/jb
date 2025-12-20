use std::path::PathBuf;

#[derive(Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    #[must_use]
    pub fn new() -> Self {
        let root = dirs::home_dir()
            .expect("could not determine home directory")
            .join(".jb");
        Self { root }
    }

    /// Create Paths with a custom root directory (useful for testing)
    #[cfg(test)]
    #[must_use]
    pub fn with_root(root: PathBuf) -> Self {
        Self { root }
    }

    #[must_use]
    pub fn database(&self) -> PathBuf {
        self.root.join("job.db")
    }

    #[must_use]
    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    #[must_use]
    pub fn log_file(&self, job_id: &str) -> PathBuf {
        self.logs_dir().join(format!("{job_id}.log"))
    }

    #[must_use]
    pub fn socket(&self) -> PathBuf {
        self.root.join("daemon.sock")
    }

    #[must_use]
    pub fn pid_file(&self) -> PathBuf {
        self.root.join("daemon.pid")
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        std::fs::create_dir_all(self.logs_dir())?;
        Ok(())
    }
}

impl Default for Paths {
    fn default() -> Self {
        Self::new()
    }
}
