use crate::core::{Database, Paths, Status, UserError, parse_duration};
use anyhow::Result;
use chrono::Utc;

pub fn execute(older_than: &str, status: Option<String>, all: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;
    let count = delete_jobs(older_than, status, all, &db, &paths)?;
    if all {
        println!("Removed {count} non-running jobs");
    } else {
        println!("Removed {count} jobs older than {older_than}");
    }
    Ok(())
}

fn delete_jobs(
    older_than: &str,
    status: Option<String>,
    all: bool,
    db: &Database,
    paths: &Paths,
) -> Result<usize> {
    let duration_secs = parse_duration(older_than)?;
    #[allow(clippy::cast_possible_wrap)] // durations won't exceed i64::MAX
    let before = if all {
        Utc::now()
    } else {
        Utc::now() - chrono::Duration::seconds(duration_secs as i64)
    };

    let status_filter = status.map(|s| s.parse::<Status>()).transpose()?;

    if let Some(s) = &status_filter
        && !s.is_terminal()
    {
        anyhow::bail!(UserError::new(format!(
            "cannot clean jobs with status '{s}': only terminal statuses allowed (completed, failed, stopped, interrupted)"
        )));
    }

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

    Ok(count)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::{Database, Job, Paths, Status};
    use std::path::PathBuf;
    use tempfile::TempDir;

    fn setup() -> (Database, Paths, TempDir) {
        let tmp = TempDir::new().unwrap();
        let paths = Paths::with_root(tmp.path().to_path_buf());
        paths.ensure_dirs().unwrap();
        let db = Database::open(&paths).unwrap();
        (db, paths, tmp)
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

    fn old_job(id: &str, status: Status) -> Job {
        let mut j = job(id, status);
        j.created_at = Utc::now() - chrono::Duration::days(10);
        j
    }

    #[test]
    fn test_rejects_running_status() {
        let (db, paths, _tmp) = setup();
        let err = delete_jobs("1d", Some("running".into()), false, &db, &paths).unwrap_err();
        assert!(err.to_string().contains("cannot clean"), "error: {err}");
    }

    #[test]
    fn test_rejects_pending_status() {
        let (db, paths, _tmp) = setup();
        let err = delete_jobs("1d", Some("pending".into()), false, &db, &paths).unwrap_err();
        assert!(err.to_string().contains("cannot clean"), "error: {err}");
    }

    #[test]
    fn test_deletes_old_completed_job() {
        let (db, paths, _tmp) = setup();
        db.insert(&old_job("abc1", Status::Completed)).unwrap();

        let count = delete_jobs("7d", None, false, &db, &paths).unwrap();

        assert_eq!(count, 1);
        assert!(db.get("abc1").unwrap().is_none(), "job should be deleted");
    }

    #[test]
    fn test_does_not_delete_recent_job() {
        let (db, paths, _tmp) = setup();
        db.insert(&job("abc1", Status::Completed)).unwrap();

        let count = delete_jobs("7d", None, false, &db, &paths).unwrap();

        assert_eq!(count, 0);
        assert!(
            db.get("abc1").unwrap().is_some(),
            "recent job should survive"
        );
    }

    #[test]
    fn test_all_flag_deletes_terminal_jobs_regardless_of_age() {
        let (db, paths, _tmp) = setup();
        db.insert(&job("abc1", Status::Completed)).unwrap();
        db.insert(&job("abc2", Status::Running)).unwrap();

        let count = delete_jobs("7d", None, true, &db, &paths).unwrap();

        assert_eq!(count, 1, "only completed should be deleted");
        assert!(db.get("abc1").unwrap().is_none());
        assert!(
            db.get("abc2").unwrap().is_some(),
            "running job should survive"
        );
    }

    #[test]
    fn test_status_filter_deletes_only_matching() {
        let (db, paths, _tmp) = setup();
        db.insert(&old_job("abc1", Status::Failed)).unwrap();
        db.insert(&old_job("abc2", Status::Completed)).unwrap();

        let count = delete_jobs("7d", Some("failed".into()), false, &db, &paths).unwrap();

        assert_eq!(count, 1);
        assert!(db.get("abc1").unwrap().is_none(), "failed job deleted");
        assert!(db.get("abc2").unwrap().is_some(), "completed job preserved");
    }

    #[test]
    fn test_invalid_status_string_errors() {
        let (db, paths, _tmp) = setup();
        let result = delete_jobs("7d", Some("bogus".into()), false, &db, &paths);
        assert!(result.is_err());
    }

    #[test]
    fn test_orphaned_log_file_removed() {
        let (db, paths, _tmp) = setup();
        paths.ensure_dirs().unwrap();
        let log = paths.log_file("xxxx");
        std::fs::write(&log, "stale log").unwrap();

        delete_jobs("7d", None, false, &db, &paths).unwrap();

        assert!(!log.exists(), "orphaned log file should be cleaned up");
    }
}
