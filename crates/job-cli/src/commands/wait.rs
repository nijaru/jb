use anyhow::Result;
use jb_core::{Database, Paths};
use std::time::{Duration, Instant};

pub async fn execute(id: String, timeout: Option<String>) -> Result<()> {
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

    let timeout_duration = timeout.map(|t| parse_duration(&t)).transpose()?;
    let start = Instant::now();

    loop {
        let current = db.get(&job.id)?.unwrap();

        if current.status.is_terminal() {
            match current.exit_code {
                Some(0) => {
                    println!("Completed (exit 0)");
                    return Ok(());
                }
                Some(code) => {
                    println!("Failed (exit {})", code);
                    std::process::exit(1);
                }
                None => {
                    println!("{}", current.status);
                    return Ok(());
                }
            }
        }

        if let Some(timeout_secs) = timeout_duration {
            if start.elapsed() > Duration::from_secs(timeout_secs) {
                eprintln!("Timeout - job still running");
                std::process::exit(124);
            }
        }

        tokio::time::sleep(Duration::from_millis(500)).await;
    }
}

fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, unit) = if s.ends_with("s") {
        (&s[..s.len() - 1], 1u64)
    } else if s.ends_with("m") {
        (&s[..s.len() - 1], 60u64)
    } else if s.ends_with("h") {
        (&s[..s.len() - 1], 3600u64)
    } else if s.ends_with("d") {
        (&s[..s.len() - 1], 86400u64)
    } else {
        anyhow::bail!("Invalid duration format. Use: 30s, 5m, 1h, 7d");
    };

    let n: u64 = num.parse()?;
    Ok(n * unit)
}
