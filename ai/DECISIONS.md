# Decisions

## 2024-12-16: Renamed from `job` to `jb`

**Context**: Crate name `job` already exists on crates.io.

**Decision**: Rename to `jb`

**Rationale**:

- Short, fast to type (2 chars)
- No crate conflict
- Unix philosophy of short commands
- Binary: `jb`, Directory: `~/.jb/`, Skills: `~/.claude/skills/jb/`

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

## 2024-12-16: Skills over MCP for agent integration

**Context**: How should agents learn to use `jb`?

**Decision**: Skills (markdown) as primary, MCP as future enhancement.

**Rationale**:

- Skills are portable across agent platforms
- Growing ecosystem adoption (Claude, Cursor, etc.)
- No infrastructure dependency
- Good `--help` as fallback for any agent

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
- `jb skill install` is the only setup command (opt-in)

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
