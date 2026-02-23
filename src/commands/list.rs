use crate::core::{Database, Paths, Status};
use anyhow::Result;
use colored::Colorize;

const DEFAULT_LIMIT: usize = 10;

fn query_jobs(
    status_filter: Option<String>,
    failed: bool,
    limit: Option<usize>,
    all: bool,
    db: &Database,
) -> Result<Vec<crate::core::Job>> {
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

    db.list(status, effective_limit)
}

pub fn execute(
    status_filter: Option<String>,
    failed: bool,
    limit: Option<usize>,
    all: bool,
    json: bool,
) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;

    // Check for orphaned jobs (dead processes still marked running)
    db.recover_orphans();

    let jobs = query_jobs(status_filter, failed, limit, all, &db)?;

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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Database, Job, Paths};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let paths = Paths::with_root(tmp.path().to_path_buf());
        let db = Database::open(&paths).unwrap();
        (db, tmp)
    }

    fn job(id: &str, status: Status) -> Job {
        let mut j = Job::new(
            id.into(),
            format!("echo {id}"),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        );
        j.status = status;
        j
    }

    #[test]
    fn test_no_filter_applies_default_limit() {
        let (db, _tmp) = setup();
        for i in 0..15u8 {
            db.insert(&job(&format!("j{i:02}"), Status::Completed))
                .unwrap();
        }
        let jobs = query_jobs(None, false, None, false, &db).unwrap();
        assert_eq!(jobs.len(), DEFAULT_LIMIT);
    }

    #[test]
    fn test_all_flag_returns_all_jobs() {
        let (db, _tmp) = setup();
        for i in 0..15u8 {
            db.insert(&job(&format!("j{i:02}"), Status::Completed))
                .unwrap();
        }
        let jobs = query_jobs(None, false, None, true, &db).unwrap();
        assert_eq!(jobs.len(), 15);
    }

    #[test]
    fn test_failed_flag_filters_to_failed_only() {
        let (db, _tmp) = setup();
        db.insert(&job("a", Status::Failed)).unwrap();
        db.insert(&job("b", Status::Completed)).unwrap();
        db.insert(&job("c", Status::Failed)).unwrap();

        let jobs = query_jobs(None, true, None, true, &db).unwrap();
        assert_eq!(jobs.len(), 2);
        assert!(jobs.iter().all(|j| j.status == Status::Failed));
    }

    #[test]
    fn test_status_filter_string() {
        let (db, _tmp) = setup();
        db.insert(&job("a", Status::Running)).unwrap();
        db.insert(&job("b", Status::Completed)).unwrap();

        let jobs = query_jobs(Some("running".into()), false, None, true, &db).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "a");
    }

    #[test]
    fn test_custom_limit() {
        let (db, _tmp) = setup();
        for i in 0..10u8 {
            db.insert(&job(&format!("j{i}"), Status::Completed))
                .unwrap();
        }
        let jobs = query_jobs(None, false, Some(3), false, &db).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_empty_result_when_no_jobs() {
        let (db, _tmp) = setup();
        let jobs = query_jobs(None, false, None, true, &db).unwrap();
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_invalid_status_string_errors() {
        let (db, _tmp) = setup();
        let result = query_jobs(Some("bogus".into()), false, None, true, &db);
        assert!(result.is_err());
    }

    #[test]
    fn test_failed_flag_overrides_status_filter() {
        let (db, _tmp) = setup();
        db.insert(&job("a", Status::Failed)).unwrap();
        db.insert(&job("b", Status::Running)).unwrap();

        let jobs = query_jobs(Some("running".into()), true, None, true, &db).unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "a");
    }
}
