use crate::core::Paths;
use crate::core::error::UserError;
use crate::core::job::{Job, Status};
use anyhow::{Result, bail};
use rand::Rng;
use rusqlite::{Connection, OptionalExtension, params};
use std::path::PathBuf;

pub struct Database {
    conn: Connection,
}

impl Database {
    pub fn open(paths: &Paths) -> Result<Self> {
        paths.ensure_dirs()?;
        let conn = Connection::open(paths.database())?;
        let db = Self { conn };
        db.init_schema()?;
        Ok(db)
    }

    fn init_schema(&self) -> Result<()> {
        self.conn.execute_batch(
            r"
            CREATE TABLE IF NOT EXISTS jobs (
                id TEXT PRIMARY KEY,
                name TEXT,
                command TEXT NOT NULL,
                status TEXT NOT NULL,
                project TEXT NOT NULL,
                cwd TEXT NOT NULL,
                pid INTEGER,
                exit_code INTEGER,
                created_at TEXT NOT NULL,
                started_at TEXT,
                finished_at TEXT,
                timeout_secs INTEGER,
                context TEXT,
                idempotency_key TEXT UNIQUE
            );

            CREATE INDEX IF NOT EXISTS idx_jobs_status ON jobs(status);
            CREATE INDEX IF NOT EXISTS idx_jobs_project ON jobs(project);
            CREATE INDEX IF NOT EXISTS idx_jobs_created_at ON jobs(created_at);
            ",
        )?;
        Ok(())
    }

    pub fn insert(&self, job: &Job) -> Result<()> {
        self.conn.execute(
            r"
            INSERT INTO jobs (
                id, name, command, status, project, cwd, pid, exit_code,
                created_at, started_at, finished_at, timeout_secs, context, idempotency_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14)
            ",
            params![
                job.id,
                job.name,
                job.command,
                job.status.as_str(),
                job.project.to_string_lossy(),
                job.cwd.to_string_lossy(),
                job.pid,
                job.exit_code,
                job.created_at.to_rfc3339(),
                job.started_at.map(|t| t.to_rfc3339()),
                job.finished_at.map(|t| t.to_rfc3339()),
                job.timeout_secs,
                job.context.as_ref().map(std::string::ToString::to_string),
                job.idempotency_key,
            ],
        )?;
        Ok(())
    }

    pub fn get(&self, id: &str) -> Result<Option<Job>> {
        let job = self
            .conn
            .query_row(
                "SELECT * FROM jobs WHERE id = ?1 OR id LIKE ?2 || '%'",
                params![id, id],
                Self::row_to_job,
            )
            .optional()?;
        Ok(job)
    }

    pub fn get_by_name(&self, name: &str) -> Result<Vec<Job>> {
        let mut stmt = self.conn.prepare("SELECT * FROM jobs WHERE name = ?1")?;
        let jobs = stmt
            .query_map(params![name], Self::row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    /// Check if a name is in use by an active (pending/running) job. Returns the job if so.
    pub fn name_in_use(&self, name: &str) -> Result<Option<Job>> {
        let job = self
            .conn
            .query_row(
                "SELECT * FROM jobs WHERE name = ?1 AND status IN ('pending', 'running')",
                params![name],
                Self::row_to_job,
            )
            .optional()?;
        Ok(job)
    }

    pub fn get_by_idempotency_key(&self, key: &str) -> Result<Option<Job>> {
        let job = self
            .conn
            .query_row(
                "SELECT * FROM jobs WHERE idempotency_key = ?1",
                params![key],
                Self::row_to_job,
            )
            .optional()?;
        Ok(job)
    }

    pub fn list(&self, status: Option<Status>, limit: Option<usize>) -> Result<Vec<Job>> {
        let mut sql = String::from("SELECT * FROM jobs WHERE 1=1");
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = Vec::new();

        if let Some(s) = status {
            sql.push_str(" AND status = ?");
            params_vec.push(Box::new(s.as_str().to_string()));
        }

        sql.push_str(" ORDER BY created_at DESC");

        if let Some(n) = limit {
            use std::fmt::Write;
            let _ = write!(sql, " LIMIT {n}");
        }

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let jobs = stmt
            .query_map(params_refs.as_slice(), Self::row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    pub fn update_status(&self, id: &str, status: Status) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = ?1 WHERE id = ?2",
            params![status.as_str(), id],
        )?;
        Ok(())
    }

    pub fn update_started(&self, id: &str, pid: u32) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'running', started_at = ?1, pid = ?2 WHERE id = ?3",
            params![chrono::Utc::now().to_rfc3339(), pid, id],
        )?;
        Ok(())
    }

    pub fn update_finished(&self, id: &str, status: Status, exit_code: Option<i32>) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = ?1, finished_at = ?2, exit_code = ?3 WHERE id = ?4",
            params![
                status.as_str(),
                chrono::Utc::now().to_rfc3339(),
                exit_code,
                id
            ],
        )?;
        Ok(())
    }

    pub fn delete_old(
        &self,
        before: chrono::DateTime<chrono::Utc>,
        status: Option<Status>,
    ) -> Result<usize> {
        let mut sql = String::from(
            "DELETE FROM jobs WHERE created_at < ?1 AND status IN ('completed', 'failed', 'stopped', 'interrupted')",
        );
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(before.to_rfc3339())];

        if let Some(s) = status {
            sql = String::from("DELETE FROM jobs WHERE created_at < ?1 AND status = ?2");
            params_vec.push(Box::new(s.as_str().to_string()));
        }

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let count = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(count)
    }

    fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
        Ok(Job {
            id: row.get("id")?,
            name: row.get("name")?,
            command: row.get("command")?,
            status: row
                .get::<_, String>("status")?
                .parse()
                .unwrap_or(Status::Interrupted),
            project: PathBuf::from(row.get::<_, String>("project")?),
            cwd: PathBuf::from(row.get::<_, String>("cwd")?),
            pid: row.get("pid")?,
            exit_code: row.get("exit_code")?,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
                .map_or_else(|_| chrono::Utc::now(), |t| t.with_timezone(&chrono::Utc)),
            started_at: row
                .get::<_, Option<String>>("started_at")?
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|t| t.with_timezone(&chrono::Utc)),
            finished_at: row
                .get::<_, Option<String>>("finished_at")?
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(&s).ok())
                .map(|t| t.with_timezone(&chrono::Utc)),
            timeout_secs: row.get("timeout_secs")?,
            context: row
                .get::<_, Option<String>>("context")?
                .and_then(|s| serde_json::from_str(&s).ok()),
            idempotency_key: row.get("idempotency_key")?,
        })
    }

    pub fn job_exists(&self, id: &str) -> Result<bool> {
        let count: i64 = self.conn.query_row(
            "SELECT COUNT(*) FROM jobs WHERE id = ?1",
            params![id],
            |row| row.get(0),
        )?;
        Ok(count > 0)
    }

    pub fn count(&self, status: Option<Status>) -> Result<usize> {
        let (sql, params_vec): (&str, Vec<Box<dyn rusqlite::ToSql>>) = match status {
            Some(s) => (
                "SELECT COUNT(*) FROM jobs WHERE status = ?1",
                vec![Box::new(s.as_str().to_string())],
            ),
            None => ("SELECT COUNT(*) FROM jobs", vec![]),
        };

        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();

        let count: i64 = self
            .conn
            .query_row(sql, params_refs.as_slice(), |row| row.get(0))?;
        #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
        Ok(count as usize)
    }

    /// Resolve a job by ID or name.
    /// For names, returns the most recent job with that name.
    pub fn resolve(&self, id: &str) -> Result<Job> {
        // Try by ID first
        if let Some(job) = self.get(id)? {
            return Ok(job);
        }

        // Try by name - get most recent
        let mut by_name = self.get_by_name(id)?;
        if by_name.is_empty() {
            bail!(UserError::new(format!(
                "No job found with ID or name '{id}'"
            )));
        }

        // Sort by created_at desc (newest first) and return first
        by_name.sort_by(|a, b| b.created_at.cmp(&a.created_at));
        Ok(by_name.into_iter().next().unwrap())
    }

    pub fn generate_id(&self) -> Result<String> {
        const CHARS: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyz";
        let mut rng = rand::rng();

        for _ in 0..100 {
            let id: String = (0..4)
                .map(|_| CHARS[rng.random_range(0..36)] as char)
                .collect();
            if !self.job_exists(&id)? {
                return Ok(id);
            }
        }

        bail!("Too many jobs - run `jb clean` to remove old jobs")
    }

    /// Check for orphaned jobs (running/pending but process dead) and mark as interrupted.
    /// Called on DB open to handle daemon crashes.
    pub fn recover_orphans(&self) {
        let orphans = self
            .list(Some(Status::Running), None)
            .unwrap_or_default()
            .into_iter()
            .chain(self.list(Some(Status::Pending), None).unwrap_or_default());

        for job in orphans {
            if let Some(pid) = job.pid
                && is_process_alive(pid)
            {
                // Process still running - leave as is
                continue;
            }
            // Process dead or no PID - mark as interrupted
            let _ = self.update_finished(&job.id, Status::Interrupted, None);
        }
    }
}

/// Check if a process is still alive by sending signal 0.
#[cfg(unix)]
fn is_process_alive(pid: u32) -> bool {
    use nix::sys::signal::kill;
    use nix::unistd::Pid;

    if pid == 0 {
        return false;
    }

    #[allow(clippy::cast_possible_wrap)]
    kill(Pid::from_raw(pid as i32), None).is_ok()
}

#[cfg(not(unix))]
fn is_process_alive(_pid: u32) -> bool {
    false
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::job::Job;
    use tempfile::TempDir;

    fn test_db() -> (Database, TempDir) {
        let tmp = TempDir::new().unwrap();
        let paths = Paths::with_root(tmp.path().to_path_buf());
        let db = Database::open(&paths).unwrap();
        (db, tmp)
    }

    fn create_test_job(id: &str, status: Status) -> Job {
        let mut job = Job::new(
            id.to_string(),
            format!("echo {id}"),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        );
        job.status = status;
        job
    }

    #[test]
    fn test_insert_and_get() {
        let (db, _tmp) = test_db();
        let job = create_test_job("abc1", Status::Pending);

        db.insert(&job).unwrap();
        let retrieved = db.get("abc1").unwrap().unwrap();

        assert_eq!(retrieved.id, "abc1");
        assert_eq!(retrieved.status, Status::Pending);
    }

    #[test]
    fn test_get_nonexistent() {
        let (db, _tmp) = test_db();
        let result = db.get("nonexistent").unwrap();
        assert!(result.is_none());
    }

    #[test]
    fn test_get_by_prefix() {
        let (db, _tmp) = test_db();
        let job = create_test_job("xyz9", Status::Running);
        db.insert(&job).unwrap();

        // Full ID works
        assert!(db.get("xyz9").unwrap().is_some());
        // Prefix works
        assert!(db.get("xyz").unwrap().is_some());
        assert!(db.get("xy").unwrap().is_some());
    }

    #[test]
    fn test_get_by_name() {
        let (db, _tmp) = test_db();
        let job = create_test_job("abc1", Status::Running).with_name("my-job");
        db.insert(&job).unwrap();

        let jobs = db.get_by_name("my-job").unwrap();
        assert_eq!(jobs.len(), 1);
        assert_eq!(jobs[0].id, "abc1");

        let jobs = db.get_by_name("nonexistent").unwrap();
        assert!(jobs.is_empty());
    }

    #[test]
    fn test_list_no_filter() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Running)).unwrap();
        db.insert(&create_test_job("b", Status::Completed)).unwrap();
        db.insert(&create_test_job("c", Status::Failed)).unwrap();

        let jobs = db.list(None, None).unwrap();
        assert_eq!(jobs.len(), 3);
    }

    #[test]
    fn test_list_with_status_filter() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Running)).unwrap();
        db.insert(&create_test_job("b", Status::Running)).unwrap();
        db.insert(&create_test_job("c", Status::Failed)).unwrap();
        db.insert(&create_test_job("d", Status::Completed)).unwrap();

        let running = db.list(Some(Status::Running), None).unwrap();
        assert_eq!(running.len(), 2);

        let failed = db.list(Some(Status::Failed), None).unwrap();
        assert_eq!(failed.len(), 1);
        assert_eq!(failed[0].id, "c");
    }

    #[test]
    fn test_list_with_limit() {
        let (db, _tmp) = test_db();
        for i in 0..20 {
            db.insert(&create_test_job(&format!("job{i:02}"), Status::Completed))
                .unwrap();
        }

        let jobs = db.list(None, Some(10)).unwrap();
        assert_eq!(jobs.len(), 10);

        let jobs = db.list(None, Some(5)).unwrap();
        assert_eq!(jobs.len(), 5);
    }

    #[test]
    fn test_list_with_status_and_limit() {
        let (db, _tmp) = test_db();
        for i in 0..10 {
            db.insert(&create_test_job(&format!("f{i}"), Status::Failed))
                .unwrap();
        }
        for i in 0..10 {
            db.insert(&create_test_job(&format!("c{i}"), Status::Completed))
                .unwrap();
        }

        let failed = db.list(Some(Status::Failed), Some(3)).unwrap();
        assert_eq!(failed.len(), 3);
        assert!(failed.iter().all(|j| j.status == Status::Failed));
    }

    #[test]
    fn test_list_ordered_by_created_at_desc() {
        let (db, _tmp) = test_db();
        // Insert in order a, b, c
        db.insert(&create_test_job("a", Status::Completed)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.insert(&create_test_job("b", Status::Completed)).unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.insert(&create_test_job("c", Status::Completed)).unwrap();

        let jobs = db.list(None, None).unwrap();
        // Should be in reverse order (newest first)
        assert_eq!(jobs[0].id, "c");
        assert_eq!(jobs[1].id, "b");
        assert_eq!(jobs[2].id, "a");
    }

    #[test]
    fn test_update_status() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Pending))
            .unwrap();

        db.update_status("abc1", Status::Running).unwrap();
        let job = db.get("abc1").unwrap().unwrap();
        assert_eq!(job.status, Status::Running);
    }

    #[test]
    fn test_update_started() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Pending))
            .unwrap();

        db.update_started("abc1", 12345).unwrap();
        let job = db.get("abc1").unwrap().unwrap();
        assert_eq!(job.status, Status::Running);
        assert_eq!(job.pid, Some(12345));
        assert!(job.started_at.is_some());
    }

    #[test]
    fn test_update_finished() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Running))
            .unwrap();

        db.update_finished("abc1", Status::Completed, Some(0))
            .unwrap();
        let job = db.get("abc1").unwrap().unwrap();
        assert_eq!(job.status, Status::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert!(job.finished_at.is_some());
    }

    #[test]
    fn test_job_exists() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Pending))
            .unwrap();

        assert!(db.job_exists("abc1").unwrap());
        assert!(!db.job_exists("xyz9").unwrap());
    }

    #[test]
    fn test_generate_id() {
        let (db, _tmp) = test_db();
        let id = db.generate_id().unwrap();

        assert_eq!(id.len(), 4);
        assert!(id.chars().all(|c| c.is_ascii_alphanumeric()));
    }

    #[test]
    fn test_generate_id_unique() {
        let (db, _tmp) = test_db();
        let mut ids = std::collections::HashSet::new();

        for _ in 0..100 {
            let id = db.generate_id().unwrap();
            assert!(ids.insert(id), "Generated duplicate ID");
        }
    }

    #[test]
    fn test_idempotency_key() {
        let (db, _tmp) = test_db();
        let job = create_test_job("abc1", Status::Pending).with_idempotency_key("unique-key");
        db.insert(&job).unwrap();

        let found = db.get_by_idempotency_key("unique-key").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, "abc1");

        let not_found = db.get_by_idempotency_key("other-key").unwrap();
        assert!(not_found.is_none());
    }

    #[test]
    fn test_idempotency_key_unique_constraint() {
        let (db, _tmp) = test_db();
        let job1 = create_test_job("abc1", Status::Pending).with_idempotency_key("same-key");
        let job2 = create_test_job("abc2", Status::Pending).with_idempotency_key("same-key");

        db.insert(&job1).unwrap();
        assert!(db.insert(&job2).is_err());
    }

    #[test]
    fn test_count_all() {
        let (db, _tmp) = test_db();
        assert_eq!(db.count(None).unwrap(), 0);

        db.insert(&create_test_job("a", Status::Running)).unwrap();
        db.insert(&create_test_job("b", Status::Completed)).unwrap();
        db.insert(&create_test_job("c", Status::Failed)).unwrap();

        assert_eq!(db.count(None).unwrap(), 3);
    }

    #[test]
    fn test_count_by_status() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Running)).unwrap();
        db.insert(&create_test_job("b", Status::Running)).unwrap();
        db.insert(&create_test_job("c", Status::Failed)).unwrap();

        assert_eq!(db.count(Some(Status::Running)).unwrap(), 2);
        assert_eq!(db.count(Some(Status::Failed)).unwrap(), 1);
        assert_eq!(db.count(Some(Status::Completed)).unwrap(), 0);
    }

    #[test]
    fn test_resolve_by_id() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Running))
            .unwrap();

        let job = db.resolve("abc1").unwrap();
        assert_eq!(job.id, "abc1");

        // Prefix also works
        let job = db.resolve("abc").unwrap();
        assert_eq!(job.id, "abc1");
    }

    #[test]
    fn test_resolve_by_name() {
        let (db, _tmp) = test_db();
        let job = create_test_job("abc1", Status::Running).with_name("my-job");
        db.insert(&job).unwrap();

        let resolved = db.resolve("my-job").unwrap();
        assert_eq!(resolved.id, "abc1");
    }

    #[test]
    fn test_resolve_not_found() {
        let (db, _tmp) = test_db();
        let result = db.resolve("nonexistent");
        assert!(result.is_err());
    }

    #[test]
    fn test_resolve_returns_most_recent() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Completed).with_name("same-name"))
            .unwrap();
        std::thread::sleep(std::time::Duration::from_millis(10));
        db.insert(&create_test_job("b", Status::Completed).with_name("same-name"))
            .unwrap();

        // Should pick the most recent (b)
        let result = db.resolve("same-name");
        assert!(result.is_ok());
        assert_eq!(result.unwrap().id, "b");
    }

    #[test]
    fn test_name_in_use_running() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Running).with_name("my-job"))
            .unwrap();
        db.insert(&create_test_job("b", Status::Completed).with_name("my-job"))
            .unwrap();

        // Running job should be detected as in-use
        let in_use = db.name_in_use("my-job").unwrap();
        assert!(in_use.is_some());
        assert_eq!(in_use.unwrap().id, "a");

        // Non-existent name should not be in use
        let not_in_use = db.name_in_use("other-name").unwrap();
        assert!(not_in_use.is_none());
    }

    #[test]
    fn test_name_in_use_pending() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Pending).with_name("my-job"))
            .unwrap();

        // Pending job should also be detected as in-use
        let in_use = db.name_in_use("my-job").unwrap();
        assert!(in_use.is_some());
        assert_eq!(in_use.unwrap().id, "a");
    }

    #[test]
    fn test_name_not_in_use_when_completed() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("a", Status::Completed).with_name("my-job"))
            .unwrap();

        // Completed job should NOT be detected as in-use
        let in_use = db.name_in_use("my-job").unwrap();
        assert!(in_use.is_none());
    }
}
