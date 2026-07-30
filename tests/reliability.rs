#![cfg(unix)]

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill, killpg};
use nix::unistd::Pid;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;
use tempfile::TempDir;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;

struct Sandbox {
    home: TempDir,
}

impl Sandbox {
    fn new() -> Self {
        Self {
            home: TempDir::new().expect("create test home"),
        }
    }

    fn home_path(&self) -> &Path {
        self.home.path()
    }

    fn jb_dir(&self) -> PathBuf {
        self.home.path().join(".jb")
    }

    async fn run(&self, args: &[&str]) -> Output {
        tokio::time::timeout(
            Duration::from_secs(15),
            Command::new(env!("CARGO_BIN_EXE_jb"))
                .args(args)
                .env("HOME", self.home_path())
                .env("NO_COLOR", "1")
                .env("RUST_LOG", "off")
                .output(),
        )
        .await
        .expect("jb command timed out")
        .expect("run jb command")
    }
}

impl Drop for Sandbox {
    fn drop(&mut self) {
        let pid_path = self.jb_dir().join("daemon.pid");
        let Some(pid) = read_pid(&pid_path) else {
            return;
        };

        let _ = kill(Pid::from_raw(pid), Some(Signal::SIGTERM));
        for _ in 0..50 {
            if !pid_alive(pid) {
                return;
            }
            std::thread::sleep(Duration::from_millis(20));
        }
        let _ = kill(Pid::from_raw(pid), Some(Signal::SIGKILL));
    }
}

fn read_pid(path: &Path) -> Option<i32> {
    std::fs::read_to_string(path).ok()?.trim().parse().ok()
}

fn pid_alive(pid: i32) -> bool {
    match kill(Pid::from_raw(pid), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

fn process_group_alive(pid: i32) -> bool {
    match killpg(Pid::from_raw(pid), None) {
        Ok(()) | Err(Errno::EPERM) => true,
        Err(Errno::ESRCH) => false,
        Err(_) => true,
    }
}

fn job_id(output: &Output) -> String {
    assert!(output.status.success(), "jb failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
}

async fn running_job(sandbox: &Sandbox, id: &str) -> Value {
    for _ in 0..100 {
        let status = sandbox.run(&["status", id, "--json"]).await;
        assert!(status.status.success(), "status failed: {status:?}");
        let job: Value = serde_json::from_slice(&status.stdout).expect("status JSON");
        if job["pid"].is_number() {
            return job;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("job {id} did not start");
}

async fn stop_via_ipc(sandbox: &Sandbox, id: &str) -> Value {
    let mut stream = UnixStream::connect(sandbox.jb_dir().join("daemon.sock"))
        .await
        .expect("connect daemon socket");
    let request = serde_json::json!({
        "Stop": {
            "id": id,
            "force": false,
        }
    });
    let data = serde_json::to_vec(&request).expect("serialize stop request");
    #[allow(clippy::cast_possible_truncation)]
    let len = (data.len() as u32).to_be_bytes();
    stream.write_all(&len).await.expect("write request length");
    stream.write_all(&data).await.expect("write request");
    stream.flush().await.expect("flush request");

    let mut len_buf = [0u8; 4];
    stream
        .read_exact(&mut len_buf)
        .await
        .expect("read response length");
    let len = u32::from_be_bytes(len_buf) as usize;
    let mut response = vec![0u8; len];
    stream
        .read_exact(&mut response)
        .await
        .expect("read response");
    serde_json::from_slice(&response).expect("response JSON")
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn concurrent_startup_has_one_usable_daemon() {
    let sandbox = Sandbox::new();
    let (a, b, c, d, e, f, g, h) = tokio::join!(
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
        sandbox.run(&["run", "true"]),
    );
    let outputs = [a, b, c, d, e, f, g, h];

    assert!(
        outputs.iter().all(|output| output.status.success()),
        "concurrent jb run failed: {outputs:?}"
    );
    assert!(sandbox.jb_dir().join("daemon.pid").exists());
    assert!(sandbox.jb_dir().join("daemon.sock").exists());
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wait_propagates_failure_and_timeout_exit_codes() {
    let sandbox = Sandbox::new();

    let failed = sandbox.run(&["run", "exit 7", "--wait"]).await;
    assert_eq!(failed.status.code(), Some(7), "failure output: {failed:?}");

    let json_failed = sandbox.run(&["run", "exit 7", "--wait", "--json"]).await;
    assert_eq!(json_failed.status.code(), Some(7));
    let json_job: Value = serde_json::from_slice(&json_failed.stdout)
        .expect("--wait --json should emit one JSON document");
    assert_eq!(json_job["status"], "failed");

    let timed_out = sandbox
        .run(&["run", "sleep 10", "--timeout", "1s", "--wait"])
        .await;
    assert_eq!(
        timed_out.status.code(),
        Some(124),
        "timeout output: {timed_out:?}"
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_waits_for_a_term_ignoring_process_group() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["run", "trap '' TERM; sleep 60"]).await;
    let id = job_id(&output);

    let job = running_job(&sandbox, &id).await;
    let pid = job["pid"].as_u64().expect("job PID") as i32;

    let stopped = sandbox.run(&["stop", &id]).await;
    assert!(stopped.status.success(), "stop failed: {stopped:?}");

    let final_status = sandbox.run(&["status", &id, "--json"]).await;
    let final_job: Value = serde_json::from_slice(&final_status.stdout).expect("final status JSON");
    assert_eq!(final_job["status"], "stopped");

    for _ in 0..50 {
        if !pid_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("job process {pid} survived stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_retries_terminal_publish_after_transient_database_lock() {
    let sandbox = Sandbox::new();
    let output = sandbox.run(&["run", "sleep 60"]).await;
    let id = job_id(&output);
    let _ = running_job(&sandbox, &id).await;

    let db_path = sandbox.jb_dir().join("job.db");
    let (locked_tx, locked_rx) = tokio::sync::oneshot::channel();
    let lock_task = tokio::task::spawn_blocking(move || {
        let connection = rusqlite::Connection::open(db_path).expect("open lock database");
        connection
            .execute_batch("BEGIN EXCLUSIVE")
            .expect("acquire exclusive database lock");
        locked_tx.send(()).expect("signal lock acquisition");
        std::thread::sleep(Duration::from_secs(6));
        connection
            .execute_batch("COMMIT")
            .expect("release exclusive database lock");
    });
    locked_rx.await.expect("wait for database lock");

    let response = tokio::time::timeout(Duration::from_secs(15), stop_via_ipc(&sandbox, &id))
        .await
        .expect("stop IPC timed out");
    assert_eq!(response, "Ok", "stop failed: {response}");
    lock_task.await.expect("database lock task");

    let final_status = sandbox.run(&["status", &id, "--json"]).await;
    let final_job: Value = serde_json::from_slice(&final_status.stdout).expect("final status JSON");
    assert_eq!(final_job["status"], "stopped");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn stop_kills_descendants_when_leader_exits_on_term() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .run(&["run", "trap 'exit 0' TERM; (trap '' TERM; sleep 60) & wait"])
        .await;
    let id = job_id(&output);

    let job = running_job(&sandbox, &id).await;
    let pid = job["pid"].as_u64().expect("job PID") as i32;

    let stopped = sandbox.run(&["stop", &id]).await;
    assert!(stopped.status.success(), "stop failed: {stopped:?}");

    for _ in 0..150 {
        if !process_group_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process group {pid} survived stop");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeout_kills_descendants_when_leader_exits_on_term() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .run(&[
            "run",
            "trap 'exit 0' TERM; (trap '' TERM; sleep 60) & wait",
            "--timeout",
            "1s",
        ])
        .await;
    let id = job_id(&output);

    let job = running_job(&sandbox, &id).await;
    let pid = job["pid"].as_u64().expect("job PID") as i32;

    let waited = sandbox.run(&["wait", &id]).await;
    assert_eq!(waited.status.code(), Some(124), "wait failed: {waited:?}");

    for _ in 0..150 {
        if !process_group_alive(pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process group {pid} survived timeout");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_shutdown_kills_descendants_before_exit() {
    let sandbox = Sandbox::new();
    let output = sandbox
        .run(&["run", "trap 'exit 0' TERM; (trap '' TERM; sleep 60) & wait"])
        .await;
    let id = job_id(&output);

    let job = running_job(&sandbox, &id).await;
    let job_pid = job["pid"].as_u64().expect("job PID") as i32;
    let daemon_pid = read_pid(&sandbox.jb_dir().join("daemon.pid")).expect("daemon PID");

    kill(Pid::from_raw(daemon_pid), Some(Signal::SIGTERM)).expect("signal daemon");
    for _ in 0..150 {
        if !pid_alive(daemon_pid) {
            break;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    assert!(!pid_alive(daemon_pid), "daemon survived shutdown");

    for _ in 0..150 {
        if !process_group_alive(job_pid) {
            return;
        }
        tokio::time::sleep(Duration::from_millis(20)).await;
    }
    panic!("process group {job_pid} survived daemon shutdown");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn daemon_state_files_have_private_permissions() {
    use std::os::unix::fs::PermissionsExt;

    let sandbox = Sandbox::new();
    let output = sandbox.run(&["run", "true"]).await;
    let _ = job_id(&output);

    let root_mode = std::fs::metadata(sandbox.jb_dir())
        .expect("jb directory")
        .permissions()
        .mode()
        & 0o777;
    let logs_mode = std::fs::metadata(sandbox.jb_dir().join("logs"))
        .expect("logs directory")
        .permissions()
        .mode()
        & 0o777;
    let socket_mode = std::fs::metadata(sandbox.jb_dir().join("daemon.sock"))
        .expect("daemon socket")
        .permissions()
        .mode()
        & 0o777;
    let db_mode = std::fs::metadata(sandbox.jb_dir().join("job.db"))
        .expect("database")
        .permissions()
        .mode()
        & 0o777;

    assert_eq!(root_mode, 0o700);
    assert_eq!(logs_mode, 0o700);
    assert_eq!(socket_mode, 0o600);
    assert_eq!(db_mode, 0o600);
}
