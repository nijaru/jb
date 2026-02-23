# Status

**Version**: 0.0.13 (released) — 0.0.14 pending
**Phase**: Release prep

## Current State

0.0.14 ready to release. Tests passing (55/55), clean build. Session added three
post-review cleanups (not in original 0.0.14 scope — include or bump to 0.0.15):

| Change             | Commit  |
| ------------------ | ------- |
| Remove --context   | b2f2778 |
| Add --dir to run   | b2f2778 |
| Default jb to list | b2f2778 |

## 0.0.14 Fix Summary

| Fix                                              | Severity |
| ------------------------------------------------ | -------- |
| Prefix match non-deterministic                   | ERROR    |
| clean --status running deletes live jobs         | ERROR    |
| Spawn failure leaves job Pending forever         | ERROR    |
| DB update errors silently swallowed              | ERROR    |
| PID 0 stored on spawn edge case                  | ERROR    |
| unwrap() panic in stop/wait on concurrent clean  | ERROR    |
| Paths::new() panics if HOME unset                | WARN     |
| Log file race in jb clean                        | WARN     |
| Mutex deadlock surface in interrupt_running_jobs | WARN     |
| Dead oneshot completion channel                  | WARN     |
| Response::UserError for structured IPC errors    | WARN     |
| recover_orphans silently swallows errors         | WARN     |
| BufReader line count in status                   | NIT      |
| PAGER="less -R" split                            | NIT      |
| &PathBuf → &Path                                 | NIT      |

## To Release 0.0.14

1. Bump version in Cargo.toml: `0.0.13` → `0.0.14`
2. Update CHANGELOG.md: move Unreleased to `[0.0.14] - 2026-02-22`
3. `git tag v0.0.14 && git push && git push --tags`
4. Wait for CI, then update Homebrew tap checksum

## Platforms

| Platform       | Build | Manual Tests |
| -------------- | ----- | ------------ |
| macOS (arm64)  | Pass  | Pass         |
| Linux (x86_64) | Pass  | CI only      |
