# Changelog

All notable changes to this project will be documented in this file.

## [0.0.12] - 2025-12-23

### Removed

- `jb skill` subcommand (not effective enough to justify maintenance)

## [0.0.11] - 2025-12-23

### Changed

- **Names unique while running**
  - Can't create job with `--name X` if another job named X is pending/running
  - Name is released when job completes, can be reused
  - `jb logs <name>` returns most recent job with that name

### Removed

- `--latest` flag (no longer needed, name always resolves to most recent)

## [0.0.10] - 2025-12-22

### Changed

- **Event-based job monitoring** replaces 100ms polling
  - Uses `tokio::select!` for instant process exit detection
  - Lower CPU usage, instant completion detection

- **Graceful timeout escalation**
  - Previously: Immediate SIGKILL on timeout
  - Now: SIGTERM → 2s wait → SIGKILL
  - Processes that handle SIGTERM get time to cleanup

- **Efficient `--tail` for large logs**
  - Previously: Loaded entire file into memory
  - Now: Seek-based backward scan, works with GB files

### Fixed

- Deduplicated `kill_process_group` to `core/` shared module

## [0.0.9] - 2025-12-21

### Fixed

- **Process group signaling for reliable job termination**
  - `jb stop` now uses `killpg` to signal entire process group
  - Previously: Only killed shell wrapper, leaving child processes running
  - Now: `jb run "source .env && ./app"` works without `exec` workaround
  - Timeout (`-t`) kills all children on expiry
  - Daemon shutdown cleanly terminates all process groups

- **Smart orphan recovery on daemon restart**
  - Checks if orphaned job's PID is still alive before marking interrupted
  - Live processes: kept as "running", can be stopped normally
  - Dead processes: correctly marked as "interrupted"
  - Recovery runs on: list, status, logs, wait commands

## [0.0.8] - 2025-12-21

### Added

- `jb ls` alias for `jb list`
- `-t` short flag for `jb clean --older-than`
- `-a` short flag for `jb clean --all`

### Changed

- Improved `jb clean` help text and output
- Refactored job resolution into shared helper (internal)

### Fixed

- `jb logs --follow` no longer reopens database every poll

## [0.0.7] - 2025-12-20

### Added

- Graceful daemon shutdown on SIGTERM/SIGINT
- Running jobs marked as interrupted on shutdown

### Fixed

- Cleanup of socket and PID files on daemon exit

## [0.0.6] - 2025-12-20

### Added

- Color output for job status in `jb list`
- Shell completions via `jb completions <shell> [--install]` (bash, zsh, fish)

### Fixed

- UTF-8 truncation bug that could panic on multi-byte characters

## [0.0.5] - 2025-12-20

### Changed

- `jb list` now shows last 10 jobs by default (was all jobs)
- Removed project-scoped filtering for simpler mental model

### Added

- `-n` flag to limit number of jobs shown
- `-a` flag to show all jobs
- `--failed` filter shortcut for `--status failed`
- Comprehensive test suite (33 unit tests)
- CI now runs `cargo test`

## [0.0.4] - 2025-12-19

### Added

- Homebrew tap: `brew install nijaru/tap/jb`

## [0.0.3] - 2025-12-18

### Added

- `jb logs --follow` to stream output until job completes
- `jb run --follow` to start and stream output
- Exit code column in `jb list`

## [0.0.2] - 2025-12-17

### Changed

- Short 4-character alphanumeric IDs (e.g., `a3x9`)
- Orphan job recovery on daemon restart
- Multiple daemon prevention via PID lock

### Fixed

- All clippy pedantic warnings

## [0.0.1] - 2025-12-16

Initial release.

### Added

- `jb run <cmd>` to start background jobs
- `jb run --wait` to block until completion
- `jb run -t <duration>` for timeout
- `jb run -k <key>` for idempotency
- `jb list` to show jobs
- `jb status [id]` for system/job status
- `jb logs <id>` to view output
- `jb stop <id>` to terminate jobs
- `jb wait <id>` to wait for completion
- `jb retry <id>` to re-run jobs
- `jb clean` to remove old jobs
- `--json` flag for structured output
- Daemon auto-start on first command
