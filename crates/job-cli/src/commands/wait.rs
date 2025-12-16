use crate::client::DaemonClient;
use anyhow::Result;
use jb_core::ipc::{Request, Response};
use jb_core::{Database, Paths};

pub async fn execute(id: String, timeout: Option<String>) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    // Resolve job ID/name
    let job = db.get(&id)?;
    let job = if let Some(j) = job {
        j
    } else {
        let by_name = db.get_by_name(&id)?;
        match by_name.len() {
            0 => anyhow::bail!("No job found with ID or name '{id}'"),
            1 => by_name.into_iter().next().unwrap(),
            _ => {
                eprintln!("Multiple jobs named '{id}'. Use ID instead:");
                for j in by_name {
                    eprintln!("  {} ({})", j.short_id(), j.status);
                }
                anyhow::bail!("Ambiguous job name");
            }
        }
    };

    // If already terminal, return immediately
    if job.status.is_terminal() {
        return handle_terminal(&job);
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
                return handle_terminal(&completed);
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
    use std::time::{Duration, Instant};
    let start = Instant::now();

    loop {
        let current = db.get(&job.id)?.unwrap();

        if current.status.is_terminal() {
            return handle_terminal(&current);
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

fn handle_terminal(job: &jb_core::Job) -> Result<()> {
    match job.exit_code {
        Some(0) => {
            println!("Completed (exit 0)");
            Ok(())
        }
        Some(code) => {
            println!("Failed (exit {code})");
            std::process::exit(code);
        }
        None => {
            println!("{}", job.status);
            Ok(())
        }
    }
}

fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, unit) = if let Some(n) = s.strip_suffix('s') {
        (n, 1u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60u64)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600u64)
    } else if let Some(n) = s.strip_suffix('d') {
        (n, 86400u64)
    } else {
        anyhow::bail!("Invalid duration format. Use: 30s, 5m, 1h, 7d");
    };

    let n: u64 = num.parse()?;
    Ok(n * unit)
}
