# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

```bash
# Build
cargo build --release      # release binary (qticc)
make build                 # alias

# Test (unit tests only, no running Qtile required)
cargo test --lib -- --nocapture
make test

# Lint / format
cargo clippy -- -D warnings
cargo fmt -- --check
make clippy
make fmt

# Integration tests (require a running Qtile instance)
make docker-test-x11                         # X11 tests in Docker (Python 3.12 default)
make docker-test-wl PYTHON_VERSION=3.13      # Wayland tests
make docker-shell                            # drop into test container

# Run a single test by name
cargo test --lib test_find_sockfile -- --nocapture
```

## Architecture

The project is a single Rust binary (`qticc`) with a companion library crate (`qtile_client_lib`). Source lives under `src/utils/`:

| File | Role |
|------|------|
| `args.rs` | CLI parsing via clap: `cmd-obj` subcommand + `--framed` global flag |
| `graph.rs` | Static `OBJECTS` list and `ObjectType` enum mapping Qtile's command graph nodes |
| `parser.rs` | `CommandParser` — translates CLI object/function/args into the `selectors`-based JSON payload Qtile expects; also fetches help via `commands`/`eval`/`doc` IPC calls |
| `ipc.rs` | `Client` — raw Unix socket I/O; implements both **framed** (4-byte BE length prefix) and **unframed** (EOF-terminated) protocols; auto-retries with framing on empty response |
| `client.rs` | `QtileClient` + `CommandQuery` builder — the public API layer between the CLI/REPL and the IPC client |
| `repl.rs` | Interactive REPL using `rustyline`; supports `cd`/`ls`/`..` navigation of the command graph and tab-completion backed by live Qtile queries |

### IPC Protocols

Two protocols exist, selected by `--framed`:

- **Framed** (new, recommended): JSON message wrapped as `{"message_type": "command", "content": <payload>}`, length-prefixed with 4-byte big-endian header. Response unwrapped from `{"message_type": "reply", "content": ...}`. Corresponds to Qtile's `json_ipc` branch (PR #5835).
- **Unframed** (legacy): raw JSON payload sent over socket, connection closed after write, response read until EOF.

The client automatically retries with framing when an unframed request returns an empty response.

### Socket Discovery

Socket path: `$XDG_CACHE_HOME/qtile/qtilesocket.<display>` (defaults to `~/.cache`). Display is resolved in order: explicit arg → `WAYLAND_DISPLAY` → `DISPLAY` → fallback scan of `wayland-0`, `:0`, `:99`.

### Test Layout

- **Unit tests** (each `utils/*.rs` and `main.rs`): pure logic, no running Qtile needed. Run with `cargo test --lib`.
- **Integration tests** (`src/tests/`): require a live Qtile socket, executed by scripts in `.github/scripts/` (e.g., `run-x11-tests`, `run-wl-tests`).
- Tests must not be parallelized across threads when a shared Qtile socket is involved; CI uses `--test-threads 1` for integration suites.

### CI

Self-hosted GitHub Actions runner (`qtile`) on Debian Trixie/Sid. Matrix: Python 3.12 / 3.13 / 3.14 × Rust stable / nightly. Python 3.14 and nightly are `continue-on-error`.

**Shared PVC** at `/opt/hostedtoolcache` caches `RUSTUP_HOME`, `CARGO_HOME`, `UV_CACHE_DIR`, and pre-built Wayland deps (`/opt/hostedtoolcache/qtile-deps/wayland/install`). Key CI details:

- **Locking**: `.github/scripts/install-deps` uses a global lock with exponential backoff and stale-lock detection (30 min threshold) to prevent parallel jobs from corrupting shared PVC state.
- **Dynamic patching**: `.github/scripts/patch_wayland_setup.py` wraps build blocks in the upstream Qtile setup script and uses `pkg-config` to skip rebuilding Wayland dependencies (wlroots, etc.) already present in the PVC.
- **UV**: Python and Qtile are managed via `uv`; GitHub remote caching is disabled (`enable-cache: false`) to avoid `tar` conflicts on the shared volume.
- Qtile is installed from the `json_ipc` branch for framing tests.

## Development Conventions

- **Error handling**: Always use `.context()` or `anyhow!()` for descriptive errors. Avoid `unwrap()` or `expect()` in core logic unless it is a guaranteed invariant.
- **Minimal changes**: Prefer surgical, high-signal edits over broad refactors.
