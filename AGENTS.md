# qtile-cmd-client (qticc)

`qticc` is a high-performance Rust CLI client for the Qtile window manager. It communicates with Qtile over Unix domain sockets and is significantly faster than the official Python `qtile cmd-obj` tool (sub-100ms vs several seconds).

**Key features:** length-prefixed framing protocol (4-byte BE, Qtile PR #5835), interactive REPL with tab-completion, JSON output for scripting, full command graph navigation.

**Tech stack:** Rust 2021, `anyhow` (errors), `clap` (CLI), `serde`/`serde_json`, `rustyline` (REPL), `simple_logger`.

## Commands

```bash
# Build
cargo build --release      # release binary (qticc)
just build                 # alias

# Test (unit tests only, no running Qtile required)
cargo test --lib --all-features -- --nocapture
just test

# Lint / format
cargo clippy --all-features -- -D warnings
cargo fmt -- --check
just clippy

# Integration tests (require a running Qtile instance)
just docker-test-x11                              # X11 tests in Docker (Python 3.12 default)
PYTHON_VERSION=3.13 just docker-test-wl           # Wayland tests
just docker-shell                                 # drop into test container

# Run a single test by name
cargo test --lib test_find_sockfile -- --nocapture
```

## Architecture

The project is a single Rust binary (`qticc`) with a companion library crate (`qtile_client_lib`). Source lives under `src/utils/`:

| File | Role |
|------|------|
| `args.rs` | CLI parsing via clap: `cmd-obj` subcommand |
| `graph.rs` | Static `OBJECTS` list and `ObjectType` enum mapping Qtile's command graph nodes |
| `parser.rs` | `CommandParser` — translates CLI object/function/args into the `selectors`-based JSON payload Qtile expects; also fetches help via `commands`/`eval`/`doc` IPC calls |
| `ipc.rs` | `Client` — raw Unix socket I/O; sends unframed JSON payload, reads response until EOF; handles both legacy array and modern `{"message_type": "reply"}` envelope responses |
| `client.rs` | `QtileClient` + `CommandQuery` builder — the public API layer between the CLI/REPL and the IPC client |
| `repl.rs` | Interactive REPL using `rustyline`; supports `cd`/`ls`/`..` navigation of the command graph and tab-completion backed by live Qtile queries |

### IPC Protocol

Unframed: raw JSON payload `[selectors, name, args, kwargs, lifted]` sent over Unix socket, connection closed after write, response read until EOF. The response may be a legacy `[status, result]` array or a modern `{"message_type": "reply", "content": {"status": N, "result": ...}}` envelope — both are handled transparently.

### Socket Discovery

Socket path: `$XDG_CACHE_HOME/qtile/qtilesocket.<display>` (defaults to `~/.cache`). Display is resolved in order: explicit arg → `WAYLAND_DISPLAY` → `DISPLAY` → fallback scan of `wayland-0`, `:0`, `:99`.

### Test Layout

- **Unit tests** (each `utils/*.rs` and `main.rs`): pure logic, no running Qtile needed. Run with `cargo test --lib`.
- **Integration tests** (`src/tests/`): require a live Qtile socket, executed by scripts in `.github/scripts/` (e.g., `run-x11-tests`, `run-wl-tests`).
- Tests must not be parallelized across threads when a shared Qtile socket is involved; CI uses `--test-threads 1` for integration suites.

### CI

Self-hosted GitHub Actions runner (`qtile`) on Fedora 44. Matrix: Python 3.12 / 3.13 / 3.14 × Rust stable / nightly. Python 3.14 and nightly are `continue-on-error`.

**Shared PVC** at `/opt/hostedtoolcache` caches `RUSTUP_HOME`, `CARGO_HOME`, and `UV_CACHE_DIR`. Wayland dependencies (wlroots, Xwayland, etc.) are baked into the runner image — no runtime installation needed.

- **UV**: Python and Qtile are managed via `uv`; GitHub remote caching is disabled (`enable-cache: false`) to avoid `tar` conflicts on the shared volume.
- Qtile is installed from mainline `master` (`qtile/qtile`).

## Development Conventions

- **Error handling**: Always use `.context()` or `anyhow!()` for descriptive errors. Avoid `unwrap()` or `expect()` in core logic unless it is a guaranteed invariant.
- **Minimal changes**: Prefer surgical, high-signal edits over broad refactors.
