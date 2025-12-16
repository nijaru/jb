use anyhow::Result;
use jb_core::{Database, Paths};
use std::io::{BufRead, BufReader};

pub async fn execute(id: String, tail: Option<usize>, follow: bool) -> Result<()> {
    let paths = Paths::new();
    let db = Database::open(&paths)?;

    let job = db.get(&id)?;
    let job = match job {
        Some(j) => j,
        None => {
            let by_name = db.get_by_name(&id)?;
            match by_name.len() {
                0 => anyhow::bail!("No job found with ID or name '{}'", id),
                1 => by_name.into_iter().next().unwrap(),
                _ => {
                    eprintln!("Multiple jobs named '{}'. Use ID instead:", id);
                    for j in by_name {
                        eprintln!("  {} ({})", j.short_id(), j.status);
                    }
                    anyhow::bail!("Ambiguous job name");
                }
            }
        }
    };

    let log_path = paths.log_file(&job.id);

    if !log_path.exists() {
        println!("No output yet");
        return Ok(());
    }

    if follow {
        // TODO: Implement follow mode with inotify/kqueue
        eprintln!("--follow not yet implemented");
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
        println!("{}", line);
    }

    Ok(())
}
