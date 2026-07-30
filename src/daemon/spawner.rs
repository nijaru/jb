use crate::core::ipc::Response;
use crate::core::{Job, Status, kill_process_group, process_group_exists};
use crate::daemon::state::{DaemonState, JobCommand, JobControl, StopReply};
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;
use std::time::Duration;
use tokio::fs::File;
use tokio::process::{Child, Command};
use tokio::sync::{mpsc, oneshot};
use tracing::{error, info, warn};

const GRACEFUL_SHUTDOWN_SECS: u64 = 2;
const GROUP_CHECK_INTERVAL_MS: u64 = 25;
const GROUP_CHECK_ATTEMPTS: usize = 80;

pub fn spawn_job(
    state: &Arc<DaemonState>,
    command: String,
    name: Option<String>,
    cwd: String,
    project: String,
    timeout_secs: Option<u64>,
    idempotency_key: Option<String>,
) -> Response {
    let _admission = state.admission_guard();
    if !state.is_accepting() {
        return Response::UserError("daemon is shutting down".to_string());
    }

    let job = {
        let db = state.db.lock().expect("database lock poisoned");

        if let Some(key) = idempotency_key.as_deref() {
            match db.get_by_idempotency_key(key) {
                Ok(Some(existing)) => return Response::Job(Box::new(existing)),
                Ok(None) => {}
                Err(error) => return Response::Error(error.to_string()),
            }
        }

        if let Some(job_name) = name.as_deref() {
            match db.name_in_use(job_name) {
                Ok(Some(active)) => {
                    return Response::UserError(format!(
                        "Name '{}' is in use by running job {}",
                        job_name,
                        active.short_id()
                    ));
                }
                Ok(None) => {}
                Err(error) => return Response::Error(error.to_string()),
            }
        }

        let id = match db.generate_id() {
            Ok(id) => id,
            Err(error) => return Response::Error(error.to_string()),
        };

        let mut job = Job::new(
            id,
            command.clone(),
            PathBuf::from(&cwd),
            PathBuf::from(&project),
        );
        if let Some(job_name) = name {
            job = job.with_name(job_name);
        }
        if let Some(timeout) = timeout_secs {
            job = job.with_timeout(timeout);
        }
        if let Some(key) = idempotency_key {
            job = job.with_idempotency_key(key);
        }

        if let Err(error) = db.insert(&job) {
            return Response::Error(format!("Failed to create job: {error}"));
        }
        job
    };

    let job_id = job.id.clone();
    let (command_tx, command_rx) = mpsc::channel(8);
    let (shutdown_tx, shutdown_rx) = tokio::sync::watch::channel(false);
    let (start_tx, start_rx) = oneshot::channel();
    let task_state = Arc::clone(state);
    let finished_tx = state.finished_sender();
    let task = tokio::spawn(async move {
        if start_rx.await.is_err() {
            return;
        }
        JobTask {
            state: task_state,
            job_id: job_id.clone(),
            command,
            cwd,
            timeout_secs,
            command_rx,
            shutdown_rx,
            finished_tx,
        }
        .run()
        .await;
    });

    let registered = state.register_job(
        job.id.clone(),
        JobControl {
            command_tx,
            shutdown_tx,
            start_tx: Some(start_tx),
            join: Some(task),
        },
    );
    if !registered || !state.start_job(&job.id) {
        return Response::Error("daemon stopped accepting jobs".to_string());
    }

    Response::Job(Box::new(job))
}

struct JobTask {
    state: Arc<DaemonState>,
    job_id: String,
    command: String,
    cwd: String,
    timeout_secs: Option<u64>,
    command_rx: mpsc::Receiver<JobCommand>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
    finished_tx: mpsc::Sender<String>,
}

impl JobTask {
    async fn run(self) {
        let Self {
            state,
            job_id,
            command,
            cwd,
            timeout_secs,
            command_rx,
            shutdown_rx,
            finished_tx,
        } = self;
        let result = run_job_inner(
            &state,
            &job_id,
            &command,
            &cwd,
            timeout_secs,
            command_rx,
            shutdown_rx,
        )
        .await;

        let result = match result {
            Ok(result) => result,
            Err(error) => {
                error!("Job {} failed: {}", job_id, error);
                JobResult {
                    status: Status::Failed,
                    exit_code: None,
                    stop_replies: Vec::new(),
                }
            }
        };
        publish_terminal(&state, &job_id, result).await;

        if finished_tx.send(job_id).await.is_err() {
            error!("Daemon stopped receiving job completion events");
        }
    }
}

async fn run_job_inner(
    state: &Arc<DaemonState>,
    job_id: &str,
    command: &str,
    cwd: &str,
    timeout_secs: Option<u64>,
    mut command_rx: mpsc::Receiver<JobCommand>,
    shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> anyhow::Result<JobResult> {
    let log_path = state.paths.log_file(job_id);
    let log_file = File::create(&log_path).await?;
    state.paths.secure_file(&log_path)?;
    let log_file_std = log_file.into_std().await;

    // A stop or shutdown can arrive while a pending job is waiting for its
    // task to run. Do not create a child after shutdown has started.
    if *shutdown_rx.borrow() {
        return Ok(finish_without_child(JobCommand::Shutdown));
    }
    if let Ok(command) = command_rx.try_recv() {
        return Ok(finish_without_child(command));
    }

    let mut child = Command::new("sh")
        .arg("-c")
        .arg(command)
        .current_dir(cwd)
        .stdout(Stdio::from(log_file_std.try_clone()?))
        .stderr(Stdio::from(log_file_std))
        .process_group(0)
        .spawn()?;

    let Some(pid) = child.id() else {
        let _ = child.kill().await;
        let _ = child.wait().await;
        anyhow::bail!("failed to get PID of spawned process");
    };

    let claim_result = {
        let db = state.db.lock().expect("database lock poisoned");
        db.mark_running(job_id, pid)
    };
    let claimed = match claim_result {
        Ok(claimed) => claimed,
        Err(error) => {
            terminate_child(
                &mut child,
                pid,
                Status::Interrupted,
                true,
                &mut command_rx,
                Vec::new(),
            )
            .await;
            return Err(error);
        }
    };
    if !claimed {
        terminate_child(
            &mut child,
            pid,
            Status::Interrupted,
            true,
            &mut command_rx,
            Vec::new(),
        )
        .await;
        anyhow::bail!("job {job_id} was no longer pending when its process started");
    }

    info!("Job {} started with PID {}", job_id, pid);
    let result = monitor_child(&mut child, pid, timeout_secs, command_rx, shutdown_rx).await;

    Ok(result)
}

fn finish_without_child(command: JobCommand) -> JobResult {
    let (status, stop_replies) = match command {
        JobCommand::Stop { reply, .. } => (Status::Stopped, vec![reply]),
        JobCommand::Shutdown => (Status::Interrupted, Vec::new()),
    };
    JobResult {
        status,
        exit_code: None,
        stop_replies,
    }
}

struct JobResult {
    status: Status,
    exit_code: Option<i32>,
    stop_replies: Vec<oneshot::Sender<StopReply>>,
}

/// Keep the job task alive until SQLite confirms the terminal state. This is
/// deliberately retry-based: a busy database must not turn a stopped child
/// into a failed row or drop a waiting stop request.
async fn publish_terminal(state: &Arc<DaemonState>, job_id: &str, result: JobResult) {
    let JobResult {
        status,
        exit_code,
        stop_replies,
    } = result;

    loop {
        let finish_result = {
            let db = state.db.lock().expect("database lock poisoned");
            db.finish(job_id, status, exit_code)
        };

        match finish_result {
            Ok(true) => {
                for reply in stop_replies {
                    let _ = reply.send(StopReply::Stopped);
                }
                info!("Job {} finished with status {}", job_id, status);
                return;
            }
            Ok(false) => {
                let current = state.db.lock().expect("database lock poisoned").get(job_id);
                if matches!(current, Ok(Some(ref job)) if job.status.is_terminal()) {
                    warn!(
                        "Job {} was already terminal while publishing {}",
                        job_id, status
                    );
                    for reply in stop_replies {
                        let _ = reply.send(StopReply::Stopped);
                    }
                    return;
                }
                error!("Terminal transition for job {} was rejected", job_id);
            }
            Err(error) => {
                error!(
                    "Failed to publish terminal state for job {}: {}",
                    job_id, error
                );
            }
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

async fn monitor_child(
    child: &mut Child,
    pid: u32,
    timeout_secs: Option<u64>,
    mut command_rx: mpsc::Receiver<JobCommand>,
    mut shutdown_rx: tokio::sync::watch::Receiver<bool>,
) -> JobResult {
    if let Some(timeout) = timeout_secs {
        let timer = tokio::time::sleep(Duration::from_secs(timeout));
        tokio::pin!(timer);
        tokio::select! {
            biased;
            command = command_rx.recv() => {
                handle_command(command, child, pid, Status::Stopped, &mut command_rx).await
            }
            _ = shutdown_rx.changed() => {
                handle_command(Some(JobCommand::Shutdown), child, pid, Status::Interrupted, &mut command_rx).await
            }
            status = child.wait() => natural_result(status, pid).await,
            () = &mut timer => {
                warn!("Job {} timed out after {}s", pid, timeout);
                terminate_child(
                    child,
                    pid,
                    Status::Timeout,
                    false,
                    &mut command_rx,
                    Vec::new(),
                )
                .await
            }
        }
    } else {
        tokio::select! {
            biased;
            command = command_rx.recv() => {
                handle_command(command, child, pid, Status::Stopped, &mut command_rx).await
            }
            _ = shutdown_rx.changed() => {
                handle_command(Some(JobCommand::Shutdown), child, pid, Status::Interrupted, &mut command_rx).await
            }
            status = child.wait() => natural_result(status, pid).await,
        }
    }
}

async fn natural_result(result: std::io::Result<std::process::ExitStatus>, pid: u32) -> JobResult {
    cleanup_orphaned_group(pid).await;
    match result {
        Ok(status) if status.success() => JobResult {
            status: Status::Completed,
            exit_code: status.code(),
            stop_replies: Vec::new(),
        },
        Ok(status) => JobResult {
            status: Status::Failed,
            exit_code: status.code(),
            stop_replies: Vec::new(),
        },
        Err(error) => {
            error!("Failed waiting for child: {}", error);
            JobResult {
                status: Status::Failed,
                exit_code: None,
                stop_replies: Vec::new(),
            }
        }
    }
}

async fn handle_command(
    command: Option<JobCommand>,
    child: &mut Child,
    pid: u32,
    default_status: Status,
    command_rx: &mut mpsc::Receiver<JobCommand>,
) -> JobResult {
    let mut replies = Vec::new();
    let (status, force) = match command {
        Some(JobCommand::Stop { force, reply }) => {
            replies.push(reply);
            (default_status, force)
        }
        Some(JobCommand::Shutdown) => (Status::Interrupted, false),
        None => (Status::Interrupted, true),
    };

    terminate_child(child, pid, status, force, command_rx, replies).await
}

async fn terminate_child(
    child: &mut Child,
    pid: u32,
    status: Status,
    force: bool,
    command_rx: &mut mpsc::Receiver<JobCommand>,
    mut replies: Vec<oneshot::Sender<StopReply>>,
) -> JobResult {
    let mut force = force;
    let mut child_result = None;

    if !force {
        if let Err(error) = kill_process_group(pid, false) {
            warn!("Failed to send SIGTERM to job group {pid}: {error}");
        }

        let grace = tokio::time::sleep(Duration::from_secs(GRACEFUL_SHUTDOWN_SECS));
        tokio::pin!(grace);
        loop {
            tokio::select! {
                biased;
                command = command_rx.recv() => match command {
                    Some(JobCommand::Stop { force: true, reply }) => {
                        replies.push(reply);
                        force = true;
                        break;
                    }
                    Some(JobCommand::Stop { reply, .. }) => replies.push(reply),
                    Some(JobCommand::Shutdown) => {}
                    None => {
                        force = true;
                        break;
                    }
                },
                result = child.wait(), if child_result.is_none() => {
                    child_result = Some(result);
                    // A leader can exit on SIGTERM while descendants remain.
                    // Keep the grace period running so they receive SIGKILL if
                    // they ignore the graceful signal.
                    if !process_group_exists(pid).unwrap_or(true) {
                        break;
                    }
                }
                () = &mut grace => {
                    force = true;
                    break;
                }
            }
        }
    }

    if force && let Err(error) = kill_process_group(pid, true) {
        warn!("Failed to send SIGKILL to job group {pid}: {error}");
    }

    let result = match child_result {
        Some(result) => result,
        None => child.wait().await,
    };
    complete_termination(pid, status, result, replies).await
}

async fn complete_termination(
    pid: u32,
    status: Status,
    result: std::io::Result<std::process::ExitStatus>,
    replies: Vec<oneshot::Sender<StopReply>>,
) -> JobResult {
    terminate_group(pid, false).await;

    JobResult {
        status,
        exit_code: result.ok().and_then(|status| status.code()),
        stop_replies: replies,
    }
}

async fn cleanup_orphaned_group(pid: u32) {
    match process_group_exists(pid) {
        Ok(false) => return,
        Ok(true) => warn!("Child exited while process group {pid} still had members"),
        Err(error) => warn!("Could not inspect process group {pid}; continuing cleanup: {error}"),
    }
    terminate_group(pid, true).await;
}

async fn terminate_group(pid: u32, graceful: bool) {
    if graceful {
        match process_group_exists(pid) {
            Ok(false) => return,
            Ok(true) => {
                if let Err(error) = kill_process_group(pid, false) {
                    warn!("Failed to send SIGTERM to process group {pid}: {error}");
                }
                if wait_for_group_gone(pid).await.is_ok() {
                    return;
                }
            }
            Err(error) => warn!("Could not inspect process group {pid}: {error}"),
        }
    }

    // Once graceful termination has failed, keep ownership until the group is
    // actually gone. A transient signal/inspection error must not be reported
    // as a successful terminal job outcome.
    loop {
        match process_group_exists(pid) {
            Ok(false) => return,
            Ok(true) => {}
            Err(error) => {
                warn!("Could not inspect process group {pid}: {error}");
                tokio::time::sleep(Duration::from_millis(GROUP_CHECK_INTERVAL_MS)).await;
                continue;
            }
        }

        if let Err(error) = kill_process_group(pid, true) {
            warn!("Failed to send SIGKILL to process group {pid}: {error}");
        }
        if let Err(error) = wait_for_group_gone(pid).await {
            warn!("Process group {pid} still exists after SIGKILL: {error}");
        }
    }
}

async fn wait_for_group_gone(pid: u32) -> anyhow::Result<()> {
    for _ in 0..GROUP_CHECK_ATTEMPTS {
        if !process_group_exists(pid)? {
            return Ok(());
        }
        tokio::time::sleep(Duration::from_millis(GROUP_CHECK_INTERVAL_MS)).await;
    }
    anyhow::bail!("process group {pid} still exists after termination")
}

pub async fn stop_job(state: &Arc<DaemonState>, job_id: &str, force: bool) -> Response {
    let Some(command_tx) = state.command_sender(job_id) else {
        return match state.get_job(job_id) {
            Ok(Some(job)) if job.status.is_terminal() => {
                Response::UserError(format!("Job {} is already {}", job.short_id(), job.status))
            }
            Ok(Some(_)) => Response::Error(format!("Job {job_id} is not managed by this daemon")),
            Ok(None) => Response::Error(format!("Job not found: {job_id}")),
            Err(error) => Response::Error(error.to_string()),
        };
    };

    let (reply_tx, reply_rx) = oneshot::channel();
    if let Err(error) = command_tx
        .send(JobCommand::Stop {
            force,
            reply: reply_tx,
        })
        .await
    {
        return match state.get_job(job_id) {
            Ok(Some(job)) if job.status.is_terminal() => {
                Response::UserError(format!("Job {} is already {}", job.short_id(), job.status))
            }
            _ => Response::Error(format!("failed to request stop for {job_id}: {error}")),
        };
    }

    match reply_rx.await {
        Ok(StopReply::Stopped) => Response::Ok,
        Err(_) => Response::Error(format!("stop task for {job_id} exited unexpectedly")),
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
        match state.get_job(job_id) {
            Ok(Some(job)) => {
                if job.status.is_terminal() {
                    return Response::Job(Box::new(job));
                }
            }
            Ok(None) => return Response::Error(format!("Job not found: {job_id}")),
            Err(error) => return Response::Error(error.to_string()),
        }

        if let Some(timeout) = timeout
            && start.elapsed() >= timeout
        {
            return Response::WaitTimeout;
        }

        tokio::time::sleep(Duration::from_millis(100)).await;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Paths, Status};
    use crate::daemon::state::DaemonState;
    use std::sync::Arc;
    use std::time::Duration;
    use tempfile::TempDir;

    fn test_state(tmp: &TempDir) -> Arc<DaemonState> {
        let paths = Paths::with_root(tmp.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        Arc::new(DaemonState::new(&paths).unwrap())
    }

    fn do_spawn(state: &Arc<DaemonState>, cmd: &str, tmp: &TempDir) -> String {
        let cwd = tmp.path().to_string_lossy().to_string();
        match spawn_job(state, cmd.into(), None, cwd.clone(), cwd, None, None) {
            Response::Job(job) => job.id.clone(),
            other => panic!("expected Job response, got {other:?}"),
        }
    }

    async fn poll_terminal(state: &Arc<DaemonState>, id: &str) -> Status {
        for _ in 0..100 {
            if let Ok(Some(job)) = state.get_job(id)
                && job.status.is_terminal()
            {
                return job.status;
            }
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
        panic!("job {id} did not reach terminal state within 5s");
    }

    #[tokio::test]
    async fn test_spawn_returns_pending_job() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "echo hi", &tmp);

        let db = state.db.lock().unwrap();
        let job = db.get(&id).unwrap().unwrap();
        assert_eq!(job.status, Status::Pending);
        assert_eq!(job.command, "echo hi");
    }

    #[tokio::test]
    async fn test_job_completes_with_exit_0() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "true", &tmp);

        let status = poll_terminal(&state, &id).await;
        assert_eq!(status, Status::Completed);

        let db = state.db.lock().unwrap();
        let job = db.get(&id).unwrap().unwrap();
        assert_eq!(job.exit_code, Some(0));
        assert!(job.started_at.is_some());
        assert!(job.finished_at.is_some());
    }

    #[tokio::test]
    async fn test_job_fails_with_nonzero_exit() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "false", &tmp);

        let status = poll_terminal(&state, &id).await;
        assert_eq!(status, Status::Failed);

        let db = state.db.lock().unwrap();
        let job = db.get(&id).unwrap().unwrap();
        assert_eq!(job.exit_code, Some(1));
    }

    #[tokio::test]
    async fn test_job_output_written_to_log() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "echo hello_world_marker", &tmp);

        poll_terminal(&state, &id).await;

        let log_path = state.paths.log_file(&id);
        let content = std::fs::read_to_string(&log_path).unwrap();
        assert!(content.contains("hello_world_marker"), "log: {content:?}");
    }

    #[tokio::test]
    async fn test_job_pid_recorded() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "true", &tmp);

        poll_terminal(&state, &id).await;

        let db = state.db.lock().unwrap();
        let job = db.get(&id).unwrap().unwrap();
        assert!(job.pid.is_some(), "pid should be recorded");
        assert!(job.pid.unwrap() > 0);
    }

    #[tokio::test]
    async fn test_spawn_bad_cwd_marks_job_failed() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let cwd = tmp.path().to_string_lossy().to_string();

        let response = spawn_job(
            &state,
            "echo hi".into(),
            None,
            "/nonexistent/path/that/does/not/exist/ever".into(),
            cwd,
            None,
            None,
        );
        let id = match response {
            Response::Job(job) => job.id,
            other => panic!("expected Job, got {other:?}"),
        };

        let status = poll_terminal(&state, &id).await;
        assert_eq!(status, Status::Failed);
    }

    #[tokio::test]
    async fn test_spawn_idempotency_key_returns_existing_job() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let cwd = tmp.path().to_string_lossy().to_string();

        let response1 = spawn_job(
            &state,
            "echo 1".into(),
            None,
            cwd.clone(),
            cwd.clone(),
            None,
            Some("mykey".into()),
        );
        let response2 = spawn_job(
            &state,
            "echo 2".into(),
            None,
            cwd.clone(),
            cwd.clone(),
            None,
            Some("mykey".into()),
        );

        let id1 = match response1 {
            Response::Job(job) => job.id,
            _ => panic!("expected Job"),
        };
        let id2 = match response2 {
            Response::Job(job) => job.id,
            _ => panic!("expected Job"),
        };
        assert_eq!(id1, id2);
    }

    #[tokio::test]
    async fn test_spawn_duplicate_name_returns_user_error() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let cwd = tmp.path().to_string_lossy().to_string();

        let response1 = spawn_job(
            &state,
            "sleep 5".into(),
            Some("myjob".into()),
            cwd.clone(),
            cwd.clone(),
            None,
            None,
        );
        assert!(matches!(response1, Response::Job(_)));

        let response2 = spawn_job(
            &state,
            "echo hi".into(),
            Some("myjob".into()),
            cwd.clone(),
            cwd,
            None,
            None,
        );
        assert!(matches!(response2, Response::UserError(_)));
    }

    #[tokio::test]
    async fn test_stop_job_waits_for_term_ignoring_process() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "trap '' TERM; sleep 60", &tmp);

        for _ in 0..100 {
            if state.get_job(&id).unwrap().unwrap().status == Status::Running {
                break;
            }
            tokio::time::sleep(Duration::from_millis(10)).await;
        }

        let response = stop_job(&state, &id, false).await;
        assert!(matches!(response, Response::Ok));
        assert_eq!(poll_terminal(&state, &id).await, Status::Stopped);
    }

    #[tokio::test]
    async fn test_stop_pending_job_is_owned_by_daemon() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let id = do_spawn(&state, "sleep 60", &tmp);

        let response = stop_job(&state, &id, true).await;
        assert!(matches!(response, Response::Ok));
        assert_eq!(poll_terminal(&state, &id).await, Status::Stopped);
    }

    #[tokio::test]
    async fn test_timeout_marks_timeout() {
        let tmp = TempDir::new().unwrap();
        let state = test_state(&tmp);
        let cwd = tmp.path().to_string_lossy().to_string();
        let response = spawn_job(
            &state,
            "sleep 60".into(),
            None,
            cwd.clone(),
            cwd,
            Some(1),
            None,
        );
        let id = match response {
            Response::Job(job) => job.id,
            _ => panic!("expected Job"),
        };

        assert_eq!(poll_terminal(&state, &id).await, Status::Timeout);
    }
}
