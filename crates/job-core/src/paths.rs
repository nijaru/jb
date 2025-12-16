use std::path::PathBuf;

pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new() -> Self {
        let root = dirs::home_dir()
            .expect("could not determine home directory")
            .join(".job");
        Self { root }
    }

    pub fn root(&self) -> &PathBuf {
        &self.root
    }

    pub fn database(&self) -> PathBuf {
        self.root.join("job.db")
    }

    pub fn logs_dir(&self) -> PathBuf {
        self.root.join("logs")
    }

    pub fn log_file(&self, job_id: &str) -> PathBuf {
        self.logs_dir().join(format!("{}.log", job_id))
    }

    pub fn socket(&self) -> PathBuf {
        self.root.join("daemon.sock")
    }

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
