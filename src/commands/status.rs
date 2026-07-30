use crate::core::{Database, Paths, Status};
use anyhow::Result;

pub fn execute(id: Option<String>, json: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;

    match id {
        Some(id) => show_job_status(&db, &paths, &id, json),
        None => show_system_status(&db, &paths, json),
    }
}

fn show_job_status(db: &Database, paths: &Paths, id: &str, json: bool) -> Result<()> {
    let job = db.resolve(id)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&job)?);
        return Ok(());
    }

    println!("ID:       {}", job.id);
    if let Some(name) = &job.name {
        println!("Name:     {name}");
    }
    println!("Status:   {}", job.status);
    println!("Command:  {}", job.command);
    println!("Project:  {}", job.project.display());
    println!("CWD:      {}", job.cwd.display());
    println!("Created:  {}", job.created_at);
    if let Some(started) = job.started_at {
        println!("Started:  {started}");
    }
    if let Some(finished) = job.finished_at {
        println!("Finished: {finished}");
    }
    if let Some(pid) = job.pid {
        println!("PID:      {pid}");
    }
    if let Some(code) = job.exit_code {
        println!("Exit:     {code}");
    }
    let log_path = paths.log_file(&job.id);
    if log_path.exists() {
        use std::io::BufRead;
        let lines = std::io::BufReader::new(std::fs::File::open(&log_path)?)
            .lines()
            .count();
        println!("Output:   {lines} lines");
    }

    Ok(())
}

fn daemon_is_reachable(paths: &Paths) -> bool {
    #[cfg(unix)]
    {
        std::os::unix::net::UnixStream::connect(paths.socket()).is_ok()
    }

    #[cfg(not(unix))]
    {
        paths.socket().exists()
    }
}

fn show_system_status(db: &Database, paths: &Paths, json: bool) -> Result<()> {
    // Use COUNT(*) queries instead of loading all jobs into memory
    let pending = db.count(Some(Status::Pending))?;
    let running = db.count(Some(Status::Running))?;
    let completed = db.count(Some(Status::Completed))?;
    let failed = db.count(Some(Status::Failed))?;
    let stopped = db.count(Some(Status::Stopped))?;
    let interrupted = db.count(Some(Status::Interrupted))?;
    let timeout = db.count(Some(Status::Timeout))?;
    let total = db.count(None)?;

    let daemon_running = daemon_is_reachable(paths);

    if json {
        let status = serde_json::json!({
            "daemon": daemon_running,
            "jobs": {
                "pending": pending,
                "running": running,
                "completed": completed,
                "failed": failed,
                "stopped": stopped,
                "interrupted": interrupted,
                "timeout": timeout,
                "total": total
            }
        });
        println!("{}", serde_json::to_string_pretty(&status)?);
        return Ok(());
    }

    println!(
        "Daemon:   {}",
        if daemon_running { "running" } else { "stopped" }
    );
    println!(
        "Jobs:     {} pending, {} running, {} completed, {} failed, {} stopped, {} interrupted, {} timeout ({} total)",
        pending, running, completed, failed, stopped, interrupted, timeout, total
    );

    Ok(())
}
