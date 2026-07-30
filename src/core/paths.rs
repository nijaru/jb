use std::path::PathBuf;

#[derive(Clone)]
pub struct Paths {
    root: PathBuf,
}

impl Paths {
    pub fn new() -> anyhow::Result<Self> {
        let root = dirs::home_dir()
            .ok_or_else(|| anyhow::anyhow!("could not determine home directory"))?
            .join(".jb");
        Ok(Self { root })
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

    #[must_use]
    pub fn lock_file(&self) -> PathBuf {
        self.root.join("daemon.lock")
    }

    pub fn ensure_dirs(&self) -> anyhow::Result<()> {
        std::fs::create_dir_all(&self.root)?;
        set_mode(&self.root, 0o700)?;
        let logs = self.logs_dir();
        std::fs::create_dir_all(&logs)?;
        set_mode(&logs, 0o700)?;
        Ok(())
    }

    pub fn secure_file(&self, path: &std::path::Path) -> anyhow::Result<()> {
        set_mode(path, 0o600)
    }
}

#[cfg(unix)]
fn set_mode(path: &std::path::Path, mode: u32) -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let mut permissions = std::fs::metadata(path)?.permissions();
    permissions.set_mode(mode);
    std::fs::set_permissions(path, permissions)?;
    Ok(())
}

#[cfg(not(unix))]
fn set_mode(_path: &std::path::Path, _mode: u32) -> anyhow::Result<()> {
    Ok(())
}
