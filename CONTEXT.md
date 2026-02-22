# Context

Background job manager for AI agents. Allows agents to spawn tasks that survive session end.

## Quick Orientation

| What                  | Where             |
| --------------------- | ----------------- |
| Architecture & design | `ai/DESIGN.md`    |
| Current status        | `ai/STATUS.md`    |
| Design decisions      | `ai/DECISIONS.md` |
| Tasks                 | `tk ls`           |

## Project Structure

```
jb/
├── src/
│   ├── main.rs       # CLI entry point
│   ├── client.rs     # Daemon client
│   ├── core/         # Types, DB, IPC protocol
│   ├── commands/     # CLI subcommands
│   └── daemon/       # Daemon implementation
└── ai/               # Design docs
```

## Key Files

| File                | Purpose                    |
| ------------------- | -------------------------- |
| `src/core/job.rs`   | Job struct and Status enum |
| `src/core/db.rs`    | SQLite operations          |
| `src/core/ipc.rs`   | Request/Response protocol  |
| `src/main.rs`       | CLI entry point            |
| `src/daemon/mod.rs` | Daemon entry point         |

## Commands

```bash
cargo build --release       # Build
./target/release/jb --help  # CLI help
tk ls                       # View tasks
```

## Releasing

**Always use the release workflow - never publish manually.**

```bash
# 1. Bump version in Cargo.toml
# 2. Commit and push
# 3. Wait for CI to pass
gh run list --limit 1

# 4. Trigger release workflow
gh workflow run release.yml

# 5. Watch release
gh run watch
```

The release workflow:

- Verifies version isn't already published
- Runs fmt/clippy checks
- Builds for linux (x86_64, aarch64) and macos (x86_64, aarch64)
- Publishes via trusted publishing (OIDC)
