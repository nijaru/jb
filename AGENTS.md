# Agent Instructions

## Project

Background job manager for AI agents. Rust CLI tool.

## Development

```bash
cargo build              # Dev build
cargo clippy -- -D warnings  # Lint
cargo fmt                # Format
cargo test               # Tests (none yet)
```

## Release Process

Releases are automated via GitHub Actions. **Do not run `cargo publish` manually.**

1. Bump version in `Cargo.toml`
2. Update `ai/STATUS.md` with release notes
3. Commit: `git commit -m "Bump version to X.Y.Z"`
4. Push: `git push`
5. Trigger release: Go to Actions → Release → Run workflow
   - Or: `gh workflow run release.yml`
   - Use `dry-run: true` to test without publishing

The workflow:

- Verifies version isn't already on crates.io
- Runs fmt, clippy, build
- Builds binaries for linux (x86_64, aarch64) and macos (x86_64, aarch64)
- Publishes to crates.io via OIDC

## Testing

Manual testing only currently. Key scenarios:

- `jb run "cmd"` - basic job start
- `jb run "cmd" --follow` - stream output
- `jb logs <id> --follow` - attach to running job
- `jb list` - verify EXIT column shows codes
- Exit code propagation (run job that exits non-zero)
