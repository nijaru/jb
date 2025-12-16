use jb_core::{Database, Paths, Status};
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::Instant;
use tokio::process::Child;
use tokio::sync::oneshot;

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

        Ok(Self {
            db: Mutex::new(db),
            paths: paths.clone(),
            started_at: Instant::now(),
            running_jobs: Mutex::new(HashMap::new()),
        })
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

    pub fn get_job(&self, id: &str) -> anyhow::Result<Option<jb_core::Job>> {
        self.db.lock().unwrap().get(id)
    }

    pub fn list_jobs(
        &self,
        status: Option<Status>,
        project: Option<&std::path::PathBuf>,
    ) -> anyhow::Result<Vec<jb_core::Job>> {
        self.db.lock().unwrap().list(status, project)
    }
}
