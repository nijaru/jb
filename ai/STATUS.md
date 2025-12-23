# Status

**Version**: 0.0.11 (unreleased)
**Phase**: Name uniqueness

## Current Work

| Task                       | Status    | Notes                                            |
| -------------------------- | --------- | ------------------------------------------------ |
| Unique names while running | Completed | Can't create job with name if another is running |
| Name resolves to latest    | Completed | `jb logs test` → most recent job named "test"    |
| Clean error messages       | Completed | UserError type, no stack traces for user errors  |

## v0.0.10 (released)

| Task                           | Status    | Notes                                   |
| ------------------------------ | --------- | --------------------------------------- |
| Event-based job monitoring     | Completed | `tokio::select!` replaces 100ms polling |
| Graceful timeout escalation    | Completed | SIGTERM → 2s wait → SIGKILL             |
| Efficient `--tail` for logs    | Completed | Seek-based, works with GB files         |
| Deduplicate kill_process_group | Completed | Moved to `core/` shared module          |

## v0.0.9 (released)

| Task                         | Status    | Notes                                   |
| ---------------------------- | --------- | --------------------------------------- |
| Process group signaling      | Completed | `killpg` for all job termination        |
| No more `exec` workaround    | Completed | `source .env && ./app` works correctly  |
| Timeout kills process group  | Completed | `-t` flag kills all children on timeout |
| Daemon shutdown kills groups | Completed | Graceful interrupt signals entire group |
| Smart orphan recovery        | Completed | Checks PID liveness before marking dead |
| Safety: pid=0 check          | Completed | Prevents accidental self-kill           |

## v0.0.8 (released)

| Task                          | Status    | Notes                                    |
| ----------------------------- | --------- | ---------------------------------------- |
| Add `ls` alias for list       | Completed | `jb ls` works, shown in help             |
| Add `-t` flag for clean       | Completed | `jb clean -t 1d` for --older-than        |
| Add `-a` flag for clean       | Completed | `jb clean -a` for --all, matches list    |
| Logs --follow DB optimization | Completed | No longer reopens DB every poll          |
| Add `db.resolve()` helper     | Completed | Deduplicated job resolution code         |
| Add `db.count()` method       | Completed | Efficient count without loading all jobs |

## v0.0.7 (released)

| Feature                   | Status    | Notes                                     |
| ------------------------- | --------- | ----------------------------------------- |
| Graceful daemon shutdown  | Completed | SIGTERM/SIGINT handled                    |
| Interrupt running on exit | Completed | Jobs marked interrupted, processes killed |
| Socket/PID cleanup        | Completed | Files removed on exit                     |

## Platforms

| Platform       | Build | Manual Tests |
| -------------- | ----- | ------------ |
| macOS (arm64)  | Pass  | Pass         |
| Linux (x86_64) | Pass  | CI only      |
