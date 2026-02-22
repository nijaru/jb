# Status

**Version**: 0.0.13 (released) — 0.0.14 pending
**Phase**: Post-review correctness fixes

## Current State

0.0.14 is ready to release. All fixes committed, tests passing (55/55), clean build.

## Changes in 0.0.14 (unreleased)

| Fix                                              | Severity | File             |
| ------------------------------------------------ | -------- | ---------------- |
| Prefix match non-deterministic                   | ERROR    | db.rs            |
| clean --status running deletes live jobs         | ERROR    | db.rs            |
| Spawn failure leaves job Pending forever         | ERROR    | spawner.rs       |
| DB update errors silently swallowed              | ERROR    | spawner.rs       |
| PID 0 stored on spawn edge case                  | ERROR    | spawner.rs       |
| unwrap() panic in stop/wait on concurrent clean  | ERROR    | stop.rs, wait.rs |
| Paths::new() panics if HOME unset                | WARN     | paths.rs         |
| Log file race in jb clean                        | WARN     | clean.rs         |
| Mutex deadlock surface in interrupt_running_jobs | WARN     | state.rs         |
| Dead oneshot completion channel                  | WARN     | spawner.rs       |
| Response::UserError for structured IPC errors    | WARN     | ipc.rs, run.rs   |
| recover_orphans silently swallows errors         | WARN     | db.rs            |
| BufReader line count in status                   | NIT      | status.rs        |
| PAGER="less -R" split                            | NIT      | logs.rs          |
| &PathBuf → &Path                                 | NIT      | project.rs       |

## To Release 0.0.14

1. Bump version in Cargo.toml: `0.0.13` → `0.0.14`
2. Update CHANGELOG.md: move Unreleased to `[0.0.14] - <date>`
3. `git tag v0.0.14 && git push && git push --tags`
4. Wait for CI, then update Homebrew tap checksum

## Platforms

| Platform       | Build | Manual Tests |
| -------------- | ----- | ------------ |
| macOS (arm64)  | Pass  | Pass         |
| Linux (x86_64) | Pass  | CI only      |
