# QA Evidence - Phase 6.5: In-VM Agent

## Feature Description
In-VM binary that runs inside each MicroVM to communicate with the host daemon via vsock (or TCP fallback). Provides process list, metrics, file read, and command execution.

## Implementation
- Created `crates/vyoma-agent-vm/` crate
- Implements AgentRequest/AgentResponse types
- TCP server (vsock requires actual VM environment)
- CLI with clap for mode/port configuration

## Test Results

### Unit Tests (10 tests)
```
$ cargo test -p vyoma-agent-vm
    Running unittests src/lib.rs

running 8 tests
test tests::test_agent_request_serialization ... ok
test tests::test_agent_response_serialization ... ok
test tests::test_exec_command_request ... ok
test tests::test_file_read_request ... ok
test tests::test_response_error_serialization ... ok
test tests::test_metrics_collection ... ok
test tests::test_process_list ... ok
test tests::test_process_info_fields ... ok

test result: ok. 8 passed; 0 failed

    Running unittests src/main.rs

running 2 tests
test tests::test_cli_defaults ... ok
test tests::test_cli_custom_port ... ok

test result: ok. 2 passed; 0 failed
```

### Components
| Feature | Status |
|---------|--------|
| Process list collection | ✅ |
| VM metrics (CPU, memory) | ✅ |
| File read | ✅ |
| Command execution | ✅ |
| TCP server | ✅ |
| CLI arguments | ✅ |

### API
Request/Response format:
```json
{"type":"ProcessList"}
{"type":"GetMetrics"}
{"type":"FileRead","path":"/etc/hostname"}
{"type":"ExecCommand","cmd":["ls","-la"],"env":{},"workdir":null}
```
