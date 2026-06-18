# ⚓ harbor

[![CI](https://github.com/FerMPY/harbor/actions/workflows/ci.yml/badge.svg)](https://github.com/FerMPY/harbor/actions/workflows/ci.yml)
![Platforms: macOS · Linux](https://img.shields.io/badge/platforms-macOS%20%C2%B7%20Linux-blue)
[![License: MIT](https://img.shields.io/badge/license-MIT-green.svg)](LICENSE)

See what's docked at every local port — dev servers, databases, and Docker
containers — **grouped by the project they belong to**, right in your terminal.

A fast, native (Rust) alternative to npm-distributed port viewers. Single
self-contained binary, **no Node runtime**, cross-platform across **macOS and
Linux** via the `listeners` + `sysinfo` crates (no `lsof`/`ps` shell-outs).

```
⚓ harbor  4 dev · 0 system
▌● 3000   node          Next.js    98248  1d 11:30  0.0  53M  ~/Works/.../checkout  ⎇ feat/checkout-page
 ● 3009   node          Vite       51006  00:16:30  0.2  58M  ~/Works/.../core-api  ⎇ feat/cart-pricing
 ● 4319   bun                      14992  2d 05:54  0.0   7M  ~/My Projects/poc  ⎇ main   (orphaned)
 ● 5452   OrbStack…     postgres:16.8  56621  11:05  0.0  471M  primary-db-v3
 ↑↓ move  o open  x kill  / filter  a all/dev  r refresh  q quit
```

## Why

With a dozen dev servers across worktrees, the hard part isn't seeing *that*
port 3000 is taken — it's knowing **which project** owns it. harbor maps every
listener to its working directory, **git branch**, and framework; tags databases
and Docker containers; flags orphaned/zombie processes; and lets you kill the
right one without hunting for a PID.

## What it detects

- **Frameworks** — Next.js, Nuxt, Vite, Remix, Astro, SvelteKit, Angular, Solid,
  Qwik, Gatsby, Expo/Metro, NestJS, Express, Fastify, Koa, Hono, Django, Flask,
  FastAPI, Rails, Laravel, and more (from the command line + `package.json`).
- **Databases** — PostgreSQL, Redis, MongoDB, MySQL/MariaDB, Memcached, nginx,
  etc. (by process name and canonical port).
- **Docker** — maps host ports to container name + image via `docker ps`.
- **Health** — flags `orphaned` (reparented to init) and `zombie` processes.
- **Git branch** — worktree-aware, read straight from `.git/HEAD`.

## Usage

```
harbor                interactive TUI (live, refreshes every 2s)
harbor <port>         deep view of one port: process tree, branch, repo, mem
harbor ps | --list    print every listener once
harbor --json         machine-readable output (for scripts)
harbor kill <p> [-f]  kill by port / pid / range (3000, 42872, 3000-3010); -f = SIGKILL
harbor clean [-n][-f] reap orphaned/zombie dev processes (-n = preview)
harbor watch          stream port start/stop events
harbor --help

# in the TUI
↑/↓ or j/k   move          o    open http://localhost:<port>
x            kill (y=TERM, K=KILL)    /  filter    a  toggle system    r  refresh    q  quit
```

## Install

### Homebrew (recommended)

```sh
brew install FerMPY/tap/harbor
```

macOS and Linux. Installs a prebuilt binary — no Rust toolchain required.

### Prebuilt binary

Download the archive for your platform from the
[latest release](https://github.com/FerMPY/harbor/releases/latest), then put
`harbor` somewhere on your `PATH`:

```sh
tar -xzf harbor-aarch64-apple-darwin.tar.gz
mv harbor /usr/local/bin/        # or anywhere on your PATH
```

### From source

```sh
cargo install --git https://github.com/FerMPY/harbor   # latest from main
# or clone and build:
cargo build --release            # -> target/release/harbor
```

Building from source requires Rust (`brew install rust`, or `apt install cargo`
on Linux).

## How it works

- `listeners` crate → every TCP listener + its PID (netlink/procfs on Linux,
  libproc on macOS)
- `sysinfo` crate → command line, working directory, CPU, memory, uptime, status
- `.git/HEAD` → current branch (handles linked worktrees)
- `docker ps` → host-port-to-container mapping (skipped if docker isn't running)

No daemon, no config, no telemetry. Verified on macOS (Apple Silicon) and
Ubuntu Linux.

## License

MIT
