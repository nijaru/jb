use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Paths, detect_project, parse_duration};
use anyhow::{Context, Result};
use std::env;
use std::path::PathBuf;

#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub async fn execute(
    command: String,
    name: Option<String>,
    timeout: Option<String>,
    dir: Option<String>,
    key: Option<String>,
    wait: bool,
    follow: bool,
    json: bool,
) -> Result<i32> {
    let paths = Paths::new()?;
    paths.ensure_dirs()?;

    let cwd = match dir {
        Some(dir) => PathBuf::from(&dir)
            .canonicalize()
            .with_context(|| format!("directory not found: {dir}"))?,
        None => env::current_dir()?,
    };
    let project = detect_project(&cwd);
    let timeout_secs = timeout
        .as_ref()
        .map(|value| parse_duration(value))
        .transpose()?;

    let mut client = DaemonClient::connect_or_start().await?;
    let request = Request::Run {
        command,
        name,
        cwd: cwd.to_string_lossy().to_string(),
        project: project.to_string_lossy().to_string(),
        timeout_secs,
        idempotency_key: key,
    };

    match client.send(request).await? {
        Response::Job(job) => {
            let job_id = job.id.clone();
            if json && !follow {
                println!("{}", serde_json::to_string(&job)?);
            } else if !follow {
                println!("{}", job.short_id());
            }

            if follow {
                crate::commands::logs::execute(&job_id, None, true, false).await
            } else if wait {
                wait_for_job(&mut client, &job_id, json).await
            } else {
                Ok(0)
            }
        }
        Response::UserError(error) => anyhow::bail!(crate::core::UserError::new(error)),
        Response::Error(error) => anyhow::bail!("{error}"),
        _ => anyhow::bail!("Unexpected response from daemon"),
    }
}

async fn wait_for_job(client: &mut DaemonClient, job_id: &str, json: bool) -> Result<i32> {
    match client
        .send(Request::Wait {
            id: job_id.to_string(),
            timeout_secs: None,
        })
        .await?
    {
        Response::Job(job) => {
            if json {
                println!("{}", serde_json::to_string(&job)?);
            } else {
                eprintln!("Job {} finished: {}", job.short_id(), job.status);
            }
            Ok(job.status.cli_exit_code(job.exit_code))
        }
        Response::Error(error) => anyhow::bail!("Wait failed: {error}"),
        _ => anyhow::bail!("Unexpected response"),
    }
}
