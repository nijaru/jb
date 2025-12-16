use crate::state::{DaemonState, RunningJob};
use jb_core::ipc::Response;
use jb_core::{Job, Status};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::process::Command;
use tokio::sync::oneshot;
use tracing::{error, info};

pub async fn spawn_job(
    state: &Arc<DaemonState>,
    command: String,
    name: Option<String>,
    cwd: String,
    project: String,
    timeout_secs: Option<u64>,
    context: Option<serde_json::Value>,
    idempotency_key: Option<String>,
) -> Response {
    // Check idempotency key
    if let Some(ref key) = idempotency_key {
        let db = state.db.lock().unwrap();
        if let Ok(Some(existing)) = db.get_by_idempotency_key(key) {
            return Response::Job(existing);
        }
    }

    // Create job record
    let mut job = Job::new(command.clone(), PathBuf::from(&cwd), PathBuf::from(&project));

    if let Some(n) = name {
        job = job.with_name(n);
    }
    if let Some(t) = timeout_secs {
        job = job.with_timeout(t);
    }
    if let Some(c) = context {
        job = job.with_context(c);
    }
    if let Some(k) = idempotency_key {
        job = job.with_idempotency_key(k);
    }

    // Insert into DB
    {
        let db = state.db.lock().unwrap();
        if let Err(e) = db.insert(&job) {
            return Response::Error(format!("Failed to create job: {}", e));
        }
    }

    let job_id = job.id.clone();
    let state_clone = state.clone();

    // Spawn the process
    tokio::spawn(async move {
        if let Err(e) = run_job(&state_clone, job_id.clone(), command, cwd, timeout_secs).await {
            error!("Job {} failed to spawn: {}", job_id, e);
        }
    });

    // Return the job (still pending, will update to running shortly)
    Response::Job(job)
}

async fn run_job(
    state: &Arc<DaemonState>,
    job_id: String,
    command: String,
    cwd: String,
    timeout_secs: Option<u64>,
) -> anyhow::Result<()> {
    let log_path = state.paths.log_file(&job_id);

    // Create log file
    let log_file = File::create(&log_path).await?;
    let log_file_std = log_file.into_std().await;

    // Spawn process in new session (detached)
    let child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .stdout(Stdio::from(log_file_std.try_clone()?))
        .stderr(Stdio::from(log_file_std))
        .process_group(0) // Create new process group (setsid equivalent)
        .spawn()?;

    let pid = child.id().unwrap_or(0);

    // Update DB with running status
    {
        let db = state.db.lock().unwrap();
        db.update_started(&job_id, pid)?;
    }

    info!("Job {} started with PID {}", job_id, pid);

    // Create channel for completion notification
    let (tx, rx) = oneshot::channel();

    // Track running job and spawn monitor task
    {
        let mut running = state.running_jobs.lock().unwrap();
        running.insert(
            job_id.clone(),
            RunningJob {
                child,
                completion_tx: Some(tx),
            },
        );
    }

    // Spawn a task to wait for the process
    let state_clone = state.clone();
    let job_id_clone = job_id.clone();
    tokio::spawn(async move {
        monitor_job(&state_clone, &job_id_clone, timeout_secs).await;
    });

    // Wait for completion signal
    let _ = rx.await;

    Ok(())
}

async fn monitor_job(state: &Arc<DaemonState>, job_id: &str, timeout_secs: Option<u64>) {
    let start = std::time::Instant::now();
    let timeout = timeout_secs.map(Duration::from_secs);

    // Poll the process status
    let result = loop {
        // Check if process is done
        let status = {
            let mut running = state.running_jobs.lock().unwrap();
            if let Some(job) = running.get_mut(job_id) {
                match job.child.try_wait() {
                    Ok(Some(status)) => Some(Ok(status)),
                    Ok(None) => None,
                    Err(e) => Some(Err(e)),
                }
            } else {
                break None;
            }
        };

        if let Some(result) = status {
            break Some(result);
        }

        // Check timeout
        if let Some(t) = timeout {
            if start.elapsed() >= t {
                // Kill the process on timeout
                let mut running = state.running_jobs.lock().unwrap();
                if let Some(job) = running.get_mut(job_id) {
                    let _ = job.child.start_kill();
                }
                break None;
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    };

    // Remove from running jobs and notify completion
    let completion_tx = {
        let mut running = state.running_jobs.lock().unwrap();
        running.remove(job_id).and_then(|j| j.completion_tx)
    };

    // Update DB with final status
    let (status, exit_code) = match result {
        Some(Ok(exit_status)) => {
            if exit_status.success() {
                (Status::Completed, exit_status.code())
            } else {
                (Status::Failed, exit_status.code())
            }
        }
        Some(Err(_)) => (Status::Failed, None),
        None => (Status::Stopped, None), // Timeout or killed
    };

    {
        let db = state.db.lock().unwrap();
        let _ = db.update_finished(job_id, status, exit_code);
    }

    info!("Job {} finished with status {:?}", job_id, status);

    // Signal completion
    if let Some(tx) = completion_tx {
        let _ = tx.send(());
    }
}

pub async fn stop_job(state: &Arc<DaemonState>, job_id: &str, force: bool) -> Response {
    let result = {
        let mut running = state.running_jobs.lock().unwrap();
        if let Some(job) = running.get_mut(job_id) {
            if force {
                job.child.start_kill()
            } else {
                job.child.start_kill() // tokio doesn't have SIGTERM, use kill
            }
        } else {
            return Response::Error(format!("Job {} is not running", job_id));
        }
    };

    match result {
        Ok(_) => {
            // Update DB
            let db = state.db.lock().unwrap();
            let _ = db.update_finished(job_id, Status::Stopped, None);
            Response::Ok
        }
        Err(e) => Response::Error(format!("Failed to stop job: {}", e)),
    }
}

pub async fn wait_for_job(
    state: &Arc<DaemonState>,
    job_id: &str,
    timeout_secs: Option<u64>,
) -> Response {
    let start = std::time::Instant::now();
    let timeout = timeout_secs.map(Duration::from_secs);

    loop {
        // Check if job exists and its status
        match state.get_job(job_id) {
            Ok(Some(job)) => {
                if job.status.is_terminal() {
                    return Response::Job(job);
                }
            }
            Ok(None) => return Response::Error(format!("Job not found: {}", job_id)),
            Err(e) => return Response::Error(e.to_string()),
        }

        // Check timeout
        if let Some(t) = timeout {
            if start.elapsed() >= t {
                return Response::Error("Wait timed out".to_string());
            }
        }

        // Poll interval
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
