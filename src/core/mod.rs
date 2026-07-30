pub mod db;
pub mod error;
pub mod ipc;
pub mod job;
pub mod paths;
pub mod project;

pub use db::Database;
pub use error::UserError;
pub use job::{Job, Status};
pub use paths::Paths;
pub use project::detect_project;

/// Signal an entire process group.
/// The PID is the process-group leader (the child is spawned with `process_group(0)`).
pub fn kill_process_group(pid: u32, force: bool) -> anyhow::Result<()> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::sys::signal::{Signal, killpg};
        use nix::unistd::Pid;

        if pid == 0 {
            anyhow::bail!("refusing to signal process group 0");
        }

        let signal = if force {
            Signal::SIGKILL
        } else {
            Signal::SIGTERM
        };
        #[allow(clippy::cast_possible_wrap)]
        let pid = Pid::from_raw(pid as i32);
        match killpg(pid, signal) {
            Ok(()) | Err(Errno::ESRCH) => Ok(()),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(unix))]
    {
        let _ = (pid, force);
        Ok(())
    }
}

/// Return whether a process group still exists.
pub fn process_group_exists(pid: u32) -> anyhow::Result<bool> {
    #[cfg(unix)]
    {
        use nix::errno::Errno;
        use nix::sys::signal::killpg;
        use nix::unistd::Pid;

        if pid == 0 {
            return Ok(false);
        }

        #[allow(clippy::cast_possible_wrap)]
        let pid = Pid::from_raw(pid as i32);
        match killpg(pid, None) {
            Ok(()) => Ok(true),
            Err(Errno::ESRCH) => Ok(false),
            Err(e) => Err(e.into()),
        }
    }

    #[cfg(not(unix))]
    {
        let _ = pid;
        Ok(false)
    }
}

/// Parse a duration string like "30s", "5m", "1h", "7d" into seconds
pub fn parse_duration(s: &str) -> anyhow::Result<u64> {
    let s = s.trim();
    let (num, unit) = if let Some(n) = s.strip_suffix('s') {
        (n, 1u64)
    } else if let Some(n) = s.strip_suffix('m') {
        (n, 60u64)
    } else if let Some(n) = s.strip_suffix('h') {
        (n, 3600u64)
    } else if let Some(n) = s.strip_suffix('d') {
        (n, 86400u64)
    } else {
        anyhow::bail!("Invalid duration format. Use: 30s, 5m, 1h, 7d");
    };

    let n: u64 = num.parse()?;
    n.checked_mul(unit)
        .ok_or_else(|| anyhow::anyhow!("duration is too large"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_duration_seconds() {
        assert_eq!(parse_duration("30s").unwrap(), 30);
        assert_eq!(parse_duration("1s").unwrap(), 1);
        assert_eq!(parse_duration("0s").unwrap(), 0);
    }

    #[test]
    fn test_parse_duration_minutes() {
        assert_eq!(parse_duration("5m").unwrap(), 300);
        assert_eq!(parse_duration("1m").unwrap(), 60);
    }

    #[test]
    fn test_parse_duration_hours() {
        assert_eq!(parse_duration("1h").unwrap(), 3600);
        assert_eq!(parse_duration("2h").unwrap(), 7200);
    }

    #[test]
    fn test_parse_duration_days() {
        assert_eq!(parse_duration("1d").unwrap(), 86400);
        assert_eq!(parse_duration("7d").unwrap(), 604_800);
    }

    #[test]
    fn test_parse_duration_with_whitespace() {
        assert_eq!(parse_duration("  30s  ").unwrap(), 30);
    }

    #[test]
    fn test_parse_duration_invalid_format() {
        assert!(parse_duration("30").is_err());
        assert!(parse_duration("30x").is_err());
        assert!(parse_duration("abc").is_err());
    }

    #[test]
    fn test_parse_duration_invalid_number() {
        assert!(parse_duration("abcs").is_err());
    }

    #[test]
    fn test_parse_duration_overflow() {
        assert!(parse_duration("18446744073709551615m").is_err());
    }
}
