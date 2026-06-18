# Changelog

All notable changes to harbor are documented here. Format loosely follows
[Keep a Changelog](https://keepachangelog.com/).

## [Unreleased]

### Added
- Detect Expo / React Native projects from `package.json` dependencies.
- README demo gif.

## [0.2.0] — 2026-06-18

### Added
- **Cross-platform support (macOS + Linux)** — the collection layer now uses the
  `listeners` + `sysinfo` crates instead of shelling out to `lsof`/`ps`/`kill`.
  One code path, single binary, verified on both platforms.
- **Git branch** per project (worktree-aware, read from `.git/HEAD`).
- **Database detection** — PostgreSQL, Redis, MongoDB, MySQL/MariaDB, Memcached,
  nginx, and others, by process name and canonical port.
- **Docker container mapping** — host ports resolved to container name + image
  via `docker ps` (skipped when docker isn't running).
- **Health flags** — `orphaned` (reparented to init) and `zombie` processes.
- More frameworks: Angular, SvelteKit, Solid, Qwik, Metro.
- CLI subcommands: `harbor <port>` deep view (with process tree), `ps`,
  `--json`, `kill` (port/pid/range/multiple, `-f`), `clean` (`-n` preview /
  `-f` force), `watch` (port start/stop events).

### Changed
- TUI now shows git branch, kind-colored markers, and health colors.

## [0.1.0] — 2026-06-17

### Added
- Initial release: interactive ratatui TUI listing local TCP listeners mapped to
  their project (cwd), with framework detection, memory column, open-in-browser,
  and in-place kill. macOS only (lsof/ps).
