# Vyoma Master Execution Plan & Multi-Branch Delivery Protocol

This document governs the entire implementation workflow for aligning the Vyoma runtime and marketing promises. It defines the exact multi-branch strategy, conventional commit standards, documentation requirements, and real-hardware/UI testing protocols.

---

## 1. Why a New Session Must Be Triggered in the IDE UI

> [!NOTE]
> As an AI assistant, I do not have a programmatic API to close the current IDE window or trigger the Antigravity chat UI's "New Chat" button autonomously.
> However, by placing this master protocol directly into the repository root (`MASTER_EXECUTION_PLAN.md`), any session (current or new) immediately inherits 100% of the full context, technical specifications, and execution rules.

---

## 2. Multi-Branch Architecture & Granular Commit Roadmap

Every phase is isolated in its own dedicated Git branch, with multiple granular, conventional commits per branch (`type(scope): description`).

```
develop (base)
  │
  ├──► feat/cli-ergonomics-and-compose (Phase 1)
  │      ├── commit 1: feat(cli): add human-readable memory size parser (--memory 512M, 1G)
  │      ├── commit 2: feat(snapshot): support named snapshot tags in CLI and daemon
  │      ├── commit 3: feat(restore): unify rollback onto /restore endpoint and add vyoma restore
  │      ├── commit 4: feat(compose): implement smart compose file discovery (docker-compose fallback)
  │      ├── commit 5: feat(cli): implement vyoma inspect subcommand with JSON output
  │      └── commit 6: test(e2e): add real hardware validation script for CLI ergonomics
  │
  ├──► feat/timemachine-and-hibernation (Phase 2)
  │      ├── commit 1: feat(storage): expose QCOW2 snapshot branching in vm_service and daemon
  │      ├── commit 2: feat(cli): add vyoma branch subcommand for runtime cloning
  │      ├── commit 3: feat(vmm): implement scale-to-zero hibernation lifecycle in vyomad
  │      ├── commit 4: feat(cli): add vyoma hibernate and vyoma resume (strictly resume, no start)
  │      └── commit 5: test(e2e): real physical VM hibernation and resume hardware test
  │
  ├──► feat/teleport-mesh-resolution (Phase 3)
  │      ├── commit 1: feat(teleport): support --to flag and flexible target arguments in CLI
  │      ├── commit 2: feat(swarm): auto-resolve cluster node names to IP endpoints via Raft registry
  │      └── commit 3: test(swarm): multi-node teleportation target resolution test
  │
  ├──► feat/python-sdk-and-vk8s (Phase 4)
  │      ├── commit 1: feat(sdk): create official vyoma python sdk package with REST client
  │      ├── commit 2: test(sdk): add unit and integration tests for python sdk
  │      ├── commit 3: feat(vk8s): add make vk8s build target and package into deb/rpm
  │      └── commit 4: docs(vk8s): add CRI setup, crictl workflow, and Kubernetes RuntimeClass docs
  │
  ├──► refactor/storage-cleanup-and-hardware-test (Phase 5)
  │      ├── commit 1: refactor(storage): prune obsolete devicemapper dead code and legacy stubs
  │      ├── commit 2: test(hardware): run full physical /dev/kvm end-to-end regression suite
  │      └── commit 3: test(ui): validate real daemon events, metrics, and VM controls in Web Dashboard
  │
  └──► docs/architecture-and-website-sync (Phase 6)
         ├── commit 1: docs(security): replace 7-layer stack with 4 Enterprise Security Pillars
         ├── commit 2: docs(cli): update COMMANDS.md and README.md with all new/unified subcommands
         └── commit 3: docs(website): generate patch/PR for marketing website (v2.9.0, copy fixes)
```

---

## 3. Granular Commit Standards (Conventional Commits)

All commits must adhere strictly to the Conventional Commits specification:
- `feat(scope): ...` for new features or user-facing enhancements
- `fix(scope): ...` for bug fixes or syntax corrections
- `refactor(scope): ...` for restructuring without behavioral changes (e.g. DM cleanup)
- `test(scope): ...` for real-hardware test suites and assertions
- `docs(scope): ...` for markdown, command manuals, and website patches

### Mandatory Commit Checklist:
1. Each logical task receives its own individual commit (no mega-commits).
2. The commit message includes a concise description of *why* the change was made.
3. Code compiles with zero warnings or errors prior to committing.

---

## 4. Documentation Protocol (How Docs Must Be Created)

Documentation cannot be an afterthought; it must be updated alongside each code change:

1. **`COMMANDS.md`:**
   - Update every CLI syntax change immediately (`--memory`, `restore`, `inspect`, `branch`, `hibernate`, `resume`, `teleport --to`).
   - Include realistic terminal examples showing actual flags and expected stdout.
2. **`README.md`:**
   - Update the quickstart guide, feature list, and system requirements.
   - Synchronize version badge to `v2.9.0`.
3. **`docs/specifications/`:**
   - Document the **4 Enterprise Security Pillars** in architecture specs.
   - Document the QCOW2 branching mechanism and scale-to-zero hibernation state machine.
4. **Website Alignment Patch (`docs/website_alignment_patch.md`):**
   - Provide the exact file diffs for the Astro/HTML website repository:
     - Memory parsing fix in snippets (`--memory 512`)
     - `vyoma restore` and `vyoma resume` commands
     - Standardized `<250ms` boot and `~30 MB` memory overhead
     - 4 Security Pillars
     - Upgraded Studio badge ("Active / Built-in")
     - Release version `v2.9.0`

---

## 5. Testing Protocol: Real Hardware & UI Validation (Zero Mocking)

> [!IMPORTANT]
> **NO MOCKING.** All verification must use the physical machine's `/dev/kvm` hardware, real Cloud Hypervisor v54.0 binaries, the Linux 6.16.9+ microVM kernel (`bin/vmlinux`), and the real Web UI dashboard.

### Protocol A: Physical `/dev/kvm` Hardware Test Suite
1. **Real VM Boot & Memory Parsing:**
   - Execute: `vyoma run alpine:latest --vcpu 1 --memory 256M`
   - Assert: MicroVM boots in `<250ms`, acquires IP via TAP bridge `vyoma0`, and is responsive to ping/exec.
2. **Real Snapshot & Restore Test:**
   - Take snapshot: `vyoma snapshot <vm-id> --name test-snap`
   - Mutate VM state (write file inside guest).
   - Restore snapshot: `vyoma restore <vm-id> --from test-snap`
   - Assert: Guest memory and filesystem revert to the exact pre-mutation state.
3. **Real QCOW2 Branching Test:**
   - Branch snapshot: `vyoma branch <vm-id> --from test-snap branched-vm`
   - Assert: New QCOW2 overlay linked to snapshot backing chain boots as independent VM.
4. **Real Scale-to-Zero Hibernation Test:**
   - Hibernate: `vyoma hibernate <vm-id>`
   - Assert: Cloud Hypervisor process terminates; host RAM drops to 0 MB overhead.
   - Resume: `vyoma resume <vm-id>`
   - Assert: Cloud Hypervisor restores memory file and resumes guest in `<200ms`.

### Protocol B: Live Web UI Dashboard Verification
1. Serve the live dashboard (port 5173 / 8080).
2. Connect to real `vyomad` daemon.
3. Open browser subagent to:
   - Verify active VMs appear on `/vms` with accurate CPU/Memory metrics.
   - Verify SSE live events stream on `/events` during VM lifecycle actions.
   - Verify `/topology` visualizes bridge networks and running MicroVM nodes.
   - Verify `/compose` and `/timemachine` display snapshot trees.
4. Capture screenshots of each verified UI screen into the artifacts directory.

---

## 6. Handoff Instruction for New Session

If you choose to start a new session now to clear memory and RAM cache, simply start the new chat and send:

```text
Please read the complete execution protocol in MASTER_EXECUTION_PLAN.md.
Create branch 1: `feat/cli-ergonomics-and-compose` and begin executing Phase 1 with granular conventional commits, real hardware testing, and documentation updates.
```
