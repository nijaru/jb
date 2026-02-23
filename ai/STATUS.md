# Status

**Version**: 0.0.14 (released)
**Phase**: Idle

## Current State

0.0.14 released. 96 tests passing, clean build.

CI and release workflows hardened: concurrency groups, cargo test in release verify, env vars for interpolated values, binary verification step.

## Platforms

| Platform        | Build | Manual Tests |
| --------------- | ----- | ------------ |
| macOS (arm64)   | Pass  | Pass         |
| Linux (x86_64)  | Pass  | CI only      |
| Linux (aarch64) | Pass  | CI only      |
