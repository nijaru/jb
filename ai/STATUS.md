# Status

**Version**: 0.0.2 (released)
**Phase**: Published to crates.io

## What Works

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

- No automated tests
- No signal handling for graceful daemon shutdown

## Next Steps

1. Real-world testing with Claude Code
2. Consider job chaining (`--after <id>`)
3. Add `--follow` for logs
