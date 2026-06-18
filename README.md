# ⚓ harbor

See what's docked at every local port — dev servers, databases, and background
processes — **grouped by the project they belong to**, right in your terminal.

A fast, native (Rust) alternative to the npm-distributed port viewers. Single
self-contained binary, no Node runtime, built on the `lsof`/`ps` that already
ship with macOS.

```
⚓ harbor  4 dev · 4 system
▌● 3000   next-server   Next.js     98248  01-10:18:03  0.1  ~/Works/.../checkout
 ● 3009   node          Vite        92068  07:06:18     0.0  ~/Works/.../core-api
 ● 4319   bun                       14992  02-04:43:23  0.0  ~/My Projects/pr-comprehend-poc
 ● 8081   node          Expo        80385  09:01:37     0.1  ~/Works/.../caladan-mem
 · 5000   ControlCenter             1153   02-13:23:22  0.0  /
 ↑↓ move  x kill  / filter  a all/dev  r refresh  q quit
```

## Why

When you have a dozen dev servers running across worktrees, the hard part isn't
seeing *that* port 3000 is taken — it's knowing **which project** owns it.
harbor maps every listener to its working directory and framework, separates
your dev servers from system noise (AirPlay on :5000/:7000, OrbStack, etc.), and
lets you kill the right one without hunting for a PID.

## Usage

```
harbor            # interactive TUI (live, refreshes every 2s)
harbor --list     # print once and exit (good for scripts / piping)
harbor --help

# in the TUI
↑/↓ or j/k   move          x    kill selected (y = SIGTERM, K = SIGKILL)
/            filter         a    toggle system processes
r            refresh        q    quit
```

## Install

### Homebrew (recommended once published)

```sh
brew install FerMPY/tap/harbor
```

### From source

```sh
cargo install --path .          # installs to ~/.cargo/bin/harbor
# or just build and symlink:
cargo build --release
ln -sf "$PWD/target/release/harbor" ~/.local/bin/harbor
```

Requires Rust (`brew install rust`).

## How it works

- `lsof -nP -iTCP -sTCP:LISTEN` → every TCP listener + its port and PID
- `ps` (batched) → command, full args, %CPU, uptime
- `lsof -d cwd` → each process's working directory
- framework guess from the command line, falling back to the project's
  `package.json` dependencies

No daemon, no config, no telemetry.

## License

MIT
