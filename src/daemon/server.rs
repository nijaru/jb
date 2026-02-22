use crate::core::ipc::{Request, Response};
use crate::core::{Paths, Status};
use crate::daemon::spawner;
use crate::daemon::state::DaemonState;
use anyhow::Result;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tokio::sync::watch;
use tracing::{error, info, warn};

pub async fn run(paths: Paths, state: Arc<DaemonState>) -> Result<()> {
    let listener = UnixListener::bind(paths.socket())?;
    info!("Listening on {}", paths.socket().display());

    // Shutdown signal channel
    let (shutdown_tx, shutdown_rx) = watch::channel(false);

    // Spawn signal handler
    let shutdown_tx_clone = shutdown_tx.clone();
    tokio::spawn(async move {
        wait_for_shutdown_signal().await;
        let _ = shutdown_tx_clone.send(true);
    });

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _addr)) => {
                        let state = state.clone();
                        let shutdown_tx = shutdown_tx.clone();
                        tokio::spawn(async move {
                            if let Err(e) = handle_connection(stream, state, shutdown_tx).await {
                                error!("Connection error: {}", e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Accept error: {}", e);
                    }
                }
            }
            () = shutdown_signal(&shutdown_rx) => {
                info!("Shutdown signal received, stopping daemon");
                break;
            }
        }
    }

    // Mark running jobs as interrupted
    state.interrupt_running_jobs();

    info!("Daemon shutdown complete");
    Ok(())
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
) -> Result<()> {
    loop {
        let request = match read_message(&mut stream).await {
            Ok(Some(req)) => req,
            Ok(None) => break,
            Err(e) => {
                warn!("Read error: {}", e);
                break;
            }
        };

        let response = handle_request(request, &state, &shutdown_tx).await;

        if let Err(e) = write_message(&mut stream, &response).await {
            warn!("Write error: {}", e);
            break;
        }
    }

    Ok(())
}

async fn read_message(stream: &mut UnixStream) -> Result<Option<Request>> {
    let mut len_buf = [0u8; 4];
    match stream.read_exact(&mut len_buf).await {
        Ok(_) => {}
        Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => return Ok(None),
        Err(e) => return Err(e.into()),
    }

    let len = u32::from_be_bytes(len_buf) as usize;
    if len > 10 * 1024 * 1024 {
        anyhow::bail!("message too large: {len} bytes");
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let request: Request = serde_json::from_slice(&buf)?;
    Ok(Some(request))
}

async fn write_message(stream: &mut UnixStream, response: &Response) -> Result<()> {
    let data = serde_json::to_vec(response)?;
    #[allow(clippy::cast_possible_truncation)] // messages are always < 4GB
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
        Request::Ping => Response::Pong {
            pid: std::process::id(),
            uptime_secs: state.uptime_secs(),
            running_jobs: state.running_count(),
            total_jobs: state.total_jobs(),
        },

        Request::Shutdown => {
            info!("Shutdown requested via IPC");
            let _ = shutdown_tx.send(true);
            Response::Ok
        }

        Request::Run {
            command,
            name,
            cwd,
            project,
            timeout_secs,
            context,
            idempotency_key,
        } => spawner::spawn_job(
            state,
            command,
            name,
            cwd,
            project,
            timeout_secs,
            context,
            idempotency_key,
        ),

        Request::Stop { id, force } => match state.get_job(&id) {
            Ok(Some(job)) => {
                if job.status != Status::Running {
                    return Response::Error(format!("Job {} is not running", job.short_id()));
                }
                spawner::stop_job(state, &job.id, force)
            }
            Ok(None) => Response::Error(format!("Job not found: {id}")),
            Err(e) => Response::Error(e.to_string()),
        },

        Request::Status { id } => match state.get_job(&id) {
            Ok(Some(job)) => Response::Job(Box::new(job)),
            Ok(None) => Response::Error(format!("Job not found: {id}")),
            Err(e) => Response::Error(e.to_string()),
        },

        Request::List { status, limit } => {
            let status_filter = status.and_then(|s| s.parse::<Status>().ok());

            match state.list_jobs(status_filter, limit) {
                Ok(jobs) => Response::Jobs(jobs),
                Err(e) => Response::Error(e.to_string()),
            }
        }

        Request::Wait { id, timeout_secs } => spawner::wait_for_job(state, &id, timeout_secs).await,
    }
}
