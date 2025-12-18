# jb

Background job manager. Run commands that persist after your terminal closes.

## Install

```bash
cargo install jb
```

## Quick Start

```bash
$ jb run "cargo build --release"
a3x9

$ jb list
ID    STATUS     COMMAND
a3x9  running    cargo build --release

$ jb logs a3x9
   Compiling foo v0.1.0
   ...

$ jb status a3x9
Status: completed
Exit: 0
```

## Commands

| Command                 | Purpose                     |
| ----------------------- | --------------------------- |
| `jb run <cmd>`          | Start background job        |
| `jb run <cmd> --follow` | Start + stream output       |
| `jb run <cmd> --wait`   | Start + wait silently       |
| `jb list`               | List jobs (current project) |
| `jb logs <id>`          | View output                 |
| `jb logs <id> --follow` | Stream output until done    |
| `jb status <id>`        | Job details                 |
| `jb stop <id>`          | Stop job                    |
| `jb wait <id>`          | Block until done            |
| `jb retry <id>`         | Re-run job                  |
| `jb clean`              | Remove old jobs             |

## Features

- Short memorable IDs (`a3x9`)
- Per-project job tracking (via git root)
- JSON output (`--json`)
- Survives terminal disconnect
- Auto-starts daemon

## vs nohup

```bash
nohup cmd > /tmp/log-$$.txt 2>&1 &
echo $!

jb run "cmd"
jb logs <id>
```

## For AI Agents

```bash
jb skills install  # installs to ~/.claude/skills/jb/
```

## License

MIT
