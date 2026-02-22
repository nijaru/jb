use crate::core::{Database, Paths};
use anyhow::Result;
use colored::Colorize;
use std::io::{BufRead, BufReader, IsTerminal, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::{Command, Stdio};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

type WriterFn = Box<dyn FnOnce(&mut dyn Write) -> Result<()>>;

fn should_colorize() -> bool {
    std::io::stdout().is_terminal() && std::env::var_os("NO_COLOR").is_none()
}

pub fn execute(id: &str, tail: Option<usize>, follow: bool, pager: bool) -> Result<()> {
    let paths = Paths::new()?;
    let db = Database::open(&paths)?;

    // Check for orphaned jobs (dead processes still marked running)
    // Skip in follow mode to avoid race condition with daemon
    if !follow {
        db.recover_orphans();
    }

    let job = db.resolve(id)?;
    let log_path = paths.log_file(&job.id);

    if follow {
        return follow_logs(&db, &paths, &job.id, &log_path);
    }

    // Non-follow mode: read existing content
    if !log_path.exists() {
        println!("No output yet");
        return Ok(());
    }

    let colorize = should_colorize();
    let use_pager = colorize && pager;

    if let Some(n) = tail {
        // Efficient tail: read last N lines without loading entire file
        if use_pager {
            output_with_pager(|| tail_lines_to_writer(&log_path, n, colorize))?;
        } else {
            let stdout = std::io::stdout();
            let mut writer = stdout.lock();
            tail_lines_to_writer(&log_path, n, colorize)(&mut writer)?;
        }
    } else {
        // Stream entire file
        if use_pager {
            output_with_pager(|| stream_file_to_writer(&log_path, colorize))?;
        } else {
            let stdout = std::io::stdout();
            let mut writer = stdout.lock();
            stream_file_to_writer(&log_path, colorize)(&mut writer)?;
        }
    }

    Ok(())
}

fn colorize_line(line: &str) -> String {
    let lower = line.to_lowercase();
    if lower.contains("error") || lower.contains("fatal") || lower.contains("panic") {
        line.red().to_string()
    } else if lower.contains("warn") {
        line.yellow().to_string()
    } else if lower.contains("info") {
        line.blue().to_string()
    } else if lower.contains("debug") || lower.contains("trace") {
        line.dimmed().to_string()
    } else {
        line.to_string()
    }
}

fn output_with_pager<F>(content_fn: F) -> Result<()>
where
    F: FnOnce() -> Box<dyn FnOnce(&mut dyn Write) -> Result<()>>,
{
    let pager_env = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
    let mut parts = pager_env.split_whitespace();
    let pager_cmd = parts.next().unwrap_or("less");
    let mut pager_args: Vec<&str> = parts.collect();
    // Always pass -R to less so ANSI colors render correctly
    if pager_cmd == "less" && !pager_args.contains(&"-R") {
        pager_args.push("-R");
    }

    let mut child = match Command::new(pager_cmd)
        .args(&pager_args)
        .stdin(Stdio::piped())
        .spawn()
    {
        Ok(child) => child,
        Err(_) => {
            // Pager not available, fall back to stdout
            let stdout = std::io::stdout();
            let mut writer = stdout.lock();
            return content_fn()(&mut writer);
        }
    };

    if let Some(mut stdin) = child.stdin.take() {
        let _ = content_fn()(&mut stdin);
    }

    let _ = child.wait();
    Ok(())
}

fn stream_file_to_writer(path: &Path, colorize: bool) -> WriterFn {
    let path = path.to_path_buf();
    Box::new(move |writer: &mut dyn Write| {
        let file = std::fs::File::open(&path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            let line = line?;
            if colorize {
                writeln!(writer, "{}", colorize_line(&line))?;
            } else {
                writeln!(writer, "{}", line)?;
            }
        }
        Ok(())
    })
}

fn tail_lines_to_writer(path: &Path, n: usize, colorize: bool) -> WriterFn {
    let path = path.to_path_buf();
    Box::new(move |writer: &mut dyn Write| tail_last_n_lines_to_writer(&path, n, colorize, writer))
}

/// Read last N lines from a file without loading the entire file into memory.
/// Uses backward chunk reading to find line boundaries efficiently.
fn tail_last_n_lines_to_writer(
    path: &Path,
    n: usize,
    colorize: bool,
    writer: &mut dyn Write,
) -> Result<()> {
    const CHUNK_SIZE: u64 = 8192;

    let mut file = std::fs::File::open(path)?;
    let len = file.metadata()?.len();
    if len == 0 || n == 0 {
        return Ok(());
    }

    let mut newline_positions: Vec<u64> = Vec::with_capacity(n + 1);
    let mut pos = len;

    // Scan backwards to find newline positions
    while pos > 0 && newline_positions.len() <= n {
        let chunk_start = pos.saturating_sub(CHUNK_SIZE);
        #[allow(clippy::cast_possible_truncation)]
        let chunk_len = (pos - chunk_start) as usize;

        file.seek(SeekFrom::Start(chunk_start))?;
        let mut buf = vec![0u8; chunk_len];
        file.read_exact(&mut buf)?;

        // Scan chunk backwards for newlines
        for (i, &byte) in buf.iter().enumerate().rev() {
            if byte == b'\n' {
                let abs_pos = chunk_start + i as u64;
                // Don't count trailing newline at end of file
                if abs_pos + 1 < len {
                    newline_positions.push(abs_pos + 1); // Position after newline
                }
                if newline_positions.len() > n {
                    break;
                }
            }
        }

        pos = chunk_start;
    }

    // Determine start position
    // newline_positions stores positions AFTER each newline (line starts)
    // To get last n lines, we need newline_positions[n-1] (0-indexed)
    let start_pos = if newline_positions.len() >= n {
        newline_positions[n - 1]
    } else {
        0 // File has fewer than n lines, read from start
    };

    // Stream from start_pos to end
    file.seek(SeekFrom::Start(start_pos))?;
    let reader = BufReader::new(file);
    for line in reader.lines() {
        let line = line?;
        if colorize {
            writeln!(writer, "{}", colorize_line(&line))?;
        } else {
            writeln!(writer, "{}", line)?;
        }
    }

    Ok(())
}

fn follow_logs(db: &Database, _paths: &Paths, job_id: &str, log_path: &Path) -> Result<()> {
    let colorize = should_colorize();

    // Set up Ctrl+C handler - on interrupt, just exit cleanly (job continues)
    let interrupted = Arc::new(AtomicBool::new(false));
    let int_clone = Arc::clone(&interrupted);
    ctrlc_handler(move || {
        int_clone.store(true, Ordering::SeqCst);
    });

    // Wait for log file to exist (job might be pending)
    while !log_path.exists() {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Check if job still exists and is not terminal
        if let Some(job) = db.get(job_id)? {
            if job.status.is_terminal() {
                // Job finished before creating output
                eprintln!("Job finished with no output");
                if let Some(code) = job.exit_code {
                    std::process::exit(code);
                }
                return Ok(());
            }
        } else {
            anyhow::bail!("Job not found");
        }

        std::thread::sleep(Duration::from_millis(100));
    }

    let mut file = std::fs::File::open(log_path)?;
    let mut position = 0u64;
    let mut buf = vec![0u8; 8192];
    let mut line_buf = String::new();

    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Read new content from current position
        file.seek(SeekFrom::Start(position))?;
        let bytes_read = file.read(&mut buf)?;
        if bytes_read > 0 {
            let chunk = String::from_utf8_lossy(&buf[..bytes_read]);
            if colorize {
                // Buffer partial lines for colorization
                for c in chunk.chars() {
                    if c == '\n' {
                        println!("{}", colorize_line(&line_buf));
                        line_buf.clear();
                    } else {
                        line_buf.push(c);
                    }
                }
            } else {
                std::io::stdout().write_all(&buf[..bytes_read])?;
            }
            std::io::stdout().flush()?;
            position += bytes_read as u64;
        }

        // Check job status
        if let Some(job) = db.get(job_id)? {
            if job.status.is_terminal() {
                // Final read to catch any remaining output
                loop {
                    file.seek(SeekFrom::Start(position))?;
                    let bytes_read = file.read(&mut buf)?;
                    if bytes_read == 0 {
                        break;
                    }
                    let chunk = String::from_utf8_lossy(&buf[..bytes_read]);
                    if colorize {
                        for c in chunk.chars() {
                            if c == '\n' {
                                println!("{}", colorize_line(&line_buf));
                                line_buf.clear();
                            } else {
                                line_buf.push(c);
                            }
                        }
                    } else {
                        std::io::stdout().write_all(&buf[..bytes_read])?;
                    }
                    position += bytes_read as u64;
                }
                // Print any remaining partial line
                if colorize && !line_buf.is_empty() {
                    println!("{}", colorize_line(&line_buf));
                }
                std::io::stdout().flush()?;

                // Exit with job's exit code
                if let Some(code) = job.exit_code {
                    std::process::exit(code);
                }
                return Ok(());
            }
        } else {
            anyhow::bail!("Job disappeared from database");
        }

        // Small sleep before next poll
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Simple Ctrl+C handler without adding ctrlc dependency
fn ctrlc_handler<F: Fn() + Send + Sync + 'static>(handler: F) {
    #[cfg(unix)]
    {
        use nix::sys::signal::{SigHandler, Signal, signal};

        static HANDLER: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> =
            std::sync::OnceLock::new();

        extern "C" fn signal_handler(_: i32) {
            if let Some(h) = HANDLER.get() {
                h();
            }
        }

        let _ = HANDLER.set(Box::new(handler));
        unsafe {
            let _ = signal(Signal::SIGINT, SigHandler::Handler(signal_handler));
        }
    }

    #[cfg(not(unix))]
    {
        // On non-Unix, just ignore - Ctrl+C will terminate the process
        let _ = handler;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use colored::control::set_override;
    use std::sync::Mutex;

    static COLOR_LOCK: Mutex<()> = Mutex::new(());

    fn with_colors<F: FnOnce()>(f: F) {
        let _guard = COLOR_LOCK.lock().unwrap();
        set_override(true);
        f();
        set_override(false);
    }

    #[test]
    fn test_colorize_line_error() {
        with_colors(|| {
            let result = colorize_line("ERROR: something failed");
            assert!(result.contains("\x1b[31m")); // red
            assert!(result.contains("ERROR: something failed"));
        });
    }

    #[test]
    fn test_colorize_line_error_lowercase() {
        with_colors(|| {
            let result = colorize_line("error: something failed");
            assert!(result.contains("\x1b[31m")); // red
        });
    }

    #[test]
    fn test_colorize_line_fatal() {
        with_colors(|| {
            let result = colorize_line("FATAL: crash");
            assert!(result.contains("\x1b[31m")); // red
        });
    }

    #[test]
    fn test_colorize_line_panic() {
        with_colors(|| {
            let result = colorize_line("thread panicked at...");
            assert!(result.contains("\x1b[31m")); // red
        });
    }

    #[test]
    fn test_colorize_line_warn() {
        with_colors(|| {
            let result = colorize_line("WARN: deprecated");
            assert!(result.contains("\x1b[33m")); // yellow
        });
    }

    #[test]
    fn test_colorize_line_warning() {
        with_colors(|| {
            let result = colorize_line("warning: unused variable");
            assert!(result.contains("\x1b[33m")); // yellow
        });
    }

    #[test]
    fn test_colorize_line_info() {
        with_colors(|| {
            let result = colorize_line("INFO: starting up");
            assert!(result.contains("\x1b[34m")); // blue
        });
    }

    #[test]
    fn test_colorize_line_debug() {
        with_colors(|| {
            let result = colorize_line("DEBUG: value = 42");
            assert!(result.contains("\x1b[2m")); // dimmed
        });
    }

    #[test]
    fn test_colorize_line_trace() {
        with_colors(|| {
            let result = colorize_line("TRACE: entering function");
            assert!(result.contains("\x1b[2m")); // dimmed
        });
    }

    #[test]
    fn test_colorize_line_normal() {
        // Without color override, should return unchanged
        let result = colorize_line("just a normal line");
        assert_eq!(result, "just a normal line");
    }

    #[test]
    fn test_colorize_line_empty() {
        let result = colorize_line("");
        assert_eq!(result, "");
    }

    #[test]
    fn test_colorize_line_priority() {
        with_colors(|| {
            // error takes priority over warn/info
            let result = colorize_line("error: warning about info");
            assert!(result.contains("\x1b[31m")); // red (error), not yellow (warn)
        });
    }

    #[test]
    fn test_colorize_line_case_insensitive() {
        with_colors(|| {
            let result = colorize_line("ErRoR: mixed case");
            assert!(result.contains("\x1b[31m")); // red
        });
    }
}
