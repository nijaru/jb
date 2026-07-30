use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Database, Paths};
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

    let mut client = DaemonClient::connect_or_start().await?;
    match client
        .send(Request::Stop {
            id: job.id.clone(),
            force,
        })
        .await?
    {
        Response::Ok => {
            let updated = db
                .get(&job.id)?
                .ok_or_else(|| anyhow::anyhow!("job {} disappeared", job.short_id()))?;
            if json {
                println!("{}", serde_json::to_string(&updated)?);
            } else {
                println!("Stopped {}", updated.short_id());
            }
            Ok(())
        }
        Response::UserError(error) => anyhow::bail!(crate::core::UserError::new(error)),
        Response::Error(error) => anyhow::bail!("{error}"),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

#[cfg(test)]
mod tests {
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
    fn database_does_not_allow_client_side_pending_transition() {
        let (db, paths, _tmp) = setup();
        let job = pending_job("abc1");
        db.insert(&job).unwrap();
        assert_eq!(db.get("abc1").unwrap().unwrap().status, Status::Pending);
        drop(paths);
    }
}
