use crate::spawner;
use crate::state::DaemonState;
use anyhow::Result;
use jb_core::ipc::{Request, Response};
use jb_core::{Paths, Status};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::{UnixListener, UnixStream};
use tracing::{error, info, warn};

pub async fn run(paths: Paths, state: Arc<DaemonState>) -> Result<()> {
    let listener = UnixListener::bind(paths.socket())?;
    info!("Listening on {}", paths.socket().display());

    loop {
        match listener.accept().await {
            Ok((stream, _addr)) => {
                let state = state.clone();
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream, state).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept error: {}", e);
            }
        }
    }
}

async fn handle_connection(mut stream: UnixStream, state: Arc<DaemonState>) -> Result<()> {
    loop {
        let request = match read_message(&mut stream).await {
            Ok(Some(req)) => req,
            Ok(None) => break,
            Err(e) => {
                warn!("Read error: {}", e);
                break;
            }
        };

        let response = handle_request(request, &state).await;

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
        anyhow::bail!("message too large: {} bytes", len);
    }

    let mut buf = vec![0u8; len];
    stream.read_exact(&mut buf).await?;

    let request: Request = serde_json::from_slice(&buf)?;
    Ok(Some(request))
}

async fn write_message(stream: &mut UnixStream, response: &Response) -> Result<()> {
    let data = serde_json::to_vec(response)?;
    let len = (data.len() as u32).to_be_bytes();

    stream.write_all(&len).await?;
    stream.write_all(&data).await?;
    stream.flush().await?;

    Ok(())
}

async fn handle_request(request: Request, state: &Arc<DaemonState>) -> Response {
    match request {
        Request::Ping => Response::Pong {
            pid: std::process::id(),
            uptime_secs: state.uptime_secs(),
            running_jobs: state.running_count(),
            total_jobs: state.total_jobs(),
        },

        Request::Shutdown => {
            info!("Shutdown requested");
            std::process::exit(0);
        }

        Request::Run {
            command,
            name,
            cwd,
            project,
            timeout_secs,
            context,
            idempotency_key,
        } => {
            spawner::spawn_job(
                state,
                command,
                name,
                cwd,
                project,
                timeout_secs,
                context,
                idempotency_key,
            )
            .await
        }

        Request::Stop { id, force } => match state.get_job(&id) {
            Ok(Some(job)) => {
                if job.status != Status::Running {
                    return Response::Error(format!("Job {} is not running", job.short_id()));
                }
                spawner::stop_job(state, &job.id, force).await
            }
            Ok(None) => Response::Error(format!("Job not found: {}", id)),
            Err(e) => Response::Error(e.to_string()),
        },

        Request::Status { id } => match state.get_job(&id) {
            Ok(Some(job)) => Response::Job(job),
            Ok(None) => Response::Error(format!("Job not found: {}", id)),
            Err(e) => Response::Error(e.to_string()),
        },

        Request::List { status, project } => {
            let status_filter = status.and_then(|s| s.parse::<Status>().ok());
            let project_filter = project.map(PathBuf::from);

            match state.list_jobs(status_filter, project_filter.as_ref()) {
                Ok(jobs) => Response::Jobs(jobs),
                Err(e) => Response::Error(e.to_string()),
            }
        }

        Request::Wait { id, timeout_secs } => spawner::wait_for_job(state, &id, timeout_secs).await,
    }
}
