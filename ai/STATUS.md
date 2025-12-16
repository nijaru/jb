# Status

**Phase**: Initial scaffolding complete

## Completed

- [x] Project structure (Cargo workspace)
- [x] Core types (Job, Status, Database, IPC protocol)
- [x] CLI skeleton with all commands
- [x] Skills file
- [x] Design documentation
- [x] Renamed from `job` to `jb` (crate name conflict)

## In Progress

- [ ] Daemon implementation
- [ ] IPC communication
- [ ] Process spawning with setsid

## Not Started

- [ ] Process monitoring
- [ ] Crash recovery
- [ ] Timeout enforcement
- [ ] Log streaming (--follow)
- [ ] Auto-clean on daemon start
- [ ] Tests
- [ ] CI/CD
- [ ] Release binaries

## Known Issues

- `jb run` creates job in DB but doesn't execute (no daemon)
- `jb stop` attempts direct kill but daemon should handle this
- `jb logs --follow` not implemented
- `jb run --wait` not implemented

## Next Steps

1. Implement daemon IPC listener
2. Implement job spawning with process group
3. Implement process monitoring loop
4. Wire CLI to communicate with daemon
