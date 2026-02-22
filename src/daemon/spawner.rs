use crate::core::ipc::Response;
use crate::core::{Job, Status, kill_process_group};
use crate::daemon::state::{DaemonState, RunningJob};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::process::Command;
use tokio::sync::watch;
use tracing::{error, info, warn};

#[allow(clippy::too_many_arguments)]
pub fn spawn_job(
    state: &Arc<DaemonState>,
    command: String,
    name: Option<String>,
    cwd: String,
    project: String,
    timeout_secs: Option<u64>,
    context: Option<serde_json::Value>,
    idempotency_key: Option<String>,
) -> Response {
    // Check idempotency key and name uniqueness, generate ID
    let id = {
        let db = state.db.lock().unwrap();

        // Idempotency check
        if let Some(ref key) = idempotency_key
            && let Ok(Some(existing)) = db.get_by_idempotency_key(key)
        {
            return Response::Job(Box::new(existing));
        }

        // Name uniqueness check: can't have two running jobs with same name
        if let Some(ref n) = name
            && let Ok(Some(running)) = db.name_in_use(n)
        {
            return Response::UserError(format!(
                "Name '{}' is in use by running job {}",
                n,
                running.short_id()
            ));
        }

        match db.generate_id() {
            Ok(id) => id,
            Err(e) => return Response::Error(e.to_string()),
        }
    };

    // Create job record
    let mut job = Job::new(
        id,
        command.clone(),
        PathBuf::from(&cwd),
        PathBuf::from(&project),
    );

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
            return Response::Error(format!("Failed to create job: {e}"));
        }
    }

    let job_id = job.id.clone();
    let state_clone = state.clone();

    // Spawn the process; on failure mark the job as failed in the DB
    tokio::spawn(async move {
        if let Err(e) = run_job(&state_clone, job_id.clone(), command, cwd, timeout_secs).await {
            error!("Job {} failed: {}", job_id, e);
            let db = state_clone.db.lock().unwrap();
            if let Err(db_err) = db.update_finished(&job_id, Status::Failed, None) {
                error!("Failed to mark job {} as failed: {}", job_id, db_err);
            }
        }
    });

    // Return the job (still pending, will update to running shortly)
    Response::Job(Box::new(job))
}

/// Time to wait for graceful shutdown before SIGKILL
const GRACEFUL_SHUTDOWN_SECS: u64 = 2;

#[allow(clippy::too_many_lines)]
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
    let mut child = Command::new("sh")
        .arg("-c")
        .arg(&command)
        .current_dir(&cwd)
        .stdout(Stdio::from(log_file_std.try_clone()?))
        .stderr(Stdio::from(log_file_std))
        .process_group(0) // Create new process group (setsid equivalent)
        .spawn()?;

    let pid = child
        .id()
        .ok_or_else(|| anyhow::anyhow!("failed to get PID of spawned process"))?;

    // Update DB with running status
    {
        let db = state.db.lock().unwrap();
        db.update_started(&job_id, pid)?;
    }

    info!("Job {} started with PID {}", job_id, pid);

    let (stop_tx, mut stop_rx) = watch::channel(false);

    // Track running job
    {
        let mut running = state.running_jobs.lock().unwrap();
        running.insert(job_id.clone(), RunningJob { pid, stop_tx });
    }

    // Event-based monitoring with tokio::select!
    // Note: We use changed() instead of wait_for() because wait_for() returns
    // a non-Send guard that causes issues with tokio::spawn
    let result = if let Some(timeout) = timeout_secs {
        tokio::select! {
            biased;

            // Stop signal from stop_job or interrupt_running_jobs
            // (changed() returns when value is updated; we only send true)
            _ = stop_rx.changed() => {
                JobResult::Stopped
            }

            // Timeout expired - escalate: SIGTERM → wait → SIGKILL
            () = tokio::time::sleep(Duration::from_secs(timeout)) => {
                warn!("Job {} timed out after {}s, sending SIGTERM", job_id, timeout);
                kill_process_group(pid, false); // SIGTERM first

                // Give process time to exit gracefully
                tokio::select! {
                    biased;
                    _ = stop_rx.changed() => JobResult::Stopped,
                    status = child.wait() => JobResult::Completed(status.ok()),
                    () = tokio::time::sleep(Duration::from_secs(GRACEFUL_SHUTDOWN_SECS)) => {
                        warn!("Job {} did not exit after SIGTERM, sending SIGKILL", job_id);
                        kill_process_group(pid, true); // Force kill
                        JobResult::Timeout
                    }
                }
            }

            // Process exited normally
            status = child.wait() => {
                JobResult::Completed(status.ok())
            }
        }
    } else {
        // No timeout - just wait for exit or stop signal
        tokio::select! {
            biased;

            _ = stop_rx.changed() => {
                JobResult::Stopped
            }

            status = child.wait() => {
                JobResult::Completed(status.ok())
            }
        }
    };

    // Remove from running jobs
    {
        let mut running = state.running_jobs.lock().unwrap();
        running.remove(&job_id);
    }

    // Handle result
    match result {
        JobResult::Stopped => {
            // stop_job already updated DB
        }
        JobResult::Timeout => {
            let db = state.db.lock().unwrap();
            if let Err(e) = db.update_finished(&job_id, Status::Stopped, None) {
                error!("Failed to update job {} status after timeout: {}", job_id, e);
            }
            info!("Job {} timed out", job_id);
        }
        JobResult::Completed(exit_status) => {
            let (status, exit_code) = match exit_status {
                Some(es) if es.success() => (Status::Completed, es.code()),
                Some(es) => (Status::Failed, es.code()),
                None => (Status::Failed, None),
            };

            {
                let db = state.db.lock().unwrap();
                if let Err(e) = db.update_finished(&job_id, status, exit_code) {
                    error!(
                        "Failed to update job {} status after completion: {}",
                        job_id, e
                    );
                }
            }
            info!("Job {} finished with status {:?}", job_id, status);
        }
    }

    Ok(())
}

enum JobResult {
    Completed(Option<std::process::ExitStatus>),
    Stopped,
    Timeout,
}

pub fn stop_job(state: &Arc<DaemonState>, job_id: &str, force: bool) -> Response {
    // Get job info and signal stop
    let job = {
        let running = state.running_jobs.lock().unwrap();
        running.get(job_id).map(|j| (j.pid, j.stop_tx.clone()))
    };

    let Some((pid, stop_tx)) = job else {
        return Response::Error(format!("Job {job_id} is not running"));
    };

    // Signal the run_job task to stop (will break out of select!)
    let _ = stop_tx.send(true);

    // Kill the entire process group (not just the shell wrapper)
    kill_process_group(pid, force);

    {
        let db = state.db.lock().unwrap();
        if let Err(e) = db.update_finished(job_id, Status::Stopped, None) {
            error!("Failed to update job {} status after stop: {}", job_id, e);
        }
    }

    info!("Job {} stopped", job_id);

    Response::Ok
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
                    return Response::Job(Box::new(job));
                }
            }
            Ok(None) => return Response::Error(format!("Job not found: {job_id}")),
            Err(e) => return Response::Error(e.to_string()),
        }

        // Check timeout
        if let Some(t) = timeout
            && start.elapsed() >= t
        {
            return Response::Error("Wait timed out".to_string());
        }

        // Poll interval
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}
