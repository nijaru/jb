use anyhow::Result;
use jb_core::{detect_project, Database, Paths, Status};
use std::env;

pub async fn execute(id: Option<String>, json: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    match id {
        Some(id) => show_job_status(&db, &paths, &id, json),
        None => show_system_status(&db, &paths, json),
    }
}

fn show_job_status(db: &Database, paths: &Paths, id: &str, json: bool) -> Result<()> {
    let job = db.get(id)?;

    let job = match job {
        Some(j) => j,
        None => {
            let by_name = db.get_by_name(id)?;
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

    if json {
        println!("{}", serde_json::to_string_pretty(&job)?);
        return Ok(());
    }

    println!("ID:       {}", job.id);
    if let Some(name) = &job.name {
        println!("Name:     {}", name);
    }
    println!("Status:   {}", job.status);
    println!("Command:  {}", job.command);
    println!("Project:  {}", job.project.display());
    println!("CWD:      {}", job.cwd.display());
    println!("Created:  {}", job.created_at);
    if let Some(started) = job.started_at {
        println!("Started:  {}", started);
    }
    if let Some(finished) = job.finished_at {
        println!("Finished: {}", finished);
    }
    if let Some(pid) = job.pid {
        println!("PID:      {}", pid);
    }
    if let Some(code) = job.exit_code {
        println!("Exit:     {}", code);
    }
    if let Some(ctx) = &job.context {
        println!("Context:  {}", ctx);
    }

    let log_path = paths.log_file(&job.id);
    if log_path.exists() {
        let lines = std::fs::read_to_string(&log_path)?
            .lines()
            .count();
        println!("Output:   {} lines", lines);
    }

    Ok(())
}

fn show_system_status(db: &Database, paths: &Paths, json: bool) -> Result<()> {
    let all_jobs = db.list(None, None)?;
    let running = all_jobs.iter().filter(|j| j.status == Status::Running).count();
    let completed = all_jobs.iter().filter(|j| j.status == Status::Completed).count();
    let failed = all_jobs.iter().filter(|j| j.status == Status::Failed).count();

    let cwd = env::current_dir()?;
    let project = detect_project(&cwd);
    let project_jobs = db.list(None, Some(&project))?.len();

    let daemon_running = paths.socket().exists();

    if json {
        let status = serde_json::json!({
            "daemon": daemon_running,
            "jobs": {
                "running": running,
                "completed": completed,
                "failed": failed,
                "total": all_jobs.len()
            },
            "project": {
                "path": project,
                "jobs": project_jobs
            }
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!("Daemon:   {}", if daemon_running { "running" } else { "stopped" });
    println!("Jobs:     {} running, {} completed, {} failed ({} total)",
        running, completed, failed, all_jobs.len());
    println!("Project:  {} ({} jobs)", project.display(), project_jobs);

    Ok(())
}
