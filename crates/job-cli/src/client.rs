use anyhow::Result;
use jb_core::ipc::{Request, Response};
use jb_core::Paths;
use std::path::Path;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;

pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    pub async fn connect() -> Result<Self> {
        let paths = Paths::new();
        Self::connect_to(paths.socket()).await
    }

    pub async fn connect_to(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self { stream })
    }

    pub async fn send(&mut self, request: Request) -> Result<Response> {
        // Write request
        let data = serde_json::to_vec(&request)?;
        let len = (data.len() as u32).to_be_bytes();
        self.stream.write_all(&len).await?;
        self.stream.write_all(&data).await?;
        self.stream.flush().await?;

        // Read response
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 10 * 1024 * 1024 {
            anyhow::bail!("Response too large: {} bytes", len);
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;

        let response: Response = serde_json::from_slice(&buf)?;
        Ok(response)
    }

    pub async fn ping(&mut self) -> Result<Response> {
        self.send(Request::Ping).await
    }
}

pub fn is_daemon_running() -> bool {
    let paths = Paths::new();
    let pid_file = paths.pid_file();

    if !pid_file.exists() {
        return false;
    }

    // Check if PID is still alive
    if let Ok(pid_str) = std::fs::read_to_string(&pid_file) {
        if let Ok(pid) = pid_str.trim().parse::<i32>() {
            use nix::sys::signal::kill;
            use nix::unistd::Pid;
            // Signal 0 checks if process exists without sending a signal
            return kill(Pid::from_raw(pid), None).is_ok();
        }
    }

    false
}
