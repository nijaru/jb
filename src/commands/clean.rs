use crate::core::{Database, Paths, Status, parse_duration};
use anyhow::Result;
use chrono::Utc;

pub fn execute(older_than: &str, status: Option<String>, all: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;

    let duration_secs = parse_duration(older_than)?;
    #[allow(clippy::cast_possible_wrap)] // durations won't exceed i64::MAX
    let before = if all {
        Utc::now()
    } else {
        Utc::now() - chrono::Duration::seconds(duration_secs as i64)
    };

    let status_filter = status.map(|s| s.parse::<Status>()).transpose()?;

    let count = db.delete_old(before, status_filter)?;

    // Clean up orphaned log files. Query the DB per-file rather than taking a
    // snapshot upfront, so a job spawned between the delete and the scan can't
    // have its log removed before the daemon opens it.
    let log_dir = paths.logs_dir();
    if log_dir.exists() {
        for entry in std::fs::read_dir(&log_dir)? {
            let entry = entry?;
            let path = entry.path();
            if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
                && db.get(stem)?.is_none()
            {
                let _ = std::fs::remove_file(&path);
            }
        }
    }

    if all {
        println!("Removed {count} non-running jobs");
    } else {
        println!("Removed {count} jobs older than {older_than}");
    }

    Ok(())
}
