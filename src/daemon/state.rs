use crate::core::{Database, Job, Paths, Status};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tokio::process::Child;
use tokio::sync::oneshot;
use tracing::warn;

pub struct RunningJob {
    pub child: Child,
    pub completion_tx: Option<oneshot::Sender<()>>,
}

pub struct DaemonState {
    pub db: Mutex<Database>,
    pub paths: Paths,
    pub started_at: Instant,
    pub running_jobs: Mutex<HashMap<String, RunningJob>>,
}

impl DaemonState {
    pub fn new(paths: &Paths) -> anyhow::Result<Self> {
        let db = Database::open(paths)?;

        // Recover orphaned jobs from previous daemon crash
        Self::recover_orphaned_jobs(&db);

        Ok(Self {
            db: Mutex::new(db),
            paths: paths.clone(),
            started_at: Instant::now(),
            running_jobs: Mutex::new(HashMap::new()),
        })
    }

    /// Mark any jobs stuck in "running" or "pending" state as interrupted.
    /// These are orphans from a previous daemon that crashed.
    fn recover_orphaned_jobs(db: &Database) {
        let orphaned = db
            .list(Some(Status::Running), None)
            .unwrap_or_default()
            .into_iter()
            .chain(db.list(Some(Status::Pending), None).unwrap_or_default());

        for job in orphaned {
            warn!(
                "Recovering orphaned job {} (was {})",
                job.short_id(),
                job.status
            );
            let _ = db.update_finished(&job.id, Status::Interrupted, None);
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn running_count(&self) -> usize {
        self.running_jobs.lock().unwrap().len()
    }

    pub fn total_jobs(&self) -> usize {
        self.db
            .lock()
            .unwrap()
            .list(None, None)
            .map(|j| j.len())
            .unwrap_or(0)
    }

    pub fn get_job(&self, id: &str) -> anyhow::Result<Option<Job>> {
        self.db.lock().unwrap().get(id)
    }

    pub fn list_jobs(
        &self,
        status: Option<Status>,
        project: Option<&std::path::PathBuf>,
    ) -> anyhow::Result<Vec<Job>> {
        self.db.lock().unwrap().list(status, project)
    }
}
