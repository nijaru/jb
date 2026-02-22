use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Paths};
use anyhow::Result;

pub async fn execute(id: String, json: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;
    let job = db.resolve(&id)?;

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
        Response::UserError(e) => anyhow::bail!(crate::core::UserError::new(e)),
        Response::Error(e) => anyhow::bail!("{e}"),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}
