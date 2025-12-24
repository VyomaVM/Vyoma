# Ignite Feature Walkthrough

## Phase 8: Port Mapping (New! 🚀)
We have successfully implemented **Userspace TCP Proxying**. You can now expose VM ports to your host machine easily.

### Verification Steps
1.  **Run VM with Port Mapping**:
    ```bash
    cargo run --bin ign -- run -p 8080:80 alpine:latest
    ```
2.  **Start Service inside VM**:
    (In the daemon console, which drops valid shell)
    ```bash
    echo "Hello from VM" | nc -l -p 80
    ```
3.  **Access from Host**:
    ```bash
    curl localhost:8080
    ```
    **Result**: You should see "Hello from VM".

---

## Phase 7: Time Travel & Teleportation
(Previously Verified)

### Verification
1.  **Snapshot**: `ign snapshot <id>` -> Creates `snapshot.snap` and git commit.
2.  **Time Travel**: `git log` in `.ignite/vms/<id>` shows history.
3.  **Teleport**: `ign export <id> file.tar.gz` and `ign import file.tar.gz` works across machines.

## Phase 6: Core Runtime
(Resolved Issues)
*   **Networking**: Bridge `ign0` and NAT are auto-configured safely.
*   **Boot**: Alpine boots correctly using `init=/bin/sh`.
