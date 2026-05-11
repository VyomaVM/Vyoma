# CRI Test-Driven Development Workflow

This document describes the critest-driven development workflow for vk8s.

## Overview

The CRI conformance test suite (critest) validates that vk8s correctly implements the Kubernetes Container Runtime Interface. Following TDD principles, we use critest failures to drive implementation improvements.

## Workflow

### 1. Run Initial Tests

```bash
# Setup test environment
./scripts/run-critest.sh setup

# Check dependencies
./scripts/run-critest.sh check

# Run specific test suite
./scripts/run-critest.sh podsandbox
```

### 2. Analyze Failures

Each test failure indicates a missing or incorrect implementation:

```
FAIL: "should support running container with non-root user"
  - Missing: User namespace support in container config
  - Action: Implement user namespace mapping in container.go

FAIL: "should return container status correctly"
  - Missing: Proper container state tracking
  - Action: Update container state machine in server.go
```

### 3. Implement Fix

Based on test failures, implement the fix following this priority:

1. **Critical**: PodSandbox lifecycle (Run, Stop, Remove, Status)
2. **High**: Container lifecycle (Create, Start, Stop, Remove, Status)
3. **Medium**: Image operations (Pull, List, Remove, Status)
4. **Low**: Streaming (Exec, Attach, PortForward)

### 4. Verify Fix

```bash
# Run tests again
./scripts/run-critest.sh podsandbox

# Check specific test
critest --runtime-endpoint=unix:///var/run/vyoma-cri.sock \
        --ginkgo.focus="should support running container"
```

## Test Categories

### PodSandbox Tests (Priority 1)

These tests validate the VM lifecycle mapping:

| Test | Description | Implementation |
|------|-------------|----------------|
| RunPodSandbox | Create and start VM | pod_sandbox.go:RunPodSandbox |
| StopPodSandbox | Stop VM gracefully | pod_sandbox.go:StopPodSandbox |
| RemovePodSandbox | Delete VM | pod_sandbox.go:RemovePodSandbox |
| PodSandboxStatus | Report VM status | pod_sandbox.go:PodSandboxStatus |
| ListPodSandbox | List all pods | pod_sandbox.go:ListPodSandbox |

### Container Tests (Priority 2)

These tests validate container process management:

| Test | Description | Implementation |
|------|-------------|----------------|
| CreateContainer | Configure container in VM | container.go:CreateContainer |
| StartContainer | Execute container process | container.go:StartContainer |
| StopContainer | Stop container gracefully | container.go:StopContainer |
| RemoveContainer | Clean up container resources | container.go:RemoveContainer |
| ContainerStatus | Report container status | container.go:ContainerStatus |
| ListContainers | List containers | container.go:ListContainers |

### Image Tests (Priority 3)

These tests validate image management:

| Test | Description | Implementation |
|------|-------------|----------------|
| PullImage | Pull OCI image | image_service.go:PullImage |
| ListImages | List cached images | image_service.go:ListImages |
| ImageStatus | Get image metadata | image_service.go:ImageStatus |
| RemoveImage | Remove cached image | image_service.go:RemoveImage |

### Streaming Tests (Priority 4)

These tests validate kubectl exec/attach:

| Test | Description | Implementation |
|------|-------------|----------------|
| ExecSync | Execute command synchronously | streaming.go:Exec |
| ExecAttach | Attach to container stdin/stdout | streaming.go:Attach |
| PortForward | Forward ports from pod | streaming.go:PortForward |

## Running Tests in CI

### Local Development

```bash
# Quick check after changes
./scripts/run-critest.sh check

# Full test suite (takes ~15 minutes)
./scripts/run-critest.sh full
```

### Pre-commit Hook

Add to `.git/hooks/pre-commit`:

```bash
#!/bin/bash
cd vk8s
go build ./...
go vet ./...
```

### CI Integration

GitHub Actions automatically runs:
1. `go vet` on every push
2. `go build` verification
3. Proto generation check
4. CRI conformance tests on PR

## Interpreting Results

### Success Criteria

For production readiness, we target:
- **100%** PodSandbox tests passing
- **100%** Container tests passing
- **90%** Image tests passing
- **80%** Streaming tests passing

### Known Limitations

Some tests may fail due to:
- VM-based isolation vs container-based
- No native user namespace support (requires VM restart)
- Limited cgroup v2 compatibility

These are tracked in `docs/cri-quirks.md`.

## Debugging Tips

### View socket activity

```bash
sudo apt install socat
socat - UNIX-CONNECT:/var/run/vyoma-cri.sock
```

### Check server logs

```bash
./vk8s -vyoma-addr localhost:7071 -vyoma-http http://localhost:8080 2>&1 | tee server.log
```

### Single test debugging

```bash
critest --runtime-endpoint=unix:///var/run/vyoma-cri.sock \
        --ginkgo.focus="should create a sandbox" \
        --ginkgo.v \
        --parallel=1
```

## Reporting Issues

When filing CRI conformance issues:

1. Include critest output
2. Show `crictl info` output
3. Provide server logs (`-v` flag)
4. List Kubernetes version
5. Describe expected vs actual behavior