use crate::Job;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Run {
        command: String,
        name: Option<String>,
        cwd: String,
        project: String,
        timeout_secs: Option<u64>,
        context: Option<serde_json::Value>,
        idempotency_key: Option<String>,
    },
    Stop {
        id: String,
        force: bool,
    },
    Status {
        id: String,
    },
    List {
        status: Option<String>,
        project: Option<String>,
    },
    Wait {
        id: String,
        timeout_secs: Option<u64>,
    },
    Ping,
    Shutdown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Response {
    Job(Job),
    Jobs(Vec<Job>),
    Ok,
    Error(String),
    Pong {
        pid: u32,
        uptime_secs: u64,
        running_jobs: usize,
        total_jobs: usize,
    },
}
