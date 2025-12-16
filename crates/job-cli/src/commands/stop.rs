use crate::client::DaemonClient;
use anyhow::Result;
use jb_core::ipc::{Request, Response};
use jb_core::{Database, Paths, Status};

pub async fn execute(id: String, force: bool, json: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    // Resolve job ID/name
    let job = db.get(&id)?;
    let job = match job {
        Some(j) => j,
        None => {
            let by_name = db.get_by_name(&id)?;
            match by_name.len() {
                0 => anyhow::bail!("No job found with ID or name '{}'", id),
                1 => by_name.into_iter().next().unwrap(),
                _ => {
                    eprintln!("Multiple jobs named '{}'. Use ID instead:", id);
                    for j in by_name {
                        eprintln!("  {} ({})", j.short_id(), j.status);
                    }
                    anyhow::bail!("Ambiguous job name");
                }
            }
        }
    };

    if job.status.is_terminal() {
        if json {
            println!("{}", serde_json::to_string(&job)?);
        } else {
            println!("Job already {}", job.status);
        }
        return Ok(());
    }

    // Try to stop via daemon
    if let Ok(mut client) = DaemonClient::connect_or_start().await {
        let request = Request::Stop {
            id: job.id.clone(),
            force,
        };

        match client.send(request).await? {
            Response::Ok => {
                if json {
                    let updated = db.get(&job.id)?.unwrap();
                    println!("{}", serde_json::to_string(&updated)?);
                } else {
                    println!("Stopped {}", job.short_id());
                }
                return Ok(());
            }
            Response::Error(e) => {
                // Job might not be running in daemon, fall back to direct kill
                if !e.contains("not running") {
                    anyhow::bail!("{}", e);
                }
            }
            _ => {}
        }
    }

    // Fallback: direct kill (for jobs started before daemon)
    if job.status == Status::Pending {
        db.update_status(&job.id, Status::Stopped)?;
    } else if let Some(pid) = job.pid {
        kill_process(pid, force)?;
        db.update_finished(&job.id, Status::Stopped, None)?;
    }

    if json {
        let updated = db.get(&job.id)?.unwrap();
        println!("{}", serde_json::to_string(&updated)?);
    } else {
        println!("Stopped {}", job.short_id());
    }

    Ok(())
}

#[cfg(unix)]
fn kill_process(pid: u32, force: bool) -> Result<()> {
    use nix::sys::signal::{killpg, Signal};
    use nix::unistd::Pid;

    let signal = if force {
        Signal::SIGKILL
    } else {
        Signal::SIGTERM
    };

    let _ = killpg(Pid::from_raw(pid as i32), signal);
    Ok(())
}

#[cfg(not(unix))]
fn kill_process(_pid: u32, _force: bool) -> Result<()> {
    anyhow::bail!("Process termination not yet implemented on this platform")
}
