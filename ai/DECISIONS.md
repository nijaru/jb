# Decisions

## 2024-12-16: Single directory over XDG

**Context**: Where to store job state, logs, config?

**Options**:

1. XDG paths (scatter across ~/.local/share, ~/.config, ~/.local/state, /run)
2. Single directory (~/.job/)

**Decision**: Single directory `~/.job/`

**Rationale**:

- Easy to discover: `ls ~/.job/`
- Easy to clean: `rm -rf ~/.job/`
- Precedent: cargo, rustup, docker all use single dotdir
- XDG is "correct" but user-hostile for simple tools

---

## 2024-12-16: Project-scoped by default

**Context**: How to handle multiple projects running jobs in parallel?

**Decision**: Auto-detect project via git root, `job list` shows current project by default.

**Rationale**:

- Agents often work within a project context
- Avoids confusion when running multiple Claude instances
- `job list --all` available for cross-project view
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

**Context**: How should agents learn to use `job`?

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

**Decision**: No init. Auto-create ~/.job/ on first use.

**Rationale**:

- Zero friction for first use
- Agents can't handle prompts
- `job skills install` is the only setup command (opt-in)
