use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Paths, Status, kill_process_group};
use anyhow::Result;

pub async fn execute(id: String, force: bool, json: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;
    let job = db.resolve(&id)?;

    if job.status.is_terminal() {
        if json {
            println!("{}", serde_json::to_string(&job)?);
        } else {
            println!("Job already {}", job.status);
        }
        return Ok(());
    }

    // Try to stop via daemon
    if let Ok(mut client) = DaemonClient::connect_or_start().await {
        let request = Request::Stop {
            id: job.id.clone(),
            force,
        };

        match client.send(request).await? {
            Response::Ok => {
                if json {
                    let updated = db
                        .get(&job.id)?
                        .ok_or_else(|| anyhow::anyhow!("job {} disappeared", job.short_id()))?;
                    println!("{}", serde_json::to_string(&updated)?);
                } else {
                    println!("Stopped {}", job.short_id());
                }
                return Ok(());
            }
            Response::UserError(_) => {
                // Job not running in daemon — fall back to direct kill below
            }
            Response::Error(e) => {
                anyhow::bail!("{e}");
            }
            _ => {}
        }
    }

    // Fallback: direct kill (for jobs started before daemon)
    stop_without_daemon(&job, &db, force)?;

    if json {
        let updated = db
            .get(&job.id)?
            .ok_or_else(|| anyhow::anyhow!("job {} disappeared", job.short_id()))?;
        println!("{}", serde_json::to_string(&updated)?);
    } else {
        println!("Stopped {}", job.short_id());
    }

    Ok(())
}

/// Direct stop path: used when daemon is unreachable or job was pre-daemon.
/// Pending jobs: mark Stopped in DB. Running jobs: kill process group + mark Stopped.
fn stop_without_daemon(job: &crate::core::Job, db: &crate::core::Database, force: bool) -> Result<()> {
    if job.status == Status::Pending {
        db.update_status(&job.id, Status::Stopped)?;
    } else if let Some(pid) = job.pid {
        kill_process_group(pid, force);
        db.update_finished(&job.id, Status::Stopped, None)?;
    }
    Ok(())
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
        let db = Database::open(&paths).unwrap();
        (db, paths, tmp)
    }

    fn pending_job(id: &str) -> Job {
        Job::new(
            id.into(),
            "sleep 60".into(),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        )
    }

    #[test]
    fn test_stop_pending_job_marks_stopped() {
        let (db, paths, _tmp) = setup();
        let job = pending_job("abc1");
        db.insert(&job).unwrap();

        stop_without_daemon(&job, &db, false).unwrap();

        let updated = db.get("abc1").unwrap().unwrap();
        assert_eq!(updated.status, Status::Stopped);
        drop(paths);
    }

    #[test]
    fn test_stop_job_without_pid_is_noop() {
        let (db, paths, _tmp) = setup();
        let mut job = pending_job("abc1");
        job.status = Status::Running;
        job.pid = None;
        db.insert(&job).unwrap();

        stop_without_daemon(&job, &db, false).unwrap();

        let updated = db.get("abc1").unwrap().unwrap();
        assert_eq!(updated.status, Status::Running);
        drop(paths);
    }
}
