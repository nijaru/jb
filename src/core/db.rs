use crate::core::Paths;
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
            sql.push_str(&format!(" LIMIT {n}"));
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
}
