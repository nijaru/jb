use anyhow::Result;
use chrono::Utc;
use jb_core::{Database, Paths, Status, parse_duration};

pub async fn execute(older_than: String, status: Option<String>, all: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    let duration_secs = parse_duration(&older_than)?;
    #[allow(clippy::cast_possible_wrap)] // durations won't exceed i64::MAX
    let before = if all {
        Utc::now()
    } else {
        Utc::now() - chrono::Duration::seconds(duration_secs as i64)
    };

    let status_filter = status.map(|s| s.parse::<Status>()).transpose()?;

    let count = db.delete_old(before, status_filter)?;

    // Clean up orphaned log files
    let log_dir = paths.logs_dir();
    if log_dir.exists() {
        let jobs = db.list(None, None)?;
        let job_ids: std::collections::HashSet<_> = jobs.iter().map(|j| j.id.as_str()).collect();

        for entry in std::fs::read_dir(&log_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && !job_ids.contains(stem)
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    println!("Removed {count} jobs");

    Ok(())
}

