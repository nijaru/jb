use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Paths, Status, kill_process_group};
use anyhow::Result;

pub async fn execute(id: String, force: bool, json: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;
    let job = db.resolve(&id)?;

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
                    anyhow::bail!("{e}");
                }
            }
            _ => {}
        }
    }

    // Fallback: direct kill (for jobs started before daemon)
    if job.status == Status::Pending {
        db.update_status(&job.id, Status::Stopped)?;
    } else if let Some(pid) = job.pid {
        kill_process_group(pid, force);
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
