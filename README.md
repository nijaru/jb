# job

Background job manager for AI agents.

## Overview

`job` is an OS-agnostic CLI for managing long-running background tasks, designed specifically for AI agents. It allows agents to spawn tasks that survive session end, run in parallel, and be monitored from any context.

## Installation

```bash
cargo install --path crates/job-cli
```

## Quick Start

```bash
# Start a background job
job run "make build"

# List jobs in current project
job list

# Check job status
job status abc123

# View job output
job logs abc123 --tail

# Stop a job
job stop abc123
```

## Commands

| Command              | Purpose                     |
| -------------------- | --------------------------- |
| `job run <cmd>`      | Start background job        |
| `job list`           | List jobs (current project) |
| `job status [<id>]`  | Job or system status        |
| `job logs <id>`      | View output                 |
| `job stop <id>`      | Stop job                    |
| `job wait <id>`      | Block until done            |
| `job retry <id>`     | Re-run job                  |
| `job clean`          | Remove old jobs             |
| `job skills install` | Install Claude skills       |

## Agent Integration

Install skills for Claude Code:

```bash
job skills install
```

This installs documentation to `~/.claude/skills/job/` that teaches Claude how to use `job`.

## Storage

All data stored in `~/.job/`:

```
~/.job/
├── job.db        # SQLite database
├── logs/         # Job output files
├── daemon.sock   # IPC socket
└── daemon.pid    # Daemon PID
```

Clean up: `rm -rf ~/.job/`

## Status

Early development. See [ai/STATUS.md](ai/STATUS.md) for current state.

## License

MIT
