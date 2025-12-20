use crate::core::{Database, Paths, Status};
use anyhow::Result;

const DEFAULT_LIMIT: usize = 10;

pub fn execute(
    status_filter: Option<String>,
    failed: bool,
    limit: Option<usize>,
    all: bool,
    json: bool,
) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    let status = if failed {
        Some(Status::Failed)
    } else {
        status_filter.map(|s| s.parse::<Status>()).transpose()?
    };

    let effective_limit = if all {
        None
    } else {
        Some(limit.unwrap_or(DEFAULT_LIMIT))
    };

    let jobs = db.list(status, effective_limit)?;

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
