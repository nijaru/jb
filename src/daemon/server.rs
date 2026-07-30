use crate::core::ipc::{Request, Response};
use crate::core::{Paths, Status};
use crate::daemon::spawner;
use crate::daemon::state::DaemonState;
use anyhow::Result;
use std::sync::Arc;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::{Semaphore, watch};
use tokio::task::JoinSet;
use tracing::{error, info, warn};

const MAX_MESSAGE_SIZE: usize = 10 * 1024 * 1024;
const MAX_CONNECTIONS: usize = 64;
const IPC_OPERATION_TIMEOUT: Duration = Duration::from_secs(30);

pub async fn run(paths: Paths, state: Arc<DaemonState>) -> Result<()> {
    let listener = UnixListener::bind(paths.socket())?;
    paths.secure_file(&paths.socket())?;
    info!("Listening on {}", paths.socket().display());

    let (shutdown_tx, shutdown_rx) = watch::channel(false);
    let shutdown_signal_tx = shutdown_tx.clone();
    let shutdown_signal_state = Arc::clone(&state);
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        initiate_shutdown(&shutdown_signal_state, &shutdown_signal_tx);
    });

    let connection_limit = Arc::new(Semaphore::new(MAX_CONNECTIONS));
    let mut handlers = JoinSet::new();
    let mut finished_rx = state.take_finished_receiver();
    let mut maintenance = tokio::time::interval(Duration::from_secs(1));

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        if !state.is_accepting() {
                            drop(stream);
                            continue;
                        }
                        let permit = match Arc::clone(&connection_limit).try_acquire_owned() {
                            Ok(permit) => permit,
                            Err(_) => {
                                warn!("Rejecting IPC connection: connection limit reached");
                                drop(stream);
                                continue;
                            }
                        };
                        let state = Arc::clone(&state);
                        let shutdown_tx_for_connection = shutdown_tx.clone();
                        let shutdown = shutdown_tx.subscribe();
                        handlers.spawn(async move {
                            let _permit = permit;
                            if let Err(error) = handle_connection(
                                stream,
                                state,
                                shutdown_tx_for_connection,
                                shutdown,
                            )
                            .await
                            {
                                error!("Connection error: {error}");
                            }
                        });
                    }
                    Err(error) => error!("Accept error: {error}"),
                }
            }
            result = handlers.join_next(), if !handlers.is_empty() => {
                if let Some(Err(error)) = result {
                    error!("Connection task failed: {error}");
                }
            }
            Some(job_id) = finished_rx.recv() => {
                reap_finished_job(&state, &job_id).await;
            }
            _ = maintenance.tick() => {
                for job_id in state.finished_job_ids() {
                    reap_finished_job(&state, &job_id).await;
                }
            }
            () = shutdown_signal(&shutdown_rx) => {
                info!("Shutdown signal received, stopping daemon");
                break;
            }
        }
    }

    initiate_shutdown(&state, &shutdown_tx);

    while let Some(result) = handlers.join_next().await {
        if let Err(error) = result {
            error!("Connection task failed during shutdown: {error}");
        }
    }

    while state.active_job_count() > 0 {
        tokio::select! {
            Some(job_id) = finished_rx.recv() => reap_finished_job(&state, &job_id).await,
            _ = maintenance.tick() => {
                for job_id in state.finished_job_ids() {
                    reap_finished_job(&state, &job_id).await;
                }
            }
            else => break,
        }
    }

    info!("Daemon shutdown complete");
    Ok(())
}

async fn reap_finished_job(state: &Arc<DaemonState>, job_id: &str) {
    let Some(mut control) = state.remove_job(job_id) else {
        return;
    };
    if let Some(join) = control.join.take()
        && let Err(error) = join.await
    {
        error!("Job task {job_id} failed: {error}");
        let _ = state.db.lock().expect("database lock poisoned").finish(
            job_id,
            Status::Interrupted,
            None,
        );
    }
}

fn initiate_shutdown(state: &Arc<DaemonState>, shutdown_tx: &watch::Sender<bool>) {
    state.begin_shutdown();
    let _ = shutdown_tx.send(true);
}

async fn wait_for_shutdown_signal() {
    #[cfg(unix)]
    {
        use tokio::signal::unix::{SignalKind, signal};
        let mut sigterm = signal(SignalKind::terminate()).expect("failed to register SIGTERM");
        let mut sigint = signal(SignalKind::interrupt()).expect("failed to register SIGINT");

        tokio::select! {
            _ = sigterm.recv() => info!("Received SIGTERM"),
            _ = sigint.recv() => info!("Received SIGINT"),
        }
    }
    #[cfg(not(unix))]
    {
        tokio::signal::ctrl_c()
            .await
            .expect("failed to listen for ctrl-c");
        info!("Received Ctrl+C");
    }
}

async fn shutdown_signal(rx: &watch::Receiver<bool>) {
    let mut rx = rx.clone();
    while !*rx.borrow() {
        if rx.changed().await.is_err() {
            break;
        }
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    state: Arc<DaemonState>,
    shutdown_tx: watch::Sender<bool>,
    shutdown_rx: watch::Receiver<bool>,
) -> Result<()> {
    loop {
        let request = tokio::select! {
            () = shutdown_signal(&shutdown_rx) => break,
            result = tokio::time::timeout(IPC_OPERATION_TIMEOUT, read_message(&mut stream)) => {
                match result {
                    Ok(Ok(Some(request))) => request,
                    Ok(Ok(None)) => break,
                    Ok(Err(error)) => {
                        warn!("Read error: {error}");
                        break;
                    }
                    Err(_) => {
                        warn!("IPC read timed out");
                        break;
                    }
                }
            }
        };

        let response = handle_request(request, &state, &shutdown_tx).await;
        match tokio::time::timeout(IPC_OPERATION_TIMEOUT, write_message(&mut stream, &response))
            .await
        {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                warn!("Write error: {error}");
                break;
            }
            Err(_) => {
                warn!("IPC write timed out");
                break;
            }
        }

        if *shutdown_rx.borrow() {
            break;
        }
    }

    Ok(())
}

async fn read_message(stream: &mut UnixStream) -> Result<Option<Request>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(error) if error.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(error) => return Err(error.into()),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    if len > MAX_MESSAGE_SIZE {
        anyhow::bail!("message too large: {len} bytes");
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;
    Ok(Some(serde_json::from_slice(&buf)?))
}

async fn write_message(stream: &mut UnixStream, response: &Response) -> Result<()> {
    let data = serde_json::to_vec(response)?;
    #[allow(clippy::cast_possible_truncation)]
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.flush().await?;
    Ok(())
}

async fn handle_request(
    request: Request,
    state: &Arc<DaemonState>,
    shutdown_tx: &watch::Sender<bool>,
) -> Response {
    match request {
        Request::Ping => match (state.running_count(), state.total_jobs()) {
            (Ok(running_jobs), Ok(total_jobs)) => Response::Pong {
                pid: std::process::id(),
                uptime_secs: state.uptime_secs(),
                running_jobs,
                total_jobs,
            },
            (Err(error), _) | (_, Err(error)) => Response::Error(error.to_string()),
        },
        Request::Shutdown => {
            info!("Shutdown requested via IPC");
            initiate_shutdown(state, shutdown_tx);
            Response::Ok
        }
        Request::Run {
            command,
            name,
            cwd,
            project,
            timeout_secs,
            idempotency_key,
        } => spawner::spawn_job(
            state,
            command,
            name,
            cwd,
            project,
            timeout_secs,
            idempotency_key,
        ),
        Request::Stop { id, force } => spawner::stop_job(state, &id, force).await,
        Request::Status { id } => match state.get_job(&id) {
            Ok(Some(job)) => Response::Job(Box::new(job)),
            Ok(None) => Response::Error(format!("Job not found: {id}")),
            Err(error) => Response::Error(error.to_string()),
        },
        Request::List { status, limit } => {
            let status_filter = match status {
                Some(status) => match status.parse::<Status>() {
                    Ok(status) => Some(status),
                    Err(error) => return Response::Error(error.to_string()),
                },
                None => None,
            };
            match state.list_jobs(status_filter, limit) {
                Ok(jobs) => Response::Jobs(jobs),
                Err(error) => Response::Error(error.to_string()),
            }
        }
        Request::Wait { id, timeout_secs } => spawner::wait_for_job(state, &id, timeout_secs).await,
    }
}
