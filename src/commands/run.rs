use crate::client::DaemonClient;
use crate::core::ipc::{Request, Response};
use crate::core::{Paths, detect_project, parse_duration};
use anyhow::Result;
use std::env;

#[allow(clippy::fn_params_excessive_bools, clippy::too_many_arguments)]
pub async fn execute(
    command: String,
    name: Option<String>,
    timeout: Option<String>,
    context: Option<String>,
    key: Option<String>,
    wait: bool,
    follow: bool,
    json: bool,
) -> Result<()> {
    let paths = Paths::new();
    paths.ensure_dirs()?;

    let cwd = env::current_dir()?;
    let project = detect_project(&cwd);

    let timeout_secs = timeout.as_ref().map(|t| parse_duration(t)).transpose()?;
    let context_json: Option<serde_json::Value> = context
        .as_ref()
        .map(|c| serde_json::from_str(c))
        .transpose()?;

    // Connect to daemon (auto-starts if not running)
    let mut client = DaemonClient::connect_or_start().await?;

    let request = Request::Run {
        command,
        name,
        cwd: cwd.to_string_lossy().to_string(),
        project: project.to_string_lossy().to_string(),
        timeout_secs,
        context: context_json,
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
                // Follow implies waiting, so use logs --follow
                crate::commands::logs::execute(&job_id, None, true)?;
            } else if wait {
                wait_for_job(&mut client, &job_id, json).await?;
            }

            Ok(())
        }
        Response::Error(e) => {
            anyhow::bail!("{e}");
        }
        _ => {
            anyhow::bail!("Unexpected response from daemon");
        }
    }
}

async fn wait_for_job(client: &mut DaemonClient, job_id: &str, json: bool) -> Result<()> {
    let request = Request::Wait {
        id: job_id.to_string(),
        timeout_secs: None,
    };

    match client.send(request).await? {
        Response::Job(job) => {
            if json {
                println!("{}", serde_json::to_string(&job)?);
            } else {
                eprintln!("Job {} finished: {}", job.short_id(), job.status);
                if let Some(code) = job.exit_code {
                    std::process::exit(code);
                }
            }
            Ok(())
        }
        Response::Error(e) => {
            anyhow::bail!("Wait failed: {e}");
        }
        _ => {
            anyhow::bail!("Unexpected response");
        }
    }
}
