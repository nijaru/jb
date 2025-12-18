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

    let job = db.get(id)?;
    let job = if let Some(j) = job {
        j
    } else {
        let by_name = db.get_by_name(id)?;
        match by_name.len() {
            0 => anyhow::bail!("No job found with ID or name '{id}'"),
            1 => by_name.into_iter().next().unwrap(),
            _ => {
                eprintln!("Multiple jobs named '{id}'. Use ID instead:");
                for j in by_name {
                    eprintln!("  {} ({})", j.short_id(), j.status);
                }
                anyhow::bail!("Ambiguous job name");
            }
        }
    };

    let log_path = paths.log_file(&job.id);

    if follow {
        return follow_logs(&paths, &job.id, &log_path);
    }

    // Non-follow mode: read existing content
    if !log_path.exists() {
        println!("No output yet");
        return Ok(());
    }

    let file = std::fs::File::open(&log_path)?;
    let reader = BufReader::new(file);
    let lines: Vec<String> = reader.lines().collect::<Result<_, _>>()?;

    let output_lines = if let Some(n) = tail {
        let start = lines.len().saturating_sub(n);
        &lines[start..]
    } else {
        &lines[..]
    };

    for line in output_lines {
        println!("{line}");
    }

    Ok(())
}

fn follow_logs(paths: &Paths, job_id: &str, log_path: &Path) -> Result<()> {
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
        let db = Database::open(paths)?;
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
        let db = Database::open(paths)?;
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
