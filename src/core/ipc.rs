use crate::core::Job;
use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Request {
    Run {
        command: String,
        name: Option<String>,
        cwd: String,
        project: String,
        timeout_secs: Option<u64>,
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
        limit: Option<usize>,
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
    Job(Box<Job>),
    Jobs(Vec<Job>),
    Ok,
    Error(String),
    UserError(String),
    Pong {
        pid: u32,
        uptime_secs: u64,
        running_jobs: usize,
        total_jobs: usize,
    },
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip<T: serde::Serialize + for<'de> serde::Deserialize<'de>>(value: &T) {
        let json = serde_json::to_string(value).expect("serialize failed");
        let back: T = serde_json::from_str(&json).expect("deserialize failed");
        let json2 = serde_json::to_string(&back).expect("re-serialize failed");
        assert_eq!(json, json2, "roundtrip changed value: {json}");
    }

    #[test]
    fn test_request_run_roundtrip() {
        roundtrip(&Request::Run {
            command: "echo hello".into(),
            name: Some("my-job".into()),
            cwd: "/tmp".into(),
            project: "/project".into(),
            timeout_secs: Some(30),
            idempotency_key: Some("key1".into()),
        });
    }

    #[test]
    fn test_request_run_minimal_roundtrip() {
        roundtrip(&Request::Run {
            command: "echo hi".into(),
            name: None,
            cwd: "/tmp".into(),
            project: "/tmp".into(),
            timeout_secs: None,
            idempotency_key: None,
        });
    }

    #[test]
    fn test_request_stop_roundtrip() {
        roundtrip(&Request::Stop {
            id: "abc1".into(),
            force: true,
        });
        roundtrip(&Request::Stop {
            id: "abc1".into(),
            force: false,
        });
    }

    #[test]
    fn test_request_status_roundtrip() {
        roundtrip(&Request::Status { id: "abc1".into() });
    }

    #[test]
    fn test_request_list_roundtrip() {
        roundtrip(&Request::List {
            status: Some("running".into()),
            limit: Some(10),
        });
        roundtrip(&Request::List {
            status: None,
            limit: None,
        });
    }

    #[test]
    fn test_request_wait_roundtrip() {
        roundtrip(&Request::Wait {
            id: "abc1".into(),
            timeout_secs: Some(60),
        });
    }

    #[test]
    fn test_request_ping_roundtrip() {
        roundtrip(&Request::Ping);
    }

    #[test]
    fn test_request_shutdown_roundtrip() {
        roundtrip(&Request::Shutdown);
    }

    #[test]
    fn test_response_ok_roundtrip() {
        roundtrip(&Response::Ok);
    }

    #[test]
    fn test_response_error_roundtrip() {
        roundtrip(&Response::Error("something went wrong".into()));
    }

    #[test]
    fn test_response_user_error_roundtrip() {
        roundtrip(&Response::UserError("name in use".into()));
    }

    #[test]
    fn test_response_pong_roundtrip() {
        roundtrip(&Response::Pong {
            pid: 12345,
            uptime_secs: 3600,
            running_jobs: 2,
            total_jobs: 50,
        });
    }
}
