# jb - Modern nohup for AI Agents

## Overview

Simple background job manager. Like nohup but with tracking.

```bash
# nohup way
nohup cmd > /tmp/log-$$.txt 2>&1 &
echo $!  # remember this somehow

# jb way
jb run "cmd"  # returns: a3x9
jb logs a3x9
jb status a3x9
```

## Architecture

```
┌─────────────┐     IPC      ┌─────────────┐     fork/exec    ┌─────────────┐
│   jb CLI    │◄────────────►│ jb daemon   │────────────────►│  job process │
└─────────────┘   Unix sock  └─────────────┘    (detached)   └─────────────┘
       │                            │
       │                            │
       ▼                            ▼
┌─────────────────────────────────────────┐
│           ~/.local/share/jb/            │
│  ├── jobs.db       (SQLite)             │
│  ├── logs/         (job output)         │
│  ├── jbd.sock      (IPC)                │
│  └── jbd.pid       (PID file)           │
└─────────────────────────────────────────┘
```

Single binary: `jb daemon` is a hidden subcommand, auto-started by client.

## Core Principles

| Principle    | Implementation                                     |
| ------------ | -------------------------------------------------- |
| Modern nohup | Simple run/status/logs workflow                    |
| Agent-first  | JSON output, non-interactive, idempotent           |
| Zero-config  | Auto-creates dirs on first use                     |
| Minimal UI   | Last 10 jobs by default, clean interface           |
| Reliable     | SQLite state, daemon monitors, recovery on restart |

## Data Model

```rust
struct Job {
    id: String,           // 4-char alphanumeric (e.g., "a3x9")
    name: Option<String>,
    command: String,
    status: Status,
    project: PathBuf,     // Git root or cwd
    cwd: PathBuf,
    pid: Option<u32>,
    exit_code: Option<i32>,
    // timestamps, timeout, context, idempotency_key...
}

enum Status {
    Pending,      // Queued
    Running,      // Executing
    Completed,    // Exit 0
    Failed,       // Exit != 0
    Stopped,      // User stopped
    Interrupted,  // Daemon crash recovery
}
```

## CLI Commands

| Command                 | Purpose                  |
| ----------------------- | ------------------------ |
| `jb run <cmd>`          | Start background job     |
| `jb run <cmd> --follow` | Start + stream output    |
| `jb run <cmd> --wait`   | Start + wait silently    |
| `jb list`               | List last 10 jobs        |
| `jb list -n 20`         | List last 20 jobs        |
| `jb list -a`            | List all jobs            |
| `jb list --failed`      | List failed jobs         |
| `jb status [<id>]`      | Job or system status     |
| `jb logs <id>`          | View output              |
| `jb logs <id> --follow` | Stream output until done |
| `jb stop <id>`          | Stop job                 |
| `jb wait <id>`          | Block until done         |
| `jb retry <id>`         | Re-run job               |
| `jb clean`              | Remove old jobs          |
| `jb skill install`      | Install Claude skill     |

## Output Streaming (`--follow`)

Both `run` and `logs` support `--follow` for real-time output:

```bash
jb run "cargo build" --follow   # Start job, stream output, exit with job's code
jb logs abc1 --follow           # Attach to running job, stream until done
```

**Behavior:**

- Streams output line-by-line as it appears
- On job completion: exits with job's exit code
- On Ctrl+C: detaches cleanly, job continues running
- Works for running jobs and replays completed job output

**Implementation:**

- Tail log file with inotify/kqueue for new content
- Poll job status to detect completion
- Propagate exit code when job reaches terminal state

## Process Lifecycle

1. `jb run "cmd"` connects to daemon (starts if needed)
2. Daemon generates 4-char ID, spawns detached process
3. Output captured to `~/.local/share/jb/logs/<id>.log`
4. Daemon monitors via PID polling
5. On completion, updates DB with exit code

## Daemon Robustness

**Startup recovery:**

- Scans for jobs stuck in "running" or "pending" state
- Marks as "interrupted" (daemon lost track)

**Multiple daemon prevention:**

- PID file with process existence check
- Stale PID files cleaned automatically

## ID Generation

4-char lowercase alphanumeric (36^4 = 1.6M combinations):

- 100 collision retries before error
- Practically unlimited for typical use
- Easy to type and remember

## Tech Stack

- **Language**: Rust
- **Database**: SQLite (rusqlite)
- **CLI**: clap
- **Async**: tokio
- **IPC**: Unix domain sockets
