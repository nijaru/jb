# Full Code Review — jb v0.0.13

Date: 2026-02-22
Reviewer: Claude Sonnet 4.6
Scope: All 23 source files
Build/test state: Clean build, 55 tests passing

---

## Correctness / Safety Issues

### [ERROR] src/daemon/spawner.rs:132 — PID 0 silently stored, then kills own process group

```rust
let pid = child.id().unwrap_or(0);
```

`child.id()` returns `None` only after the child has already been `wait()`ed or the handle was consumed. In `run_job`, the child was just spawned, so `None` here is actually unreachable in practice — but if it ever fires, `pid = 0` is stored in `RunningJob` and in the DB. Later, `kill_process_group(0, ...)` is guarded by `pid == 0` in `core/mod.rs:22`, so the kill itself is safe. However, `db.update_started(&job_id, 0)` records PID 0 in SQLite, and the orphan recovery's `is_process_alive(0)` returns `false`, causing the job to be falsely interrupted on daemon restart even while it is still running.

Fix: propagate the error rather than defaulting to 0.

```rust
// Before
let pid = child.id().unwrap_or(0);

// After
let pid = child.id().ok_or_else(|| anyhow::anyhow!("child PID unavailable after spawn"))?;
```

---

### [ERROR] src/daemon/spawner.rs:209–244 — DB update errors silently swallowed in hot path

All four `db.update_finished` calls inside `run_job`'s result handler use `let _ =`, discarding errors:

```rust
let _ = db.update_finished(&job_id, Status::Stopped, None);   // line 226
let _ = db.update_finished(&job_id, Status::Stopped, None);   // line 226 (Timeout branch)
let _ = db.update_finished(&job_id, status, exit_code);       // line 240
```

If SQLite write fails (disk full, locked, etc.) the job remains permanently in "running" state in the DB with no log entry. The orphan recovery on next restart will mark it "interrupted" — a different status than the real outcome. This is a silent data corruption scenario.

Fix: propagate or at minimum log the error.

```rust
// Before
let _ = db.update_finished(&job_id, Status::Stopped, None);

// After
if let Err(e) = db.update_finished(&job_id, Status::Stopped, None) {
    error!("Failed to update job {} status: {}", job_id, e);
}
```

The same applies to `state.rs:77` and `spawner.rs:276` (stop_job path).

---

### [ERROR] src/commands/logs.rs:317–336 — Signal handler registered via unsafe global, only first registration takes effect

```rust
static HANDLER: std::sync::OnceLock<Box<dyn Fn() + Send + Sync>> = OnceLock::new();
// ...
let _ = HANDLER.set(Box::new(handler));  // Silently ignored on second call
```

`OnceLock::set` returns `Err` if already set — the `let _` discards this. If `follow_logs` is somehow called twice in a process (e.g., future refactor), the second Ctrl+C handler is silently dropped. More critically, using `OnceLock` in a static means the handler from a prior invocation of `follow_logs` in the same process persists. For a CLI tool this is usually fine, but it's fragile.

Additionally, calling `signal()` from an `unsafe` block with a global function pointer that calls into safe Rust (`h()`) is technically correct here, but the comment ("Simple Ctrl+C handler without adding ctrlc dependency") is misleading — the `ctrlc` crate is not a dependency, but `nix` (already a dependency) is being used directly. The implementation is correct but fragile.

Fix: use `ctrlc = "3"` (one crate, one line) or assert that `HANDLER.set` succeeds.

---

### [ERROR] src/daemon/spawner.rs:162–208 — Stopped branch after timeout SIGTERM is wrong

In the timeout branch, after sending SIGTERM:

```rust
tokio::select! {
    biased;
    _ = stop_rx.changed() => JobResult::Stopped,   // ← WRONG: this is a timeout kill
    status = child.wait() => JobResult::Completed(status.ok()),
    () = tokio::time::sleep(...) => {
        kill_process_group(pid, true);
        JobResult::Timeout
    }
}
```

If `stop_job` fires while we're in the SIGTERM grace period (rare but possible — user runs `jb stop` while timeout is active), the code returns `JobResult::Stopped`. In the Stopped handler, the code comments "stop_job already updated DB" and just signals completion. But at this point, `stop_job` may not have run yet (the stop signal is from `stop_rx.changed()`, not from `stop_job` itself). The DB update in `stop_job` and the DB update in `run_job`'s Timeout/Stopped handler both write to the same row without coordination, which is a TOCTOU issue. It's benign because SQLite serializes writes, but the final status depends on write ordering rather than actual outcome.

This is low-impact in practice but is a correctness concern worth documenting.

---

### [ERROR] src/commands/stop.rs:30, 56 — Unwrap on DB get after successful stop

```rust
let updated = db.get(&job.id)?.unwrap();  // line 30
let updated = db.get(&job.id)?.unwrap();  // line 56
```

The job was just read from the DB two lines earlier, but `.unwrap()` on a fresh `.get()` can still panic if the DB is concurrently cleaned (`jb clean` in another process) between the two reads. This is a real-world failure mode for a tool used by AI agents.

Fix: use `ok_or_else` or propagate properly.

```rust
// Before
let updated = db.get(&job.id)?.unwrap();

// After
let updated = db.get(&job.id)?.ok_or_else(|| anyhow::anyhow!("job disappeared"))?;
```

Same pattern in `commands/wait.rs:51`.

---

### [ERROR] src/core/db.rs:82–86 — Prefix query can return wrong job (ambiguous LIKE match)

```rust
"SELECT * FROM jobs WHERE id = ?1 OR id LIKE ?2 || '%'"
```

With 4-char IDs and a busy system, a 1- or 2-char prefix can match multiple jobs. SQLite will return whichever row the query plan picks — typically the first by rowid (oldest). There is no `ORDER BY` or `LIMIT 1`, so the result is indeterminate if two jobs share a prefix.

The `resolve()` method used to look up jobs by name falls back to prefix matching via `get()`, which means `jb stop ab` could stop a different job than the user expects.

Fix: add `LIMIT 1 ORDER BY created_at DESC` or error on ambiguous prefix.

```sql
-- Before
SELECT * FROM jobs WHERE id = ?1 OR id LIKE ?2 || '%'

-- After
SELECT * FROM jobs WHERE id = ?1 OR id LIKE ?2 || '%' ORDER BY created_at DESC LIMIT 1
```

Or: fail with "ambiguous prefix, use full ID" when >1 match.

---

### [WARN] src/daemon/state.rs:67–78 — Deadlock potential: holding two Mutex guards simultaneously

```rust
pub fn interrupt_running_jobs(&self) {
    let mut running = self.running_jobs.lock().unwrap();  // lock 1
    let db = self.db.lock().unwrap();                      // lock 2
    // ...
}
```

Acquiring two `Mutex` locks without a consistent order is a deadlock hazard. Any other code path that acquires `db` then `running_jobs` (or vice versa) in the opposite order will deadlock. Currently no other function holds both simultaneously, but this is an invariant that is enforced only by inspection, not by the type system.

In practice, `interrupt_running_jobs` is only called once during shutdown when no requests are being serviced, so this is low-risk today but fragile.

Fix: drain `running_jobs` first, drop its lock, then lock `db`.

```rust
pub fn interrupt_running_jobs(&self) {
    let drained: Vec<_> = {
        let mut running = self.running_jobs.lock().unwrap();
        running.drain().collect()
    };  // running lock released here
    let db = self.db.lock().unwrap();
    for (id, job) in drained {
        // ...
    }
}
```

---

### [WARN] src/daemon/spawner.rs:143 — completion_rx dropped immediately, oneshot is useless

```rust
let (completion_tx, _completion_rx) = oneshot::channel();
```

`_completion_rx` is dropped immediately. The sender `completion_tx` is stored in `RunningJob::completion_tx` and fired by `signal_completion()`. Nobody ever waits on the receiver. The entire `completion_tx/rx` mechanism in `RunningJob` is inert infrastructure.

If `wait_for_job` was intended to use this channel (instead of polling), it never does — it polls the DB every 100ms. This is not a bug, but is dead code that adds confusion and suggests an incomplete design.

Fix: either implement event-based waiting using the channel, or remove it entirely.

---

### [WARN] src/core/db.rs:183–189 — delete_old status filter rebuilds entire SQL string, losing non-running guard

```rust
if let Some(s) = status {
    sql = String::from("DELETE FROM jobs WHERE created_at < ?1 AND status = ?2");
    // ← replaces the original SQL that had status IN ('completed','failed','stopped','interrupted')
    params_vec.push(Box::new(s.as_str().to_string()));
}
```

When a `status` filter is provided, the new SQL replaces the original one entirely — including the `NOT IN ('running','pending')` guard. A caller could now delete running jobs with `jb clean --status running` (if Status::Running were accepted by the CLI). The CLI currently parses the status filter and accepts any `Status` value, including `Running`. Running `jb clean --status running` would delete running jobs from the DB while they are still executing — orphaning their processes and log files permanently.

Fix: always include the non-terminal guard, and add a status clause on top.

```sql
-- When status filter present:
DELETE FROM jobs WHERE created_at < ?1 AND status = ?2 AND status NOT IN ('running', 'pending')
```

Or reject non-terminal status values at the command level.

---

### [WARN] src/commands/logs.rs:206–315 — follow_logs uses blocking I/O and sleep on async thread

`follow_logs` is called from synchronous `execute()` which is called from `main`'s `run()` — but `run()` is `async fn` inside `#[tokio::main]`. Calling `std::thread::sleep` (line 237, 313) inside an async context blocks the Tokio thread for 100ms intervals. Since `follow` mode occupies the CLI process exclusively (it's the main future), this is benign in practice but violates Tokio's threading model. If this code is ever moved into a `tokio::spawn`'d task it will block the executor.

Fix: use `tokio::time::sleep` and make `follow_logs` async, or clearly document the sync-only constraint.

---

### [WARN] src/daemon/mod.rs:14–17 — TOCTOU between PID file check and socket removal

```rust
if let Some(existing_pid) = check_existing_daemon(&paths) {
    bail!("Daemon already running with PID {existing_pid}");
}
// ... write PID file ...
if paths.socket().exists() {
    std::fs::remove_file(paths.socket())?;
}
```

Two daemon processes started simultaneously can both pass the `check_existing_daemon` gate before either writes its PID file. Both then try to remove the socket and bind to it; whichever wins the `UnixListener::bind` survives, but both wrote PID files with different content. The losing daemon will crash with a bind error, but the PID file will contain the losing daemon's PID. Subsequent invocations will see a running PID and refuse to start.

This race is inherent to the PID-file pattern and is unlikely in practice, but worth noting. Fix would require advisory file locking (`flock`).

---

## Quality / Refactoring Issues

### [WARN] src/core/paths.rs:11–13 — expect() panics if HOME is unset

```rust
let root = dirs::home_dir()
    .expect("could not determine home directory")
    .join(".jb");
```

`dirs::home_dir()` returns `None` in containers, CI environments, or when `HOME` is explicitly unset. This panics with no useful context rather than returning a `Result`. Every `Paths::new()` call (in every command) can panic for this reason.

Fix: propagate as an error. Requires changing callers that hold `Paths` by value.

---

### [WARN] src/daemon/spawner.rs:86–90 — spawn error logged but caller sees Response::Job (pending)

```rust
tokio::spawn(async move {
    if let Err(e) = run_job(...).await {
        error!("Job {} failed to spawn: {}", job_id, e);
    }
});
// Return the job (still pending, will update to running shortly)
Response::Job(Box::new(job))
```

If `run_job` fails immediately (e.g., bad `cwd`, permission denied on log file, `sh` not found), the error is logged but the job remains `Pending` in the DB permanently. The caller gets `Response::Job` indicating success. `jb status <id>` will show "pending" indefinitely. There is no mechanism to transition the job to `Failed` on spawn error.

Fix: on `run_job` error, update DB to `Failed` before returning from the spawn task.

```rust
tokio::spawn(async move {
    if let Err(e) = run_job(&state_clone, job_id.clone(), command, cwd, timeout_secs).await {
        error!("Job {} failed to spawn: {}", job_id, e);
        let db = state_clone.db.lock().unwrap();
        let _ = db.update_finished(&job_id, Status::Failed, None);
    }
});
```

---

### [WARN] src/core/db.rs:296–313 — recover_orphans swallows all errors silently

```rust
pub fn recover_orphans(&self) {
    let orphans = self
        .list(Some(Status::Running), None)
        .unwrap_or_default()   // swallowed
        ...
    for job in orphans {
        // ...
        let _ = self.update_finished(&job.id, Status::Interrupted, None);  // swallowed
    }
}
```

If the DB is corrupt or locked, `recover_orphans` silently does nothing. Called on every `list`, `status`, `wait`, and `logs` invocation — if it silently fails, stale "running" jobs persist forever. The function returns `()` with no error signal.

Fix: return `anyhow::Result<()>` and let callers decide whether to propagate or warn.

---

### [WARN] src/commands/run.rs:64–66 — String pattern matching to detect user errors from daemon

```rust
if e.starts_with("Name '") && e.contains("is in use") {
    anyhow::bail!(crate::core::UserError::new(e));
}
```

This is brittle string matching against an error message formatted in `spawner.rs`. If the message ever changes, the `UserError` wrapping breaks silently. The daemon's `Response::Error` is a plain string — there is no structured error variant for "name in use" vs "DB error" vs "permission denied".

Fix: add structured error variants to `Response` (e.g., `Response::UserError(String)`) so the client can pattern match on type, not text.

---

### [WARN] src/commands/clean.rs:22–35 — Log file cleanup races with active jobs

```rust
let jobs = db.list(None, None)?;
let job_ids: HashSet<_> = jobs.iter().map(|j| j.id.as_str()).collect();

for entry in std::fs::read_dir(&log_dir)? {
    let entry = entry?;
    if let Some(stem) = path.file_stem().and_then(|s| s.to_str())
        && !job_ids.contains(stem)
    {
        let _ = std::fs::remove_file(&path);
    }
}
```

The DB query and the filesystem cleanup are not atomic. A job spawned between the `db.list` call and the `read_dir` call will have created its log file, but its ID will not be in `job_ids`. The log file will be deleted while the job is running — all output is lost, but the job continues writing to the (now-deleted) file descriptor. The daemon keeps the FD open, so the process doesn't fail, but `jb logs <id>` will show nothing.

Fix: only delete log files whose corresponding DB entry is truly absent (not just "not in the list from a snapshot"). Cross-check after deletion, or hold the daemon's cooperation via an IPC message.

---

### [WARN] src/commands/status.rs:52–53 — Reads entire log file to count lines

```rust
let lines = std::fs::read_to_string(&log_path)?.lines().count();
```

For a long-running job with gigabytes of output, this loads everything into memory just to count newlines. Use `BufReader` line counting instead.

```rust
let file = std::fs::File::open(&log_path)?;
let lines = std::io::BufReader::new(file).lines().count();
```

---

### [NIT] src/core/db.rs:275 — Infallible unwrap on sorted non-empty Vec

```rust
Ok(by_name.into_iter().next().unwrap())
```

The `is_empty()` check two lines above guarantees this is safe, but the pattern is fragile. Use `into_iter().next().expect("checked non-empty above")` or restructure to avoid the Option entirely.

---

### [NIT] src/core/db.rs:129 — Dynamic SQL with LIMIT injected via format! (not a SQL injection risk here, but smell)

```rust
let _ = write!(sql, " LIMIT {n}");
```

`n` is a `usize` parsed from CLI input — no injection risk. But using parameterized form would be more consistent. SQLite supports `LIMIT ?` via parameters. The `let _` on `write!` is also unnecessary since `String` write never fails.

---

### [NIT] src/commands/logs.rs:85–86 — PAGER env var split is naive

```rust
let pager = std::env::var("PAGER").unwrap_or_else(|_| "less".to_string());
let pager_args: Vec<&str> = if pager == "less" { vec!["-R"] } else { vec![] };
```

If `PAGER` is set to `"less -R"` (common in dotfiles), the entire string is treated as the binary name and the spawn fails, falling back to stdout. `Command::new("less -R")` will fail with "No such file or directory". This is a known footgun with `PAGER`.

Fix: split on whitespace, use first token as binary and rest as args, or use `sh -c "$PAGER"`.

---

### [NIT] src/core/project.rs:5 — &PathBuf parameter should be &Path

```rust
pub fn detect_project(cwd: &PathBuf) -> PathBuf {
```

`&PathBuf` is always better as `&Path` (more general, same cost). Clippy will flag this as `clippy::ptr_arg`.

---

### [NIT] src/daemon/spawner.rs:13–14 — Suppress of legitimate lint without justification

```rust
#[allow(clippy::unused_async)]
pub async fn spawn_job(...) -> Response {
```

`spawn_job` is `async` but contains no `.await` — the lint is correct. The function should either be made sync (no `async`) or the `await` should be explained. Making it sync requires updating the call site in `server.rs` which does `.await` it, but `async fn` returning a non-`Future`-wrapping value is a zero-cost abstraction in tokio anyway. However the suppress hides a real code smell.

---

## Summary: Prioritized Refactoring Plan

### P1 — Must fix (correctness bugs)

1. `spawner.rs:132` — PID 0 stored in DB on spawn, causes false orphan recovery. Propagate error.
2. `spawner.rs:226,240` — DB update failures silently swallowed; job stuck in "running". Log errors.
3. `spawner.rs:86–90` — Spawn failure leaves job in Pending forever. Update to Failed on error.
4. `db.rs:82–86` — Ambiguous prefix match; determinism not guaranteed. Add ORDER BY + LIMIT 1.
5. `db.rs:183–189` — `jb clean --status running` can delete running jobs. Add terminal-only guard.
6. `stop.rs:30,56`, `wait.rs:51` — Unwrap on re-fetched job can panic. Use `ok_or_else`.

### P2 — Should fix (robustness/design)

7. `state.rs:67–78` — Two-mutex acquire order; drain first to eliminate deadlock surface area.
8. `spawner.rs:143` — Dead oneshot channel infrastructure. Remove or implement event-based wait.
9. `run.rs:64–66` — Fragile string matching to classify daemon errors. Add structured Response variant.
10. `clean.rs:22–35` — Race condition deletes log files of newly spawned jobs.
11. `paths.rs:11–13` — `expect()` panics if HOME unset. Return Result.
12. `logs.rs:206–315` — Blocking sleep in async context. Make follow_logs async.
13. `db.rs:296–313` — recover_orphans swallows all errors. Return Result.

### P3 — Nice to have

14. `status.rs:52` — Read entire file to count lines. Use BufReader.
15. `logs.rs:85–86` — PAGER split is naive, fails on `PAGER="less -R"`.
16. `project.rs:5` — `&PathBuf` should be `&Path`.
17. `spawner.rs:13–14` — Remove `#[allow(clippy::unused_async)]` and make function sync.

---

## Verdict

The codebase is clean, well-structured, and reads clearly. The overall design (daemon + SQLite + IPC + file-based log streaming) is sound for its stated purpose.

The most pressing issues are correctness bugs rather than safety issues: the PID-0 scenario, spawn-failure leaving jobs in Pending, ambiguous prefix matching, and the `clean --status running` data hazard. These are all reachable in practice and none require adversarial input.

The deadlock potential in `interrupt_running_jobs` and the silent error swallowing in the DB update path are the highest-severity structural issues.

Nothing here is a security vulnerability given the tool runs as the current user on a local machine with no network exposure beyond the Unix socket. The socket is in `~/.jb/` which is user-owned, so no privilege escalation surface.

After fixing P1, this is production-quality for its scope.
