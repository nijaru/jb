use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Job, Paths, parse_duration};
use anyhow::Result;
use std::time::{Duration, Instant};

pub async fn execute(id: String, timeout: Option<String>) -> Result<i32> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;
    let job = db.resolve(&id)?;

    if job.status.is_terminal() {
        return Ok(print_terminal(&job));
    }

    let timeout_secs = timeout.map(|value| parse_duration(&value)).transpose()?;
    if let Ok(mut client) = DaemonClient::connect_or_start().await {
        match client
            .send(Request::Wait {
                id: job.id.clone(),
                timeout_secs,
            })
            .await?
        {
            Response::Job(completed) => return Ok(print_terminal(&completed)),
            Response::WaitTimeout => {
                eprintln!("Timeout - job still running");
                return Ok(124);
            }
            Response::Error(error) => anyhow::bail!("{error}"),
            _ => anyhow::bail!("Unexpected response from daemon"),
        }
    }

    let start = Instant::now();
    loop {
        let current = db
            .get(&job.id)?
            .ok_or_else(|| anyhow::anyhow!("job {} disappeared", job.id))?;

        if current.status.is_terminal() {
            return Ok(print_terminal(&current));
        }

        if let Some(timeout_secs) = timeout_secs
            && start.elapsed() >= Duration::from_secs(timeout_secs)
        {
            eprintln!("Timeout - job still running");
            return Ok(124);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn print_terminal(job: &Job) -> i32 {
    match job.status {
        crate::core::Status::Completed => println!("Completed (exit 0)"),
        crate::core::Status::Failed => {
            println!("Failed (exit {})", job.exit_code.unwrap_or(1));
        }
        status => println!("{status}"),
    }
    job.status.cli_exit_code(job.exit_code)
}
