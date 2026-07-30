use crate::core::{Database, Job, Paths, Status};
use std::collections::HashMap;
use std::sync::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Instant;
use tokio::sync::{mpsc, oneshot};
use tokio::task::JoinHandle;

pub(crate) enum JobCommand {
    Stop {
        force: bool,
        reply: oneshot::Sender<StopReply>,
    },
    Shutdown,
}

#[derive(Debug)]
pub(crate) enum StopReply {
    Stopped,
}

pub(crate) struct JobControl {
    pub command_tx: mpsc::Sender<JobCommand>,
    pub shutdown_tx: tokio::sync::watch::Sender<bool>,
    pub start_tx: Option<oneshot::Sender<()>>,
    pub join: Option<JoinHandle<()>>,
}

pub struct DaemonState {
    pub db: Mutex<Database>,
    pub paths: Paths,
    pub started_at: Instant,
    admission: Mutex<()>,
    jobs: Mutex<HashMap<String, JobControl>>,
    accepting: AtomicBool,
    finished_tx: mpsc::Sender<String>,
    finished_rx: Mutex<Option<mpsc::Receiver<String>>>,
}

impl DaemonState {
    pub fn new(paths: &Paths) -> anyhow::Result<Self> {
        let db = Database::open(paths)?;
        db.recover_orphans()?;
        let (finished_tx, finished_rx) = mpsc::channel(1024);

        Ok(Self {
            db: Mutex::new(db),
            paths: paths.clone(),
            started_at: Instant::now(),
            admission: Mutex::new(()),
            jobs: Mutex::new(HashMap::new()),
            accepting: AtomicBool::new(true),
            finished_tx,
            finished_rx: Mutex::new(Some(finished_rx)),
        })
    }

    pub(crate) fn finished_sender(&self) -> mpsc::Sender<String> {
        self.finished_tx.clone()
    }

    pub(crate) fn take_finished_receiver(&self) -> mpsc::Receiver<String> {
        self.finished_rx
            .lock()
            .expect("finished receiver lock poisoned")
            .take()
            .expect("finished receiver already taken")
    }

    pub(crate) fn admission_guard(&self) -> std::sync::MutexGuard<'_, ()> {
        self.admission.lock().expect("admission lock poisoned")
    }

    pub(crate) fn is_accepting(&self) -> bool {
        self.accepting.load(Ordering::Acquire)
    }

    pub(crate) fn register_job(&self, id: String, control: JobControl) -> bool {
        if !self.is_accepting() {
            return false;
        }

        let mut jobs = self.jobs.lock().expect("job registry lock poisoned");
        jobs.insert(id, control).is_none()
    }

    pub(crate) fn start_job(&self, id: &str) -> bool {
        let mut jobs = self.jobs.lock().expect("job registry lock poisoned");
        let Some(control) = jobs.get_mut(id) else {
            return false;
        };
        control
            .start_tx
            .take()
            .is_some_and(|sender| sender.send(()).is_ok())
    }

    pub(crate) fn command_sender(&self, id: &str) -> Option<mpsc::Sender<JobCommand>> {
        self.jobs
            .lock()
            .expect("job registry lock poisoned")
            .get(id)
            .map(|control| control.command_tx.clone())
    }

    pub(crate) fn remove_job(&self, id: &str) -> Option<JobControl> {
        self.jobs
            .lock()
            .expect("job registry lock poisoned")
            .remove(id)
    }

    pub(crate) fn active_job_count(&self) -> usize {
        self.jobs.lock().expect("job registry lock poisoned").len()
    }

    pub(crate) fn finished_job_ids(&self) -> Vec<String> {
        self.jobs
            .lock()
            .expect("job registry lock poisoned")
            .iter()
            .filter_map(|(id, control)| {
                control
                    .join
                    .as_ref()
                    .filter(|join| join.is_finished())
                    .map(|_| id.clone())
            })
            .collect()
    }

    pub(crate) fn begin_shutdown(&self) {
        let _admission = self.admission_guard();
        if !self.accepting.swap(false, Ordering::AcqRel) {
            return;
        }

        let jobs = self.jobs.lock().expect("job registry lock poisoned");
        for control in jobs.values() {
            let _ = control.shutdown_tx.send(true);
        }
    }

    pub fn uptime_secs(&self) -> u64 {
        self.started_at.elapsed().as_secs()
    }

    pub fn running_count(&self) -> anyhow::Result<usize> {
        self.db
            .lock()
            .expect("database lock poisoned")
            .count(Some(Status::Running))
    }

    pub fn total_jobs(&self) -> anyhow::Result<usize> {
        self.db.lock().expect("database lock poisoned").count(None)
    }

    pub fn get_job(&self, id: &str) -> anyhow::Result<Option<Job>> {
        self.db.lock().expect("database lock poisoned").get(id)
    }

    pub fn list_jobs(
        &self,
        status: Option<Status>,
        limit: Option<usize>,
    ) -> anyhow::Result<Vec<Job>> {
        self.db
            .lock()
            .expect("database lock poisoned")
            .list(status, limit)
    }
}
