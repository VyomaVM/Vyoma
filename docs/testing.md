# Testing Guide 🧪

This document outlines how to test the Vyoma ecosystem to ensure robustness and stability.

## 1. Unit Tests (Core Logic)
We use standard Rust testing for individual components.

```bash
# Run all unit tests
cargo test

# Run tests for specific crate
cargo test -p vyoma-core
```

## 2. Integration Tests (End-to-End)
Integration tests require a running `vyomad` daemon.

### Automated Test Script
We provide a script to run a full lifecycle test:
```bash
./scripts/test_integration.sh
```
This script will:
1.  Build release binaries.
2.  Start `vyomad` in the background (using a temporary home dir).
3.  Run `vyoma` commands (pull, run, stop, network).
4.  Verify outcomes.
5.  Cleanup.

### Manual Testing
1.  Start Daemon:
    ```bash
    sudo ./target/release/vyomad
    ```
2.  Run Commands:
    ```bash
    ./target/release/vyoma doctor
    ./target/release/vyoma run alpine:latest
    ```

## 3. Environment Variables
Configure behavior using `.env` or shell variables:

| Variable | Description | Default |
|----------|-------------|---------|
| `RUST_LOG` | Logging level (`info`, `debug`, `trace`) | `info` |
| `VYOMA_HOME` | Custom root directory (overrides `~/.vyoma`) | `~/.vyoma` |
| `VYOMA_SOCK` | Path to daemon socket (if applicable) | `/tmp/vyoma.sock` |

## 4. Contributing
*   Ensure `cargo fmt` and `cargo clippy` pass before pushing.
*   Add tests for any new features.