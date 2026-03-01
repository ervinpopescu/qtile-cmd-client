# GEMINI.md - Project Context: qtile-cmd-client (qticc)

This document provides essential context and instructions for AI agents interacting with the `qtile-cmd-client` (or `qticc`) project.

## Project Overview

`qticc` is a high-performance, Rust-based command-line client for the window manager Qtile. It is designed as a significantly faster alternative to the official Python-based `qtile cmd-obj` tool.

### Key Features:
- **Performance**: Written in Rust, it offers sub-100ms response times compared to several seconds for the Python client.
- **IPC Protocol**: Communicates with Qtile via Unix domain sockets (typically found in `~/.cache/qtile/qtilesocket.*`).
- **Framing Support**: Implements the length-prefixed framing protocol (4-byte BE length) introduced in Qtile PR #5835.
- **Interactive REPL**: Includes an interactive shell for navigating the Qtile command graph.
- **JSON Support**: Can output results in JSON format for easy scripting.

### Tech Stack:
- **Language**: Rust (Edition 2021)
- **Error Handling**: `anyhow` for comprehensive error context.
- **Argument Parsing**: `clap` (derive API).
- **Serialization**: `serde`, `serde_json`.
- **Logging**: `simple_logger`.

## Building and Running

A `Makefile` is provided to standardize development tasks.

### Core Commands:
- **Build**: `make build` (builds release binary) or `cargo build`.
- **Test**: `make test` (runs library unit tests) or `cargo test`.
- **Lint**: `make clippy` (runs clippy with `-D warnings`).
- **Format**: `make fmt` (checks formatting).

### Integration Testing (Docker):
The project requires a running Qtile instance for integration tests. A `Dockerfile.test` (based on Debian Sid) is used to create a clean environment.
- **X11 Tests**: `make docker-test-x11`
- **Wayland Tests**: `make docker-test-wl PYTHON_VERSION=3.13`
- **Interactive Shell**: `make docker-shell`

## CI/CD and Infrastructure

The project features a modernized CI/CD pipeline (`.github/workflows/rust.yml`) optimized for **self-hosted Debian Trixie/Sid runners** using a **shared PVC** (Persistent Volume Claim) at `/opt/hostedtoolcache`.

### CI Optimizations:
- **Shared Caching**: `RUSTUP_HOME`, `CARGO_HOME`, and `UV_CACHE_DIR` are all pointed to the PVC to avoid redundant downloads across runs.
- **UV Integration**: Uses `astral-sh/setup-uv@v7` for fast Python and Qtile management. GitHub remote caching is disabled (`enable-cache: false`) to avoid `tar` conflicts on the shared volume.
- **Robust Locking**: `scripts/install-deps` implements a global lock with **exponential backoff** and **stale lock detection** (30 mins) to prevent parallel jobs from corrupting the shared PVC state.
- **Dynamic Patching**: `scripts/patch_wayland_setup.py` dynamically wraps build blocks in the upstream Qtile setup script. It uses `pkg-config` to skip building Wayland dependencies (wlroots, etc.) if they are already present in the PVC.

## Development Conventions

- **Surgical Updates**: When modifying code, prioritize minimal, high-signal changes.
- **Error Handling**: Always use `.context()` or `anyhow!` for descriptive errors. Avoid `unwrap()` or `expect()` in core logic unless it's a guaranteed invariant.
- **Dependency Management**: Source dependencies (Wayland/wlroots) should be installed into the shared PVC prefix (`/opt/hostedtoolcache/qtile-deps/wayland/install`) to ensure they persist across ephemeral runner instances.
- **Testing**: Integration tests are located in `src/tests/` and are executed by scripts in the `scripts/` directory (e.g., `run-x11-tests`).
