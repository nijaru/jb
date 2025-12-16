# jb - Background Job Manager

Use `jb` to run tasks that should:

- Survive session end
- Run longer than 30 seconds
- Execute in parallel with other work

## Quick Reference

| Command          | Purpose              |
| ---------------- | -------------------- |
| `jb run "cmd"`   | Start background job |
| `jb list`        | List project jobs    |
| `jb status <id>` | Job details          |
| `jb status`      | System status        |
| `jb logs <id>`   | View output          |
| `jb stop <id>`   | Stop a job           |
| `jb wait <id>`   | Block until done     |
| `jb retry <id>`  | Re-run a job         |
| `jb clean`       | Remove old jobs      |

## Starting Jobs

```bash
# Basic usage - returns job ID immediately
jb run "pytest tests/"

# With name for easy reference
jb run "make build" --name build

# With timeout
jb run "npm test" --timeout 30m

# With context metadata (for your own tracking)
jb run "deploy.sh" --context '{"pr": 123, "env": "staging"}'

# Idempotent - won't create duplicate if key exists
jb run "pytest" --key "test-$(git rev-parse HEAD)"

# Wait for completion (blocks)
jb run "pytest" --wait
```

## Listing Jobs

```bash
# List jobs for current project (default)
jb list

# List all jobs across all projects
jb list --all

# Filter by status
jb list --status running
jb list --status failed

# JSON output for parsing
jb list --json
```

## Checking Status

```bash
# System status (no ID)
jb status

# Job details
jb status abc123
jb status build  # by name if unique

# JSON output
jb status abc123 --json
```

## Viewing Logs

```bash
# Full output
jb logs abc123

# Last 50 lines (default with --tail)
jb logs abc123 --tail

# Last N lines
jb logs abc123 --tail 100

# Stream live output (follow mode)
jb logs abc123 --follow
```

## Stopping Jobs

```bash
# Graceful stop (SIGTERM)
jb stop abc123

# Force kill (SIGKILL)
jb stop abc123 --force
```

## Waiting for Completion

```bash
# Block until job finishes
jb wait abc123

# With timeout
jb wait abc123 --timeout 5m
```

Exit codes:

- `0` - Job completed successfully
- `1` - Job failed
- `124` - Timeout reached (job still running)

## Patterns

### Fire and Forget

```bash
jb run "make build" --name build
# Continue with other work...
```

### Run Multiple in Parallel

```bash
jb run "npm test" --name tests
jb run "npm run lint" --name lint
jb run "npm run typecheck" --name types

# Check results later
jb list
```

### Wait for Multiple Jobs

```bash
jb wait tests && jb wait lint && jb wait types
```

### Check Project Jobs After Break

```bash
# See what's running/completed in this project
jb list

# Check specific job output
jb logs <id> --tail
```

### Retry Failed Job

```bash
jb retry abc123
# Creates new job with same command/config
```

## When NOT to Use

- Quick commands (<10 seconds)
- Interactive commands requiring TTY
- Commands that need user input

## Storage

Jobs are stored in `~/.jb/`:

- `job.db` - SQLite database
- `logs/` - Job output files

Clean up old jobs:

```bash
jb clean                    # Remove jobs older than 7 days
jb clean --older-than 1d    # Custom retention
jb clean --all              # Remove all non-running jobs
```
