use crate::core::{Database, Paths, Status};
use anyhow::Result;
use colored::Colorize;

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

    // Check for orphaned jobs (dead processes still marked running)
    db.recover_orphans();

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
        let cmd = truncate(&job.command, 28);
        let started = job
            .started_at
            .map_or_else(|| "-".to_string(), format_relative_time);
        let exit = job
            .exit_code
            .map_or_else(|| "-".to_string(), |c| c.to_string());

        let status_colored = format_status(job.status);
        println!(
            "{:<10} {} {:<6} {:<12} {:<30} {}",
            job.short_id(),
            status_colored,
            exit,
            truncate(name, 10),
            cmd,
            started
        );
    }

    Ok(())
}

fn format_status(status: Status) -> String {
    // Pad to 12 chars before colorizing to preserve alignment
    let s = format!("{:<12}", status.as_str());
    match status {
        Status::Pending => s.yellow().to_string(),
        Status::Running => s.cyan().bold().to_string(),
        Status::Completed => s.green().to_string(),
        Status::Failed => s.red().to_string(),
        Status::Stopped => s.magenta().to_string(),
        Status::Interrupted => s.yellow().dimmed().to_string(),
    }
}

fn truncate(s: &str, max: usize) -> String {
    let char_count = s.chars().count();
    if char_count > max {
        let truncated: String = s.chars().take(max.saturating_sub(3)).collect();
        format!("{truncated}...")
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
