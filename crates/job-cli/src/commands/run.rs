use anyhow::Result;
use jb_core::{detect_project, Database, Job, Paths};
use std::env;

pub async fn execute(
    command: String,
    name: Option<String>,
    timeout: Option<String>,
    context: Option<String>,
    key: Option<String>,
    wait: bool,
    json: bool,
) -> Result<()> {
    let paths = Paths::new();
    paths.ensure_dirs()?;
    let db = Database::open(&paths)?;

    let cwd = env::current_dir()?;
    let project = detect_project(&cwd);

    if let Some(ref k) = key {
        if let Some(existing) = db.get_by_idempotency_key(k)? {
            if json {
                println!("{}", serde_json::to_string(&existing)?);
            } else {
                println!("{}", existing.short_id());
                eprintln!("Job with key '{}' already exists", k);
            }
            return Ok(());
        }
    }

    let mut job = Job::new(command, cwd, project);

    if let Some(n) = name {
        job = job.with_name(n);
    }

    if let Some(t) = timeout {
        let secs = parse_duration(&t)?;
        job = job.with_timeout(secs);
    }

    if let Some(c) = context {
        let ctx: serde_json::Value = serde_json::from_str(&c)?;
        job = job.with_context(ctx);
    }

    if let Some(k) = key {
        job = job.with_idempotency_key(k);
    }

    db.insert(&job)?;

    // TODO: Send to daemon for execution
    // For now, just print the job ID
    if json {
        println!("{}", serde_json::to_string(&job)?);
    } else {
        println!("{}", job.short_id());
    }

    if wait {
        // TODO: Implement wait logic
        eprintln!("--wait not yet implemented");
    }

    Ok(())
}

fn parse_duration(s: &str) -> Result<u64> {
    let s = s.trim();
    let (num, unit) = if s.ends_with("s") {
        (&s[..s.len() - 1], 1u64)
    } else if s.ends_with("m") {
        (&s[..s.len() - 1], 60u64)
    } else if s.ends_with("h") {
        (&s[..s.len() - 1], 3600u64)
    } else if s.ends_with("d") {
        (&s[..s.len() - 1], 86400u64)
    } else {
        anyhow::bail!("Invalid duration format. Use: 30s, 5m, 1h, 7d");
    };

    let n: u64 = num.parse()?;
    Ok(n * unit)
}
