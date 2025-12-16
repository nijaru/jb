# job - Background Job Manager

Use `job` to run tasks that should:

- Survive session end
- Run longer than 30 seconds
- Execute in parallel with other work

## Quick Reference

| Command           | Purpose              |
| ----------------- | -------------------- |
| `job run "cmd"`   | Start background job |
| `job list`        | List project jobs    |
| `job status <id>` | Job details          |
| `job status`      | System status        |
| `job logs <id>`   | View output          |
| `job stop <id>`   | Stop a job           |
| `job wait <id>`   | Block until done     |
| `job retry <id>`  | Re-run a job         |
| `job clean`       | Remove old jobs      |

## Starting Jobs

```bash
# Basic usage - returns job ID immediately
job run "pytest tests/"

# With name for easy reference
job run "make build" --name build

# With timeout
job run "npm test" --timeout 30m

# With context metadata (for your own tracking)
job run "deploy.sh" --context '{"pr": 123, "env": "staging"}'

# Idempotent - won't create duplicate if key exists
job run "pytest" --key "test-$(git rev-parse HEAD)"

# Wait for completion (blocks)
job run "pytest" --wait
```

## Listing Jobs

```bash
# List jobs for current project (default)
job list

# List all jobs across all projects
job list --all

# Filter by status
job list --status running
job list --status failed

# JSON output for parsing
job list --json
```

## Checking Status

```bash
# System status (no ID)
job status

# Job details
job status abc123
job status build  # by name if unique

# JSON output
job status abc123 --json
```

## Viewing Logs

```bash
# Full output
job logs abc123

# Last 50 lines (default with --tail)
job logs abc123 --tail

# Last N lines
job logs abc123 --tail 100

# Stream live output (follow mode)
job logs abc123 --follow
```

## Stopping Jobs

```bash
# Graceful stop (SIGTERM)
job stop abc123

# Force kill (SIGKILL)
job stop abc123 --force
```

## Waiting for Completion

```bash
# Block until job finishes
job wait abc123

# With timeout
job wait abc123 --timeout 5m
```

Exit codes:

- `0` - Job completed successfully
- `1` - Job failed
- `124` - Timeout reached (job still running)

## Patterns

### Fire and Forget

```bash
job run "make build" --name build
# Continue with other work...
```

### Run Multiple in Parallel

```bash
job run "npm test" --name tests
job run "npm run lint" --name lint
job run "npm run typecheck" --name types

# Check results later
job list
```

### Wait for Multiple Jobs

```bash
job wait tests && job wait lint && job wait types
```

### Check Project Jobs After Break

```bash
# See what's running/completed in this project
job list

# Check specific job output
job logs <id> --tail
```

### Retry Failed Job

```bash
job retry abc123
# Creates new job with same command/config
```

## When NOT to Use

- Quick commands (<10 seconds)
- Interactive commands requiring TTY
- Commands that need user input

## Storage

Jobs are stored in `~/.job/`:

- `job.db` - SQLite database
- `logs/` - Job output files

Clean up old jobs:

```bash
job clean                    # Remove jobs older than 7 days
job clean --older-than 1d    # Custom retention
job clean --all              # Remove all non-running jobs
```
