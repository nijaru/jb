#![cfg(unix)]

use nix::errno::Errno;
use nix::sys::signal::{Signal, kill};
use nix::unistd::Pid;
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Output;
use std::time::Duration;
use tempfile::TempDir;
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

fn job_id(output: &Output) -> String {
    assert!(output.status.success(), "jb failed: {output:?}");
    String::from_utf8_lossy(&output.stdout).trim().to_owned()
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

    let status = sandbox.run(&["status", &id, "--json"]).await;
    assert!(status.status.success(), "status failed: {status:?}");
    let job: Value = serde_json::from_slice(&status.stdout).expect("status JSON");
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
