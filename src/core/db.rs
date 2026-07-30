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
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        paths.secure_file(&paths.database())?;
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
                created_at, started_at, finished_at, timeout_secs, idempotency_key
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13)
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
                job.idempotency_key,
            ],
        )?;
        Ok(())
    }

    /// Look up one job by its complete ID.
    pub fn get(&self, id: &str) -> Result<Option<Job>> {
        Ok(self
            .conn
            .query_row(
                "SELECT * FROM jobs WHERE id = ?1",
                params![id],
                Self::row_to_job,
            )
            .optional()?)
    }

    fn get_prefix(&self, prefix: &str) -> Result<Vec<Job>> {
        if prefix.contains(['%', '_', '\\']) {
            return Ok(Vec::new());
        }

        let mut stmt = self.conn.prepare(
            "SELECT * FROM jobs WHERE id LIKE ?1 || '%' ORDER BY created_at DESC LIMIT 2",
        )?;
        Ok(stmt
            .query_map(params![prefix], Self::row_to_job)?
            .collect::<Result<Vec<_>, _>>()?)
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

        // SQLite treats LIMIT -1 as no limit, so we can always parameterize
        sql.push_str(" ORDER BY created_at DESC LIMIT ?");
        #[allow(clippy::cast_possible_truncation)] // limits are always small
        let limit_val: i64 = limit.map_or(-1, |n| n as i64);
        params_vec.push(Box::new(limit_val));

        let mut stmt = self.conn.prepare(&sql)?;
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let jobs = stmt
            .query_map(params_refs.as_slice(), Self::row_to_job)?
            .collect::<Result<Vec<_>, _>>()?;
        Ok(jobs)
    }

    /// Claim a pending job for a child process.
    pub fn mark_running(&self, id: &str, pid: u32) -> Result<bool> {
        let updated = self.conn.execute(
            "UPDATE jobs SET status = 'running', started_at = ?1, pid = ?2 WHERE id = ?3 AND status = 'pending'",
            params![chrono::Utc::now().to_rfc3339(), pid, id],
        )?;
        Ok(updated > 0)
    }

    /// Publish a terminal state exactly once. Both pending setup failures and
    /// running children use this transition; terminal rows are immutable.
    pub fn finish(&self, id: &str, status: Status, exit_code: Option<i32>) -> Result<bool> {
        if !status.is_terminal() {
            bail!("cannot finish a job with non-terminal status {status}");
        }

        let updated = self.conn.execute(
            "UPDATE jobs SET status = ?1, finished_at = ?2, exit_code = ?3 WHERE id = ?4 AND status IN ('pending', 'running')",
            params![
                status.as_str(),
                chrono::Utc::now().to_rfc3339(),
                exit_code,
                id
            ],
        )?;
        Ok(updated > 0)
    }

    pub fn delete_old(
        &self,
        before: chrono::DateTime<chrono::Utc>,
        status: Option<Status>,
    ) -> Result<usize> {
        // If a specific status is requested, verify it's a terminal status; otherwise
        // match any terminal status. Both branches derive the set from Status::terminal_strs.
        let terminal = Status::terminal_strs();
        let mut params_vec: Vec<Box<dyn rusqlite::ToSql>> = vec![Box::new(before.to_rfc3339())];
        let status_clause = match status {
            Some(s) => {
                if !s.is_terminal() {
                    return Ok(0);
                }
                params_vec.push(Box::new(s.as_str().to_string()));
                "status = ?2".to_string()
            }
            None => {
                let placeholders = (2..2 + terminal.len())
                    .map(|i| format!("?{i}"))
                    .collect::<Vec<_>>()
                    .join(", ");
                for t in terminal {
                    params_vec.push(Box::new((*t).to_string()));
                }
                format!("status IN ({placeholders})")
            }
        };

        let sql = format!("DELETE FROM jobs WHERE created_at < ?1 AND {status_clause}");
        let params_refs: Vec<&dyn rusqlite::ToSql> =
            params_vec.iter().map(std::convert::AsRef::as_ref).collect();
        let count = self.conn.execute(&sql, params_refs.as_slice())?;
        Ok(count)
    }

    fn row_to_job(row: &rusqlite::Row) -> rusqlite::Result<Job> {
        let status_raw = row.get::<_, String>("status")?;
        let status = status_raw.parse::<Status>().map_err(|error| {
            rusqlite::Error::FromSqlConversionFailure(
                row.as_ref().column_index("status").unwrap_or(3),
                rusqlite::types::Type::Text,
                Box::new(std::io::Error::new(
                    std::io::ErrorKind::InvalidData,
                    error.to_string(),
                )),
            )
        })?;

        let parse_time = |value: Option<String>, column: &str| {
            value
                .map(|raw| {
                    chrono::DateTime::parse_from_rfc3339(&raw)
                        .map(|time| time.with_timezone(&chrono::Utc))
                        .map_err(|error| {
                            rusqlite::Error::FromSqlConversionFailure(
                                row.as_ref().column_index(column).unwrap_or(0),
                                rusqlite::types::Type::Text,
                                Box::new(error),
                            )
                        })
                })
                .transpose()
        };

        Ok(Job {
            id: row.get("id")?,
            name: row.get("name")?,
            command: row.get("command")?,
            status,
            project: PathBuf::from(row.get::<_, String>("project")?),
            cwd: PathBuf::from(row.get::<_, String>("cwd")?),
            pid: row.get("pid")?,
            exit_code: row.get("exit_code")?,
            created_at: chrono::DateTime::parse_from_rfc3339(&row.get::<_, String>("created_at")?)
                .map(|time| time.with_timezone(&chrono::Utc))
                .map_err(|error| {
                    rusqlite::Error::FromSqlConversionFailure(
                        row.as_ref().column_index("created_at").unwrap_or(8),
                        rusqlite::types::Type::Text,
                        Box::new(error),
                    )
                })?,
            started_at: parse_time(row.get("started_at")?, "started_at")?,
            finished_at: parse_time(row.get("finished_at")?, "finished_at")?,
            timeout_secs: row.get("timeout_secs")?,
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

    /// Resolve a user selector without silently choosing an ambiguous ID.
    /// Exact IDs win, followed by exact names (latest first), then unique IDs.
    pub fn resolve(&self, selector: &str) -> Result<Job> {
        if selector.is_empty() {
            bail!(UserError::new("job selector cannot be empty"));
        }

        if let Some(job) = self.get(selector)? {
            return Ok(job);
        }

        let mut by_name = self.get_by_name(selector)?;
        if !by_name.is_empty() {
            by_name.sort_by_key(|job| std::cmp::Reverse(job.created_at));
            return Ok(by_name.into_iter().next().expect("non-empty name results"));
        }

        let matches = self.get_prefix(selector)?;
        match matches.as_slice() {
            [] => bail!(UserError::new(format!(
                "No job found with ID or name '{selector}'"
            ))),
            [job] => Ok(job.clone()),
            _ => bail!(UserError::new(format!(
                "Job selector '{selector}' is ambiguous; use a full ID"
            ))),
        }
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

    /// Mark active rows as interrupted during daemon startup. This runs only
    /// while the daemon singleton lock is held, before it admits any jobs.
    pub fn recover_orphans(&self) -> Result<()> {
        self.conn.execute(
            "UPDATE jobs SET status = 'interrupted', finished_at = ?1, exit_code = NULL WHERE status IN ('pending', 'running')",
            params![chrono::Utc::now().to_rfc3339()],
        )?;
        Ok(())
    }
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
    fn test_resolve_unique_prefix() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("xyz9", Status::Running))
            .unwrap();

        assert_eq!(db.resolve("xyz9").unwrap().id, "xyz9");
        assert_eq!(db.resolve("xyz").unwrap().id, "xyz9");
        assert_eq!(db.resolve("xy").unwrap().id, "xyz9");
    }

    #[test]
    fn test_resolve_rejects_empty_and_ambiguous_prefixes() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abcd", Status::Completed))
            .unwrap();
        db.insert(&create_test_job("abce", Status::Completed))
            .unwrap();

        assert!(db.resolve("").is_err());
        let error = db.resolve("abc").unwrap_err().to_string();
        assert!(error.contains("ambiguous"), "error: {error}");
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
    fn test_mark_running_is_cas() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Pending))
            .unwrap();

        assert!(db.mark_running("abc1", 12345).unwrap());
        assert!(!db.mark_running("abc1", 12346).unwrap());
        let job = db.get("abc1").unwrap().unwrap();
        assert_eq!(job.status, Status::Running);
        assert_eq!(job.pid, Some(12345));
        assert!(job.started_at.is_some());
    }

    #[test]
    fn test_finish_is_cas_and_terminal_only() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Running))
            .unwrap();

        assert!(db.finish("abc1", Status::Completed, Some(0)).unwrap());
        assert!(!db.finish("abc1", Status::Failed, Some(1)).unwrap());
        let job = db.get("abc1").unwrap().unwrap();
        assert_eq!(job.status, Status::Completed);
        assert_eq!(job.exit_code, Some(0));
        assert!(job.finished_at.is_some());
    }

    #[test]
    fn test_recover_orphans_marks_active_rows_interrupted() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("run1", Status::Running))
            .unwrap();
        db.insert(&create_test_job("pend", Status::Pending))
            .unwrap();
        db.recover_orphans().unwrap();

        for id in ["run1", "pend"] {
            let job = db.get(id).unwrap().unwrap();
            assert_eq!(job.status, Status::Interrupted);
            assert!(job.finished_at.is_some());
        }
    }

    #[test]
    fn test_corrupt_status_is_rejected() {
        let (db, _tmp) = test_db();
        db.insert(&create_test_job("abc1", Status::Completed))
            .unwrap();
        db.conn
            .execute("UPDATE jobs SET status = 'future' WHERE id = 'abc1'", [])
            .unwrap();
        assert!(db.get("abc1").is_err());
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
