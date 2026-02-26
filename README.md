# qtile-cmd-client (qticc)

A high-performance Rust implementation of the Qtile command client.

`qticc` is a fast alternative to the standard Python-based `qtile cmd-obj`. It interacts with Qtile's IPC via Unix sockets to traverse the command graph and execute functions.

## Performance

The primary motivation for this project is speed. The standard Python client can be slow for repeated calls.

**Python Client:**
```bash
time (qtile cmd-obj -f windows &>/dev/null)

real    4.73s
user    1.09s
sys     3.64s
cpu     99%
```

**qticc (Rust):**
```bash
time (qticc cmd-obj -f windows &>/dev/null)

real    0.06s
user    0.00s
sys     0.00s
cpu     2%
```

## Development and Testing

A `Makefile` is provided to simplify common tasks.

### Local Development
```bash
make build      # Build the release binary
make test       # Run unit tests
make clippy     # Run linting
```

### Containerized Testing
You can run the full integration test suite (which requires a running Qtile instance) inside a clean Docker container based on Debian Sid.

```bash
# Build and run X11 tests (defaults to Python 3.12)
make docker-test-x11

# Build and run Wayland tests for a specific Python version
make docker-test-wl PYTHON_VERSION=3.13

# Drop into a shell inside the test container
make docker-shell
```

## CI/CD Pipeline

The project uses GitHub Actions (`.github/workflows/rust.yml`) which:
1. Runs on a self-hosted runner with shared PVC support at `/opt/hostedtoolcache`.
2. Automatically caches Rust and Python dependencies.
3. Runs linting and integration tests across multiple Python versions.

## Installation

```bash
cargo install --path .
```

## Usage

```bash
qticc cmd-obj [-o <object_path>] [-f <function>] [-a <args...>] [--info] [--framed]
```

Use `--framed` to support the new length-prefixed IPC protocol (Qtile PR #5835).

## License

GNU GENERAL PUBLIC LICENSE Version 3
