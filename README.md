# jb

Background job manager for AI agents.

## Overview

`jb` is an OS-agnostic CLI for managing long-running background tasks, designed specifically for AI agents. It allows agents to spawn tasks that survive session end, run in parallel, and be monitored from any context.

## Installation

```bash
cargo install --path crates/job-cli
```

## Quick Start

```bash
# Start a background job
jb run "make build"

# List jobs in current project
jb list

# Check job status
jb status abc123

# View job output
jb logs abc123 --tail

# Stop a job
jb stop abc123
```

## Commands

| Command             | Purpose                     |
| ------------------- | --------------------------- |
| `jb run <cmd>`      | Start background job        |
| `jb list`           | List jobs (current project) |
| `jb status [<id>]`  | Job or system status        |
| `jb logs <id>`      | View output                 |
| `jb stop <id>`      | Stop job                    |
| `jb wait <id>`      | Block until done            |
| `jb retry <id>`     | Re-run job                  |
| `jb clean`          | Remove old jobs             |
| `jb skills install` | Install Claude skills       |

## Agent Integration

Install skills for Claude Code:

```bash
jb skills install
```

This installs documentation to `~/.claude/skills/jb/` that teaches Claude how to use `jb`.

## Storage

All data stored in `~/.jb/`:

```
~/.jb/
├── job.db        # SQLite database
├── logs/         # Job output files
├── daemon.sock   # IPC socket
└── daemon.pid    # Daemon PID
```

Clean up: `rm -rf ~/.jb/`

## Status

Early development. See [ai/STATUS.md](ai/STATUS.md) for current state.

## License

MIT
