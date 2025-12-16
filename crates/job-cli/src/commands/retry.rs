use anyhow::Result;
use job_core::{Database, Job, Paths};

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

    let mut new_job = Job::new(job.command.clone(), job.cwd.clone(), job.project.clone());

    if let Some(name) = &job.name {
        new_job = new_job.with_name(name.clone());
    }

    if let Some(timeout) = job.timeout_secs {
        new_job = new_job.with_timeout(timeout);
    }

    if let Some(ctx) = &job.context {
        new_job = new_job.with_context(ctx.clone());
    }

    db.insert(&new_job)?;

    // TODO: Send to daemon for execution

    if json {
        println!("{}", serde_json::to_string(&new_job)?);
    } else {
        println!("{}", new_job.short_id());
    }

    Ok(())
}
