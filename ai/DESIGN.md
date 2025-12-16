# jb - Background Job Manager for AI Agents

## Overview

OS-agnostic background job framework designed for AI agents to manage long-running tasks independent of their host TUI/GUI.

## Architecture

```
┌─────────────┐     IPC      ┌─────────────┐     fork/exec    ┌─────────────┐
│   jb CLI    │◄────────────►│    jbd      │────────────────►│  job process │
└─────────────┘   Unix sock  └─────────────┘    (detached)   └─────────────┘
       │                            │
       │                            │
       ▼                            ▼
┌─────────────────────────────────────────┐
│               ~/.jb/                     │
│  ├── job.db        (SQLite)             │
│  ├── logs/         (job output)         │
│  ├── daemon.sock   (IPC)                │
│  └── daemon.pid    (PID file)           │
└─────────────────────────────────────────┘
```

## Core Principles

| Principle      | Implementation                                                   |
| -------------- | ---------------------------------------------------------------- |
| Agent-first    | JSON output, non-interactive, idempotent operations              |
| Zero-config    | Auto-creates ~/.jb/ on first use, sensible defaults              |
| Project-scoped | Jobs tagged with git root, `jb list` shows current project       |
| Reliable       | SQLite for state, daemon monitors processes, recovery on restart |
| Cross-platform | macOS, Linux, Windows (via Rust)                                 |

## Data Model

```rust
struct Job {
    id: Ulid,
    name: Option<String>,
    command: String,
    status: Status,
    project: PathBuf,      // Git root or cwd
    cwd: PathBuf,          // Working directory
    pid: Option<u32>,
    exit_code: Option<i32>,
    created_at: DateTime<Utc>,
    started_at: Option<DateTime<Utc>>,
    finished_at: Option<DateTime<Utc>>,
    timeout_secs: Option<u64>,
    context: Option<serde_json::Value>,
    idempotency_key: Option<String>,
}

enum Status {
    Pending,      // Queued
    Running,      // Executing
    Completed,    // Exit 0
    Failed,       // Exit != 0
    Stopped,      // User stopped
    Interrupted,  // Daemon lost track / crash recovery
}
```

## CLI Commands

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

## Process Lifecycle

1. `jb run "cmd"` sends request to daemon via IPC
2. Daemon spawns process with `setsid()` (new session, detached)
3. Daemon monitors via PID polling
4. Output captured to `~/.jb/logs/<id>.log`
5. On completion, daemon updates DB with exit code

### Daemon Crash Recovery

If daemon crashes:

- Jobs keep running (detached processes)
- On restart, daemon scans "running" jobs in DB
- Checks if PID still alive
- Alive → reattach monitoring
- Dead → mark `interrupted`

## Project Detection

```rust
fn detect_project(cwd: &Path) -> PathBuf {
    // Try git root first
    git rev-parse --show-toplevel
    // Fallback to cwd
}
```

- `jb list` defaults to current project
- `jb list --all` shows everything

## Storage

Single directory: `~/.jb/`

| File            | Purpose         |
| --------------- | --------------- |
| `job.db`        | SQLite database |
| `logs/<id>.log` | Job output      |
| `daemon.sock`   | IPC socket      |
| `daemon.pid`    | Daemon PID      |

No XDG scatter - easy to find, easy to clean.

## Agent Integration

Primary: **Skills** (portable markdown documentation)

- `jb skills install` → `~/.claude/skills/jb/skill.md`
- Works with Claude Code, Cursor, others adopting skills convention

Secondary: Good `--help` output as fallback.

Future: MCP server for structured tool access.

## Edge Cases

| Scenario             | Handling                            |
| -------------------- | ----------------------------------- |
| Daemon crash         | Jobs continue, recovered on restart |
| Job process crash    | Marked `failed` with exit code      |
| Machine reboot       | Jobs marked `interrupted`           |
| Duplicate submission | Use `--key` for idempotency         |
| Ambiguous name       | Error with list of matching IDs     |
| Disk full            | Logs truncated, job continues       |
| Timeout reached      | SIGTERM → wait → SIGKILL            |

## Tech Stack

- **Language**: Rust (reliability, cross-platform, single binary)
- **Database**: SQLite (rusqlite)
- **CLI**: clap
- **Async**: tokio
- **IPC**: Unix domain sockets
