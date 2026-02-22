use crate::core::Paths;
use crate::core::ipc::{Request, Response};
use anyhow::Result;
use std::path::Path;
use std::process::Stdio;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::UnixStream;
use tokio::process::Command;

pub struct DaemonClient {
    stream: UnixStream,
}

impl DaemonClient {
    async fn connect_to(socket_path: impl AsRef<Path>) -> Result<Self> {
        let stream = UnixStream::connect(socket_path).await?;
        Ok(Self { stream })
    }

    /// Connect to daemon, starting it if not running
    pub async fn connect_or_start() -> Result<Self> {
        let paths = Paths::new()?;

        // Try connecting first
        if let Ok(client) = Self::connect_to(paths.socket()).await {
            return Ok(client);
        }

        // Daemon not running, start it
        start_daemon()?;

        // Wait for daemon to be ready
        for _ in 0..50 {
            tokio::time::sleep(Duration::from_millis(100)).await;
            if let Ok(client) = Self::connect_to(paths.socket()).await {
                return Ok(client);
            }
        }

        anyhow::bail!("Daemon failed to start within 5 seconds")
    }

    pub async fn send(&mut self, request: Request) -> Result<Response> {
        // Write request
        let data = serde_json::to_vec(&request)?;
        #[allow(clippy::cast_possible_truncation)] // messages are always < 4GB
        let len = (data.len() as u32).to_be_bytes();
        self.stream.write_all(&len).await?;
        self.stream.write_all(&data).await?;
        self.stream.flush().await?;

        // Read response
        let mut len_buf = [0u8; 4];
        self.stream.read_exact(&mut len_buf).await?;
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 10 * 1024 * 1024 {
            anyhow::bail!("Response too large: {len} bytes");
        }

        let mut buf = vec![0u8; len];
        self.stream.read_exact(&mut buf).await?;

        let response: Response = serde_json::from_slice(&buf)?;
        Ok(response)
    }
}

fn start_daemon() -> Result<()> {
    // Use same binary with "daemon" subcommand
    let exe = std::env::current_exe()?;

    // Spawn daemon detached
    Command::new(&exe)
        .arg("daemon")
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .process_group(0)
        .spawn()?;

    Ok(())
}
