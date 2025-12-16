use crate::client::DaemonClient;
use anyhow::Result;
use jb_core::ipc::{Request, Response};
use jb_core::{Database, Paths};

pub async fn execute(id: String, json: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

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

    // Send to daemon
    let mut client = DaemonClient::connect_or_start().await?;

    let request = Request::Run {
        command: job.command.clone(),
        name: job.name.clone(),
        cwd: job.cwd.to_string_lossy().to_string(),
        project: job.project.to_string_lossy().to_string(),
        timeout_secs: job.timeout_secs,
        context: job.context.clone(),
        idempotency_key: None, // Don't reuse idempotency key
    };

    match client.send(request).await? {
        Response::Job(new_job) => {
            if json {
                println!("{}", serde_json::to_string(&new_job)?);
            } else {
                println!("{}", new_job.short_id());
            }
            Ok(())
        }
        Response::Error(e) => anyhow::bail!("{}", e),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}
