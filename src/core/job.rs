use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

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
    #[must_use]
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Running => "running",
            Self::Completed => "completed",
            Self::Failed => "failed",
            Self::Stopped => "stopped",
            Self::Interrupted => "interrupted",
        }
    }

    #[must_use]
    pub fn is_terminal(self) -> bool {
        matches!(
            self,
            Self::Completed | Self::Failed | Self::Stopped | Self::Interrupted
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
            _ => anyhow::bail!("unknown status: {s}"),
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
    #[must_use]
    pub fn new(id: String, command: String, cwd: PathBuf, project: PathBuf) -> Self {
        Self {
            id,
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

    #[must_use]
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }

    #[must_use]
    pub fn with_timeout(mut self, secs: u64) -> Self {
        self.timeout_secs = Some(secs);
        self
    }

    #[must_use]
    pub fn with_context(mut self, context: serde_json::Value) -> Self {
        self.context = Some(context);
        self
    }

    #[must_use]
    pub fn with_idempotency_key(mut self, key: impl Into<String>) -> Self {
        self.idempotency_key = Some(key.into());
        self
    }

    #[must_use]
    pub fn short_id(&self) -> &str {
        &self.id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_status_as_str() {
        assert_eq!(Status::Pending.as_str(), "pending");
        assert_eq!(Status::Running.as_str(), "running");
        assert_eq!(Status::Completed.as_str(), "completed");
        assert_eq!(Status::Failed.as_str(), "failed");
        assert_eq!(Status::Stopped.as_str(), "stopped");
        assert_eq!(Status::Interrupted.as_str(), "interrupted");
    }

    #[test]
    fn test_status_is_terminal() {
        assert!(!Status::Pending.is_terminal());
        assert!(!Status::Running.is_terminal());
        assert!(Status::Completed.is_terminal());
        assert!(Status::Failed.is_terminal());
        assert!(Status::Stopped.is_terminal());
        assert!(Status::Interrupted.is_terminal());
    }

    #[test]
    fn test_status_from_str() {
        assert_eq!("pending".parse::<Status>().unwrap(), Status::Pending);
        assert_eq!("running".parse::<Status>().unwrap(), Status::Running);
        assert_eq!("completed".parse::<Status>().unwrap(), Status::Completed);
        assert_eq!("failed".parse::<Status>().unwrap(), Status::Failed);
        assert_eq!("stopped".parse::<Status>().unwrap(), Status::Stopped);
        assert_eq!(
            "interrupted".parse::<Status>().unwrap(),
            Status::Interrupted
        );
    }

    #[test]
    fn test_status_from_str_case_insensitive() {
        assert_eq!("PENDING".parse::<Status>().unwrap(), Status::Pending);
        assert_eq!("Running".parse::<Status>().unwrap(), Status::Running);
        assert_eq!("COMPLETED".parse::<Status>().unwrap(), Status::Completed);
    }

    #[test]
    fn test_status_from_str_invalid() {
        assert!("invalid".parse::<Status>().is_err());
        assert!("".parse::<Status>().is_err());
    }

    #[test]
    fn test_status_display() {
        assert_eq!(format!("{}", Status::Running), "running");
        assert_eq!(format!("{}", Status::Failed), "failed");
    }

    #[test]
    fn test_job_new() {
        let job = Job::new(
            "abc1".to_string(),
            "echo hello".to_string(),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        );

        assert_eq!(job.id, "abc1");
        assert_eq!(job.command, "echo hello");
        assert_eq!(job.status, Status::Pending);
        assert_eq!(job.cwd, PathBuf::from("/tmp"));
        assert_eq!(job.project, PathBuf::from("/project"));
        assert!(job.name.is_none());
        assert!(job.pid.is_none());
        assert!(job.exit_code.is_none());
    }

    #[test]
    fn test_job_builder_methods() {
        let job = Job::new(
            "abc1".to_string(),
            "echo hello".to_string(),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        )
        .with_name("test-job")
        .with_timeout(300)
        .with_idempotency_key("unique-key");

        assert_eq!(job.name, Some("test-job".to_string()));
        assert_eq!(job.timeout_secs, Some(300));
        assert_eq!(job.idempotency_key, Some("unique-key".to_string()));
    }

    #[test]
    fn test_job_short_id() {
        let job = Job::new(
            "xyz9".to_string(),
            "cmd".to_string(),
            PathBuf::from("/tmp"),
            PathBuf::from("/project"),
        );
        assert_eq!(job.short_id(), "xyz9");
    }
}
