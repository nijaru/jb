use crate::core::{Database, Paths};
use anyhow::Result;
use std::io::{BufRead, BufReader, Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::Duration;

pub fn execute(id: &str, tail: Option<usize>, follow: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    // Check for orphaned jobs (dead processes still marked running)
    db.recover_orphans();

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

    if let Some(n) = tail {
        // Efficient tail: read last N lines without loading entire file
        tail_last_n_lines(&log_path, n)?;
    } else {
        // Stream entire file line by line (memory efficient)
        let file = std::fs::File::open(&log_path)?;
        let reader = BufReader::new(file);
        for line in reader.lines() {
            println!("{}", line?);
        }
    }

    Ok(())
}

/// Read last N lines from a file without loading the entire file into memory.
/// Uses backward chunk reading to find line boundaries efficiently.
fn tail_last_n_lines(path: &Path, n: usize) -> Result<()> {
    use std::io::BufWriter;

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
    let mut reader = BufReader::new(file);
    let stdout = std::io::stdout();
    let mut writer = BufWriter::new(stdout.lock());
    std::io::copy(&mut reader, &mut writer)?;

    Ok(())
}

fn follow_logs(db: &Database, _paths: &Paths, job_id: &str, log_path: &Path) -> Result<()> {
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

    loop {
        if interrupted.load(Ordering::SeqCst) {
            return Ok(());
        }

        // Read new content from current position
        file.seek(SeekFrom::Start(position))?;
        let bytes_read = file.read(&mut buf)?;
        if bytes_read > 0 {
            std::io::stdout().write_all(&buf[..bytes_read])?;
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
                    std::io::stdout().write_all(&buf[..bytes_read])?;
                    position += bytes_read as u64;
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
