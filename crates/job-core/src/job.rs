use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use ulid::Ulid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Status {
    Pending,
    Running,
    Completed,
    Failed,
    Stopped,
    Interrupted,
}

impl Status {
    pub fn as_str(&self) -> &'static str {
        match self {
            Status::Pending => "pending",
            Status::Running => "running",
            Status::Completed => "completed",
            Status::Failed => "failed",
            Status::Stopped => "stopped",
            Status::Interrupted => "interrupted",
        }
    }

    pub fn is_terminal(&self) -> bool {
        matches!(
            self,
            Status::Completed | Status::Failed | Status::Stopped | Status::Interrupted
        )
    }
}

impl std::fmt::Display for Status {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

impl std::str::FromStr for Status {
    type Err = anyhow::Error;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "pending" => Ok(Status::Pending),
            "running" => Ok(Status::Running),
            "completed" => Ok(Status::Completed),
            "failed" => Ok(Status::Failed),
            "stopped" => Ok(Status::Stopped),
            "interrupted" => Ok(Status::Interrupted),
            _ => anyhow::bail!("unknown status: {}", s),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Job {
    pub id: String,
    pub name: Option<String>,
    pub command: String,
    pub status: Status,
    pub project: PathBuf,
    pub cwd: PathBuf,
    pub pid: Option<u32>,
    pub exit_code: Option<i32>,
    pub created_at: DateTime<Utc>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub timeout_secs: Option<u64>,
    pub context: Option<serde_json::Value>,
    pub idempotency_key: Option<String>,
}

impl Job {
    pub fn new(command: String, cwd: PathBuf, project: PathBuf) -> Self {
        Self {
            id: Ulid::new().to_string(),
            name: None,
            command,
            status: Status::Pending,
            project,
            cwd,
            pid: None,
            exit_code: None,
            created_at: Utc::now(),
            started_at: None,
            finished_at: None,
            timeout_secs: None,
            context: None,
            idempotency_key: None,
        }
    }

    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    pub fn short_id(&self) -> &str {
        &self.id[..8]
    }
}
