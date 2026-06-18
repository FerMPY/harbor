# Contributing to harbor

Thanks for your interest! harbor is a small, focused tool — contributions that
keep it fast, dependency-light, and cross-platform (macOS + Linux) are very
welcome.

## Development

```sh
cargo build           # debug build
cargo run             # launch the TUI
cargo run -- --list   # one-shot CLI output
cargo test            # unit tests
```

Before opening a PR, please make sure these pass (CI runs them on macOS + Linux):

```sh
cargo fmt --check
cargo clippy -- -D warnings
cargo test
```

## Architecture

- `src/collect.rs` — the data engine. Enumerates TCP listeners (`listeners`
  crate) and enriches each with process info, cwd, git branch, framework /
  database / docker label, and health (`sysinfo` crate). No `lsof`/`ps`.
- `src/app.rs` — TUI application state and actions.
- `src/ui.rs` — ratatui rendering.
- `src/main.rs` — CLI dispatch (TUI / `<port>` / `ps` / `--json` / `kill` /
  `clean` / `watch`) and the event loop.

Pure helpers in `collect.rs` (formatting, framework/database detection, docker
port parsing, git HEAD parsing) are unit-tested — please add tests for new
detection logic.

## Scope

harbor intentionally does **not** target native Windows (reading another
process's working directory there needs debug/admin APIs); WSL2 users get the
Linux build. The `logs` feature is also intentionally out of scope.
