use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Job, Paths, parse_duration};
use anyhow::Result;
use std::time::{Duration, Instant};

pub async fn execute(id: String, timeout: Option<String>) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;

    // Check for orphaned jobs (dead processes still marked running)
    db.recover_orphans();

    let job = db.resolve(&id)?;

    // If already terminal, return immediately
    if job.status.is_terminal() {
        handle_terminal(&job);
        return Ok(());
    }

    let timeout_secs = timeout.map(|t| parse_duration(&t)).transpose()?;

    // Wait via daemon
    if let Ok(mut client) = DaemonClient::connect_or_start().await {
        let request = Request::Wait {
            id: job.id.clone(),
            timeout_secs,
        };

        match client.send(request).await? {
            Response::Job(completed) => {
                handle_terminal(&completed);
                return Ok(());
            }
            Response::Error(e) => {
                if e.contains("timed out") {
                    eprintln!("Timeout - job still running");
                    std::process::exit(124);
                }
                anyhow::bail!("{e}");
            }
            _ => {}
        }
    }

    // Fallback: poll DB
    let start = Instant::now();

    loop {
        let current = db
            .get(&job.id)?
            .ok_or_else(|| anyhow::anyhow!("job {} disappeared", job.id))?;

        if current.status.is_terminal() {
            handle_terminal(&current);
            return Ok(());
        }

        if let Some(timeout_secs) = timeout_secs
            && start.elapsed() > Duration::from_secs(timeout_secs)
        {
            eprintln!("Timeout - job still running");
            std::process::exit(124);
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn handle_terminal(job: &Job) {
    match job.exit_code {
        Some(0) => {
            println!("Completed (exit 0)");
        }
        Some(code) => {
            println!("Failed (exit {code})");
            std::process::exit(code);
        }
        None => {
            println!("{}", job.status);
        }
    }
}
