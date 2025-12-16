use anyhow::Result;
use job_core::{Database, Paths, Status};

pub async fn execute(id: String, force: bool, json: bool) -> Result<()> {
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

    if job.status.is_terminal() {
        if json {
            println!("{}", serde_json::to_string(&job)?);
        } else {
            println!("Job already {}", job.status);
        }
        return Ok(());
    }

    // TODO: Send stop signal to daemon
    // For now, just update the database
    if job.status == Status::Pending {
        db.update_status(&job.id, Status::Stopped)?;
    } else if let Some(pid) = job.pid {
        let signal = if force {
            nix::sys::signal::Signal::SIGKILL
        } else {
            nix::sys::signal::Signal::SIGTERM
        };

        // Try to kill the process group
        let _ = nix::sys::signal::killpg(
            nix::unistd::Pid::from_raw(pid as i32),
            signal,
        );

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
