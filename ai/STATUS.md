# Status

**Phase**: Core daemon implemented, ready for testing

## Completed

- [x] Project structure (Cargo workspace)
- [x] Core types (Job, Status, Database, IPC protocol)
- [x] CLI skeleton with all 9 commands
- [x] Skills file for Claude integration
- [x] Design documentation
- [x] Renamed from `job` to `jb` (crate name conflict)
- [x] GitHub repo: github.com/nijaru/jb
- [x] Beads initialized with tasks
- [x] **Daemon IPC listener** - Unix socket server accepting connections
- [x] **Job spawning with process groups** - `setsid()` equivalent via `process_group(0)`
- [x] **Process monitoring loop** - Polls `try_wait()`, handles timeout
- [x] **CLI daemon communication** - `jb run` talks to daemon when running

## What Works

- `jb run "cmd"` - sends to daemon if running, falls back to DB-only mode
- `jb list` - shows jobs for current project
- `jb status <id>` - shows job details
- `jb status` - shows system status (daemon state)
- `jb clean` - removes old jobs from DB
- `jbd` - daemon that executes jobs, captures output to `~/.jb/logs/<id>.log`

## What Needs Testing/Polish

- `jb run --wait` - wait for job completion
- `jb stop` - stop running job (uses `start_kill()`)
- `jb logs` - view job output
- `jb wait` - wait for existing job
- Timeout handling
- Error recovery (orphaned jobs)

## Build & Test

```bash
cargo build --release

# Start daemon
./target/release/jbd &

# Run a job
./target/release/jb run "echo hello" --name test
./target/release/jb list
./target/release/jb status

# View output
cat ~/.jb/logs/<job-id>.log
```
