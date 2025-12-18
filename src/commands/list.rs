use crate::core::{Database, Paths, Status, detect_project};
use anyhow::Result;
use std::env;

pub fn execute(status_filter: Option<String>, here: bool, all: bool, json: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    let status = status_filter.map(|s| s.parse::<Status>()).transpose()?;

    let project = if all {
        None
    } else if here {
        Some(env::current_dir()?)
    } else {
        let cwd = env::current_dir()?;
        Some(detect_project(&cwd))
    };

    let jobs = db.list(status, project.as_ref())?;

    if json {
        println!("{}", serde_json::to_string(&jobs)?);
        return Ok(());
    }

    if jobs.is_empty() {
        println!("No jobs found");
        return Ok(());
    }

    println!(
        "{:<10} {:<12} {:<6} {:<12} {:<30} STARTED",
        "ID", "STATUS", "EXIT", "NAME", "COMMAND"
    );

    for job in jobs {
        let name = job.name.as_deref().unwrap_or("-");
        let cmd = if job.command.len() > 28 {
            format!("{}...", &job.command[..25])
        } else {
            job.command.clone()
        };
        let started = job
            .started_at
            .map_or_else(|| "-".to_string(), format_relative_time);
        let exit = job
            .exit_code
            .map_or_else(|| "-".to_string(), |c| c.to_string());

        println!(
            "{:<10} {:<12} {:<6} {:<12} {:<30} {}",
            job.short_id(),
            job.status,
            exit,
            truncate(name, 10),
            cmd,
            started
        );
    }

    Ok(())
}

fn truncate(s: &str, max: usize) -> String {
    if s.len() > max {
        format!("{}...", &s[..max - 3])
    } else {
        s.to_string()
    }
}

fn format_relative_time(t: chrono::DateTime<chrono::Utc>) -> String {
    let now = chrono::Utc::now();
    let diff = now.signed_duration_since(t);

    if diff.num_days() > 0 {
        format!("{}d ago", diff.num_days())
    } else if diff.num_hours() > 0 {
        format!("{}h ago", diff.num_hours())
    } else if diff.num_minutes() > 0 {
        format!("{}m ago", diff.num_minutes())
    } else {
        "just now".to_string()
    }
}
