# Decisions

## 2026-02-22: Response::UserError for structured IPC errors

**Context**: `spawn_job` returned `Response::Error("Name 'X' is in use by running job Y")`. The client in `run.rs` detected this by string-matching the prose message with `e.starts_with("Name '") && e.contains("is in use")`. Any rewording of the message would silently break the classification.

**Decision**: Add `Response::UserError(String)` variant to the IPC protocol. Daemon returns `UserError` for errors the user can act on (currently: name in use). Client pattern-matches the variant, not the message text.

**Rationale**:

- Structural — variant determines behavior, message is display-only
- Extensible — future user-recoverable errors use the same path
- Cheap — one new enum variant, no protocol version bump needed (same binary)

---

## 2026-02-22: Paths::new() returns Result

**Context**: `dirs::home_dir()` can return `None` in containers or minimal environments where HOME is unset. The original code called `.expect()`, causing a panic with an opaque message on every command.

**Decision**: `Paths::new()` returns `anyhow::Result<Self>`. All 10 callers propagate with `?`. `Default` impl removed (callers all use explicit construction).

**Rationale**:

- Panic is wrong for a predictable, recoverable error condition
- All callers already propagate `Result` — adding `?` is zero friction
- Error message ("could not determine home directory") surfaces cleanly to the user

---

## 2026-02-22: Per-file DB query in jb clean log sweep

**Context**: `jb clean` took a DB snapshot then scanned the log directory, deleting files whose stem wasn't in the snapshot. A job spawned between the snapshot and the scan would have its log file deleted (TOCTOU race).

**Decision**: Query `db.get(stem)` per file instead of comparing against a snapshot.

**Rationale**:

- Eliminates the race window entirely
- Orphaned log files are rare so the extra queries are negligible
- Simpler code — no HashSet construction or lifetime management

## 2024-12-16: Renamed from `job` to `jb`

**Context**: Crate name `job` already exists on crates.io.

**Decision**: Rename to `jb`

**Rationale**:

- Short, fast to type (2 chars)
- No crate conflict
- Unix philosophy of short commands
- Binary: `jb`, Directory: `~/.jb/`

---

## 2024-12-16: Single directory over XDG

**Context**: Where to store job state, logs, config?

**Options**:

1. XDG paths (scatter across ~/.local/share, ~/.config, ~/.local/state, /run)
2. Single directory (~/.jb/)

**Decision**: Single directory `~/.jb/`

**Rationale**:

- Easy to discover: `ls ~/.jb/`
- Easy to clean: `rm -rf ~/.jb/`
- Precedent: cargo, rustup, docker all use single dotdir
- XDG is "correct" but user-hostile for simple tools

---

## 2024-12-16: Project-scoped by default

**Context**: How to handle multiple projects running jobs in parallel?

**Decision**: Auto-detect project via git root, `jb list` shows current project by default.

**Rationale**:

- Agents often work within a project context
- Avoids confusion when running multiple Claude instances
- `jb list --all` available for cross-project view
- No config needed - detection is automatic

---

## 2024-12-16: `stop` over `cancel`/`kill`

**Context**: What to call the command that terminates jobs?

**Decision**: `stop` with `--force` flag

**Rationale**:

- `stop` is intuitive regardless of job state (pending or running)
- `--force` for SIGKILL is explicit
- Matches Docker/systemd mental model
- Single command, no need to check state first

---

## 2024-12-16: Rust over Go/TypeScript

**Context**: Implementation language choice.

**Decision**: Rust

**Rationale**:

- Process management requires reliability (signals, process groups, crash recovery)
- Compiler enforces handling edge cases
- Single binary distribution
- Cross-platform from day 1
- `nix` crate for POSIX, `tokio` for async daemon

---

## 2024-12-16: No config file for v1

**Context**: What settings should be configurable?

**Decision**: No config file. Sensible defaults only.

**Rationale**:

- YAGNI - no clear need identified
- Timeout: per-job flag
- Retention: 7 days is reasonable default
- Paths ready for config.toml if needed later

---

## 2024-12-16: No `init` command

**Context**: Should users run a setup command?

**Decision**: No init. Auto-create ~/.jb/ on first use.

**Rationale**:

- Zero friction for first use
- Agents can't handle prompts

---

## 2024-12-17: Single crate with daemon subcommand

**Context**: Originally had 3 crates (jb, jbd, jb-core). `cargo install jb` didn't install daemon.

**Decision**: Merge into single `jb` crate with hidden `jb daemon` subcommand.

**Rationale**:

- `cargo install jb` installs everything needed
- No separate daemon binary to distribute
- Client spawns `jb daemon` instead of looking for `jbd`
- Simpler release process (one crate vs three)

---

## 2024-12-17: Short 4-char alphanumeric IDs

**Context**: ULID generated 26-char uppercase IDs like `01KCQ0FPDS6ZYKMSV076QX9HTA`.

**Decision**: 4-char lowercase alphanumeric IDs like `a3x9`.

**Rationale**:

- Easier to type and remember
- 1.6M combinations (36^4) is plenty for job tracking
- 100 collision retries handles edge cases
- Matches beads-style short IDs
- Lowercase is friendlier than UPPERCASE

---

## 2024-12-17: Orphan recovery marks jobs as "interrupted"

**Context**: If daemon crashes, jobs in DB stay "running" forever.

**Decision**: On startup, mark orphaned "running"/"pending" jobs as "interrupted".

**Rationale**:

- "Interrupted" is semantically correct (daemon tracking was interrupted)
- Can't re-attach to orphaned processes
- Don't know actual exit status, so can't mark completed/failed
- Clean slate for new daemon instance

---

## 2024-12-18: v0.0.3 feature selection - `--follow` only

**Context**: Agent feedback requested several features: job chaining (`--after`), job groups, list filters, graceful stop, separate stderr, and `--follow` for output streaming.

**Decision**: Implement only `--follow` (logs + run) and exit code in list. Skip the rest.

**Rationale**:

| Feature           | Verdict | Why                                          |
| ----------------- | ------- | -------------------------------------------- |
| `--follow`        | Add     | Essential for monitoring; no complexity cost |
| Exit code in list | Add     | Zero cost, saves a `status` call             |
| `--after <id>`    | Skip    | Shell chaining (`wait x && run y`) is better |
| Job groups        | Skip    | `cmd1 & cmd2 & wait` in shell is clearer     |
| List filters      | Skip    | `jb list \| grep x` works fine               |
| Graceful stop     | Skip    | SIGTERM default, SIGKILL --force is enough   |
| Separate stderr   | Skip    | Adds confusion; interleaved is standard      |

Unix philosophy: let the shell handle sequencing and filtering. Keep jb focused on job lifecycle.

---

## 2024-12-20: Simplified `jb list` - last 10 default, no project scoping

**Context**: `jb list` showed all jobs for current project, resulting in 39+ jobs cluttering output. Users primarily want to see recent activity, not full history.

**Decision**:

- Default: show last 10 jobs (any status)
- Remove project-based filtering entirely
- Add `-n <N>` for custom limit, `-a` for all, `--failed` shortcut

**New interface:**

| Command              | Shows                 |
| -------------------- | --------------------- |
| `jb list`            | Last 10 jobs          |
| `jb list -n 20`      | Last 20 jobs          |
| `jb list -a`         | All jobs              |
| `jb list --failed`   | Last 10 failed        |
| `jb list --status X` | Last 10 with status X |

**Rationale:**

1. **"Last 10" beats "running only"**: Running-only default hides what just finished. You kick off a build, come back, see "no running jobs" - now you need extra flags to see if it passed. Last 10 shows running jobs naturally (they're recent) plus recent results.

2. **Project scoping adds complexity without value**: For a personal job runner, you want to see YOUR jobs, period. If the list is long, that's what `-n` and `jb clean` are for. Removes `--here`, `--all` (project), `--global` mental overhead.

3. **`--failed` earns its shortcut**: Failures are actionable (debugging). Successes are expected - rarely need to filter for them. Use `--status completed` for that edge case.

4. **Consistent limit behavior**: Default limit of 10 applies everywhere. Override with `-n` or `-a`. Filters (`--failed`, `--status`) also respect the limit.

**Breaking changes:**

- `--all` now means "all jobs" (no limit), not "all projects"
- `--here` removed (no project scoping)
- Default behavior changed from "all statuses, current project" to "last 10, all projects"

---

## 2024-12-22: Event-based monitoring over polling

**Context**: Job completion was detected via 100ms polling loop with `try_wait()`.

**Decision**: Use `tokio::select!` with `child.wait()` for instant process exit detection.

**Rationale**:

- Instant completion detection (was 0-100ms latency)
- Lower CPU usage (no busy polling)
- Cleaner code structure with select! branches
- Stop signal handled via watch channel, integrates cleanly

---

## 2024-12-22: Graceful timeout escalation (SIGTERM → SIGKILL)

**Context**: Timeout handling immediately sent SIGKILL, giving processes no chance to cleanup.

**Decision**: On timeout, send SIGTERM first, wait 2s, then SIGKILL if still running.

**Rationale**:

- Processes that handle SIGTERM (cleanup temp files, flush buffers) get 2s to exit
- Well-behaved processes exit faster than previous instant-SIGKILL
- Stubborn processes still killed after 2s (not indefinite wait)
- Standard Unix practice (Docker, systemd use similar escalation)

---

## 2024-12-22: Seek-based `--tail` for large logs

**Context**: `jb logs <id> --tail N` loaded entire file into memory to get last N lines.

**Decision**: Scan backwards from end of file to find N newlines, then stream from that position.

**Rationale**:

- Works with GB-sized log files without memory issues
- Only reads ~8KB chunks from end of file
- Streaming output to stdout (no full file buffering)
- Same user-visible behavior, better resource usage

---

## 2024-12-23: Names unique while running, resolve to latest

**Context**: When multiple jobs share the same name, commands like `jb logs <name>` were ambiguous. Initial approach added `--latest` flag and auto-resolve logic, but this felt complex.

**Decision**: Simpler model inspired by Docker containers:

1. **Unique while running**: Can't create a job with name X if another job named X is running
2. **Released on completion**: Once a job finishes, its name is available again
3. **Resolve to latest**: `jb logs test` returns the most recent job named "test"

**Rationale**:

- Names behave like "handles" to the current job, not labels for categorization
- No ambiguity: running job owns the name, completed jobs accessible by ID or latest
- Simpler mental model than `--latest` flag and auto-resolve heuristics
- Similar to Docker container names (unique, but can be reused after removal)
