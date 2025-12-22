# Status

**Version**: 0.0.8 (released)
**Phase**: Published to crates.io + homebrew tap

## Current Work (v0.0.8)

| Task                           | Status    | Notes                                      |
| ------------------------------ | --------- | ------------------------------------------ |
| Linux testing on Fedora        | Completed | All features verified working              |
| Docs: --tail [N] clarification | Completed | Skill and README updated                   |
| Fix redundant help defaults    | Completed | clean --help no longer shows default 2x    |
| Improve clean output/help      | Completed | Single-line output, cleaner help text      |
| Add `ls` alias for list        | Completed | `jb ls` works, shown in help               |
| Add `-t` flag for clean        | Completed | `jb clean -t 1d` for --older-than          |
| Add `-a` flag for clean        | Completed | `jb clean -a` for --all, matches list      |
| Review all CLI help text       | Completed | All commands verified consistent           |
| Document SIGTERM limitation    | Completed | Shell wrapper issue in ai/SIGTERM_ISSUE.md |

## v0.0.7 Changes

| Feature                   | Status    | Notes                                     |
| ------------------------- | --------- | ----------------------------------------- |
| Graceful daemon shutdown  | Completed | SIGTERM/SIGINT handled                    |
| Interrupt running on exit | Completed | Jobs marked interrupted, processes killed |
| Socket/PID cleanup        | Completed | Files removed on exit                     |

## v0.0.6 (released)

| Feature                  | Status    | Notes                                    |
| ------------------------ | --------- | ---------------------------------------- |
| Fix UTF-8 truncation bug | Completed | Panic-safe multi-byte character handling |
| Add color output         | Completed | Status column colored by state           |
| Add shell completions    | Completed | `jb completions <shell> [--install]`     |
| Add CHANGELOG.md         | Completed | Track changes across releases            |

## v0.0.5 (released)

| Feature                | Status    | Notes                                   |
| ---------------------- | --------- | --------------------------------------- |
| List default: last 10  | Completed | Show recent jobs, not all               |
| Remove project scoping | Completed | Global by default, simpler mental model |
| Add `-n` limit flag    | Completed | `jb list -n 20` for custom limit        |
| Add `-a` all flag      | Completed | Show all jobs (no limit)                |
| Add `--failed` filter  | Completed | Shortcut for `--status failed`          |
| Add test suite         | Completed | 33 unit tests, CI runs cargo test       |

## v0.0.4 (released)

| Feature             | Status    | Notes                        |
| ------------------- | --------- | ---------------------------- |
| `skills` -> `skill` | Completed | Singular command (git-style) |
| Homebrew tap        | Completed | `brew install nijaru/tap/jb` |

## v0.0.3 Changes

| Feature             | Status    | Notes                         |
| ------------------- | --------- | ----------------------------- |
| `logs --follow`     | Completed | Stream output until job done  |
| `run --follow`      | Completed | Start + stream (resilient fg) |
| Exit code in `list` | Completed | Shows exit code column        |
| Docs updated        | Completed | README + skill                |

## What Works (v0.0.2)

| Command         | Status | Notes                      |
| --------------- | ------ | -------------------------- |
| `jb run "cmd"`  | Tested | Auto-starts daemon         |
| `jb run --wait` | Tested | Blocks, returns exit code  |
| `jb run -t 5s`  | Tested | Timeout kills job          |
| `jb run -k key` | Tested | Idempotency works          |
| `jb list`       | Tested | Per-project by default     |
| `jb status`     | Tested | System + job detail        |
| `jb logs`       | Tested |                            |
| `jb stop`       | Tested | Via daemon IPC             |
| `jb wait`       | Tested | Via daemon IPC             |
| `jb retry`      | Tested | Via daemon IPC             |
| `jb clean`      | Tested |                            |
| `--json`        | Tested | Valid JSON output          |
| Daemon recovery | Tested | Orphans marked interrupted |
| PID locking     | Tested | Prevents multiple daemons  |

## v0.0.2 Changes

- Short 4-char alphanumeric IDs (e.g., `a3x9`)
- Orphan job recovery on daemon restart
- Multiple daemon prevention via PID lock
- All clippy pedantic warnings fixed
- README improved (standalone description, not nohup-dependent)

## Platforms

| Platform       | Build | Manual Tests |
| -------------- | ----- | ------------ |
| macOS (arm64)  | Pass  | Pass         |
| Linux (x86_64) | Pass  | CI only      |

## Known Limitations

See `ai/SIGTERM_ISSUE.md` for shell wrapper SIGTERM issue.
