# jb

[![Crates.io](https://img.shields.io/crates/v/jb)](https://crates.io/crates/jb)
[![CI](https://github.com/nijaru/jb/actions/workflows/ci.yml/badge.svg)](https://github.com/nijaru/jb/actions)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)

Run background jobs that survive terminal disconnect. Track status and retrieve output anytime.

## Install

```bash
# Homebrew
brew install nijaru/tap/jb

# Cargo
cargo install jb
```

## Quick Start

```bash
$ jb run "cargo build --release"
a3x9

$ jb list
ID         STATUS       EXIT   NAME         COMMAND
a3x9       running      -      -            cargo build --release

$ jb logs a3x9 --follow
   Compiling foo v0.1.0
   ...

$ jb status a3x9
Status: completed
Exit: 0
```

## Commands

| Command                 | Purpose                  |
| ----------------------- | ------------------------ |
| `jb run <cmd>`          | Start background job     |
| `jb run <cmd> --follow` | Start + stream output    |
| `jb run <cmd> --wait`   | Start + wait silently    |
| `jb list` (or `jb ls`)  | List last 10 jobs        |
| `jb list -n 20`         | List last 20 jobs        |
| `jb list -a`            | List all jobs            |
| `jb list --failed`      | List failed jobs         |
| `jb logs <id>`          | View output (colorized)  |
| `jb logs <id> --tail`   | Last 50 lines            |
| `jb logs <id> --tail N` | Last N lines             |
| `jb logs <id> --follow` | Stream output until done |
| `jb logs <id> --pager`  | View in pager (less -R)  |
| `jb status <id>`        | Job details              |
| `jb stop <id>`          | Stop job                 |
| `jb wait <id>`          | Block until done         |
| `jb retry <id>`         | Re-run job               |
| `jb clean`              | Remove old jobs          |

## Features

- Short memorable IDs (`a3x9`)
- Clean output (last 10 jobs by default)
- Color-coded status and logs (error/warn/info/debug)
- Shell completions (bash, zsh, fish)
- JSON output (`--json`)
- Survives terminal disconnect
- Auto-starts daemon
- Respects `NO_COLOR` environment variable

## vs nohup

```bash
nohup cmd > /tmp/log-$$.txt 2>&1 &
echo $!

jb run "cmd"
jb logs <id>
```

## Shell Completions

```bash
# Install once (recommended)
jb completions zsh --install
jb completions bash --install
jb completions fish --install

# Or generate to stdout
jb completions zsh > ~/.zsh/completions/_jb
```

## License

MIT
