# Architectural Decision Records (ADR)

This document tracks significant architectural decisions, their context, consequences, and alternatives considered.

## 001. Project Naming & Branding
*   **Date**: 2025-12-23
*   **Decision**: Rename project from "Generic Micro-VM (MVM)" to **Vyoma**.
*   **Visuals**: Daemon = `vyomad`, CLI = `vyoma`.
*   **Context**: User requested a "premium" and energetic brand. "Vyoma" aligns with Firecracker (the underlying VMM).
*   **Alternatives**: "Capsule" (discarded for being too generic/safe).

## 002. CLI Wrapper Strategy for System Operations
*   **Date**: 2025-12-23
*   **Decision**: Use `std::process::Command` to wrap standard Linux utilities (`mkfs.ext4`, `losetup`, `mount`, `dmsetup`) instead of linking against native C libraries (`libdevmapper`, `libmount`).
*   **Reasoning**:
    1.  **Portability & Simplicity**: Reduces build-time dependencies (`build-essential` is enough). No need to handle complex C-to-Rust bindings/FFI issues during early prototyping.
    2.  **Debuggability**: It is much easier to print the exact CLI command and reproduce issues manually in the terminal than to debug an ioctl failure code.
    3.  **Stability**: Linux CLI tools have extremely stable interfaces.
*   **Consequences**:
    *   Performance: Slight overhead from spawning processes (negligible for infrequent ops like VM lifecycles).
    *   Error Handling: Must parse stderr/stdout strings instead of typed errors.
*   **Future Considerations**:
    *   For high-scale production usage (thousands of VMs/sec), we should migrate critical paths (like `dmsetup`) to native Rust crates (`devicemapper-rs`) to avoid `fork/exec` overhead.

## 003. OCI Image Handling
*   **Date**: 2025-12-23
*   **Decision**: Implement a custom simple OCI client in `vyoma-core` using `reqwest` instead of using the `oci-distribution` crate.
*   **Reasoning**:
    *   The `oci-distribution` crate (v0.9.4) is heavy and had compatibility issues with recent tokio/http versions in our testing.
    *   We need specific control over handling OCI Indexes vs Docker V2 Manifests to force `linux/amd64` resolution.
*   **Consequences**:
    *   We own the OCI parsing logic (maintenance burden).
    *   We can easily customize authentication logic (Docker Hub vs private registries).

## 004. Database / State Management
*   **Status**: Pending
*   **Context**: We need to track running VMs, allocated IPs, and active loop devices.
*   **Current Direction**: Likely file-based JSON/ToML in `.vyoma/.vyoma/state` for MVP.

## 005. Networking Strategy
*   **Date**: 2025-12-23
*   **Decision**: Use Linux Bridge (`brctl`/`ip link`) + TAP interfaces + `iptables` NAT.
*   **Reasoning**:
    *   Standard Docker-like networking model.
    *   Allows VMs to communicate with each other (via bridge) and internet (via NAT).
*   **Alternatives**:
    *   **MacVTap**: Higher performance, but harder to perform host-to-VM communication (hairpin mode issues).
    *   **User-mode Networking (slirp)**: Safer (no root needed), but much slower and harder to expose ports.
*   **Consequences**:
    *   Requires `NET_ADMIN` capability or Root.
    *   Daemon must run with high privileges.

## 006. Storage Stategy: Device Mapper
*   **Date**: 2025-12-23
*   **Decision**: Use `dm-snapshot` for instant cloning.
*   **Reasoning**:
    *   Allows starting 100 VMs from 1 base image with minimal space overhead.
    *   Standard Linux kernel feature (stable).

## 007. Verification Strategy & Known Gaps
*   **Date**: 2025-12-23
*   **Context**: Development is happening in a **Pure Linux (Ubuntu)** environment. Root/Sudo and KVM permissions should be available.
*   **Decision**:
    1.  Implement logic robustly using best-practice wrappers.
    2.  Write Integration Tests for all privileged operations but mark them as `#[ignore]`.
    3.  Track skipped verifications explicitly.
*   **Known Gaps (requiring manual verification)**:
    *   **Storage Population**: `mount` and `cp` require `sudo`. Logic confirmed via `test_storage_population` but requires manual run.
    *   **Loopback/DM**: `losetup` and `dmsetup` require `sudo`.
    *   **Networking**: `ip link` and `iptables` require `NET_ADMIN`/`sudo`.
    *   **Firecracker Boot**: Requires user to be in `kvm` group or have RW access to `/dev/kvm`.
*   **Risk Mitigation**:
    *   The `vyoma-core` library is designed to be modular. If one component fails (e.g., networking), the others (storage) remain testable.
    *   Future CI pipeline MUST run on a bare-metal or nested-virt enabled runner with passwordless sudo to fully validate the `#[ignore]` tests.

## 008. Daemon State Management
*   **Date**: 2025-12-24
*   **Decision**: Store active VM instances in `vyomad` memory using `Arc<std::sync::Mutex<HashMap<String, Arc<tokio::sync::Mutex<VmmManager>>>>>`.
*   **Reasoning**:
    *   Daemon is the source of truth for running processes.
    *   `VmmManager` owns the `std::process::Child` handle.
    *   `tokio::sync::Mutex` allows locking the VMM handle across async API calls (like pause/resume) which wait for Firecracker's HTTP response.
*   **Consequences**:
    *   Daemon restart loses control of running VMs (orphaned processes). (Future task: State persistence/recovery).

## 009. Port Mapping Strategy (Phase 8)
*   **Date**: 2025-12-24
*   **Decision**: Use userspace TCP proxying (Tokio tasks) instead of `iptables` DNAT.
*   **Reasoning**:
    1.  **Flexibility**: Allows mapping `localhost:8080` to VM `80` without managing complex NAT tables or avoiding port conflicts on the bridge.
    2.  **Safety**: Isolates the port opening logic to the `vyomad` process. If the daemon dies, the ports close automatically (unlike iptables rules which persist).
    3.  **Future-Proofing**: Aligns with "Rootless" goals (Phase 11). Userspace proxies don't strictly *need* root (if binding non-privileged ports), whereas `iptables` always does.
*   **Implementation**:
    *   Spawn a `tokio::task` for each mapped port.
    *   Bind `0.0.0.0:HOST_PORT`.
    *   Accept connections and pump bytes to `VM_IP:VM_PORT`.

## 010. Log Streaming Strategy
*   **Date**: 2025-12-24
*   **Decision**: Use `tokio::sync::broadcast` + Server-Sent Events (SSE) for log streaming.
*   **Reasoning**:
    *   `broadcast` generic channel allows multiple consumers (though we currently use one main one, it allows future expansion like "vyoma logs" + "dashboard" simultaneously).
    *   Firecracker logs (stdout/stderr) are captured via pipes and immediately pushed to the broadcast channel.
    *   SSE (`text/event-stream`) is a standard HTTP protocol for streaming updates, supported natively by browsers and easy to consume in CLI via `reqwest`.
    *   Avoids complex WebSocket setup just for read-only logs.
*   **Consequences**:
    *   Clients must handle SSE parsing (implemented in CLI).
    *   Logs are transient in memory (buffer size 100). If no one is listening, logs are dropped. (Acceptable for "streaming" logs, but means we don't have "history" unless we implement persistent logging).

## 011. Volume Mount Strategy (VirtioFS)
*   **Date**: 2025-12-24
*   **Decision**: Use **VirtioFS** with the Rust-based `virtiofsd` binary for sharing host directories.
*   **Reasoning**:
    *   Standard way to share files with Firecracker.
    *   Performance is near-native for cached reads.
    *   Allows "Hot Reload" workflows (editing code on host, running in VM).
*   **Implementation**:
    *   **Dependency**: Requires `virtiofsd` binary in `bin/` or system PATH.
    *   **Daemon**: Spawns a dedicated `virtiofsd` process for *each* shared volume (or one per VM handling multiple paths if supported, but usually one socket per fs).
    *   **Socket**: `virtiofsd` listens on a Unix socket, Firecracker connects to it.
    *   **Kernel**: Depends on guest kernel having `virtiofs`.

## 012. Builder Strategy (Vyomafile)
*   **Date**: 2025-12-24
*   **Decision**: Implement `vyoma build` via a Client-Server model where the Daemon performs the build using `chroot` for `RUN` instructions.
*   **Reasoning**:
    *   **Context**: The daemon manages the image store (`.vyoma/.vyoma/images`), which is often root-owned or privileged.
    *   **Performance**: `RUN` commands are executed via `chroot` on a mounted loopback device of the image. This avoids the overhead of booting a full Firecracker VM for every build step, similar to how Docker builds work (mostly).
    *   **Simplicity**: We mimic Docker's context sending (streaming tarball to daemon).
*   **Directives MVP**:
    *   `FROM <image>`: Starts from a base image.
    *   `RUN <cmd>`: Executes command in chroot.
    *   `COPY <src> <dest>`: Copies files from build context to image.

## 013. Resource Limits Strategy (Cgroups v2)
*   **Date**: 2025-12-24
*   **Decision**: Use Cgroups v2 explicitly to manage VM resource limits (CPU, Memory).
*   **Reasoning**:
    *   **Modern Standard**: Cgroups v2 is the standard on modern Linux (Ubuntu 22.04+).
    *   **Firecracker Integration**: Firecracker supports running inside a Cgroup. We will create a parent cgroup `vyoma.slice` and sub-cgroups for each VM `vyoma-<id>.scope`.
    *   **Implementation**: We will use direct file system manipulation of `/sys/fs/cgroup/vyoma.slice/` for simplicity and control, rather than `systemd-run` for now, unless `systemd` integration is strictly required. Direct FS manipulation is more educational and portable for a "from scratch" project build.

## 014. Rootless Strategy (Future)
*   **Date**: 2025-12-25
*   **Status**: Proposed / In Progress
*   **Context**: Running  as root is a security risk.
*   **Decision**: We will transition to "Rootless" capability using **Slirp4netns** or **Passt** for unprivileged networking.
*   **Challenges**:
    1.  **Networking**: Creating TAP/Bridge requires root. Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit runs in userspace.
    2.  **Storage**:  requires root. We must move to user-space mounting (FUSE) or rely on direct file usage (Firecracker supports raw files without mounting).
    3.  **KVM**: Requires user to be in  group.
*   **Phasing**:
    *   Phase 1 is establishing the Rootless Architecture.
    *   Currently, we will focus on investigating these requirements in a separate branch .

## 014. Rootless Strategy (Future)
*   **Date**: 2025-12-25
*   **Status**: Proposed / In Progress
*   **Context**: Running `vyomad` as root is a security risk.
*   **Decision**: We will transition to "Rootless" capability using **Slirp4netns** or **Passt** for unprivileged networking.
*   **Challenges**:
    1.  **Networking**: Creating TAP/Bridge requires root. `slirp4netns` runs in userspace.
    2.  **Storage**: `mount -o loop` requires root. We must move to user-space mounting (FUSE) or rely on direct file usage (Firecracker supports raw files without mounting).
    3.  **KVM**: Requires user to be in `kvm` group.
*   **Phasing**:
    *   Phase 1 is establishing the Rootless Architecture.
    *   Currently, we will focus on investigating these requirements in a separate branch `feat/rootless`.


## 015. Monitor VM Health and OOM Events

Date: 2025-12-27

### Status

Accepted

### Context

To provide a production-grade experience,  needs to detect not just when a VM fully stops, but *why* it stopped or if it is under stress. A common failure mode for micro-VMs is running out of memory, leading to the host kernel killing the Firecracker process (OOM Kill) via Cgroups.

We currently have a "Zombie Reaper" that polls the process status. We need to extend this to monitor Cgroup events.

### Decision

1. **Unified Monitor Loop**: We will assume the existing global "Process Monitor" task is the central place for health checks. It runs periodically (e.g., every 5 seconds).
2. **Polling over Notifications**: For Cgroup OOM events (), we will read the file and parse the  counter. While  or  file descriptors offer push-based notifications, implementing them asynchronously in Rust adds significant complexity (handling file descriptors effectively in Tokio). Given the non-critical real-time nature of restart logic (seconds are fine), polling is sufficient and much simpler to implement and debug.
3. **Stateless Logic**: The monitor will read the current value. If it's non-zero (and we haven't seen it before? or just if the process is gone?), we infer OOM.
    - actually,  is a counter. We should track the previous value?
    - Simplification: If the process is DEAD and , we report "Killed by OOM". If monitoring a running VM, we can just log warnings if  increments.

### Consequences

- **Pros**: Simple verification, minimal dependencies, reuses existing monitor loop.
- **Cons**: Detection is not instant (up to loop interval delay).
- **Future**: Can upgrade to  polling with  for instant reaction in Phase 16+.

## 016. Internal DNS for Service Discovery

Date: 2025-12-27

### Status

Accepted

### Context

Users need VMs to communicate with each other by hostname (e.g.,  resolving to ) and access the internet (resolving public domains). Since  assigns static IPs and manages the network namespace, we control the network environment.

Existing CNI plugins like  exist but add external dependencies (dnsmasq).

### Decision

1.  **Embedded DNS Server**:  will run a lightweight UDP DNS server (using  or  crate) on the bridge gateway IP (e.g., ).
2.  **Zone Management**: This server will be authoritative for the  TLD (e.g., ).
3.  **Forwarding**: Requests for other domains will be forwarded to the host's system resolver (e.g.,  or ).
4.  **Guest Configuration**: The guest VM will be configured to use the gateway IP as its nameserver via Kernel Boot Args ( parameter support for DNS).

### Consequences

- **Pros**: Zero external dependencies, automatic registration of VMs, instant updates.
- **Cons**: Adds complexity to  (UDP handling). requires  if binding port 53 (but inside private netns or on high port? No, must be 53 for guest /etc/resolv.conf usually implies 53).
    - Actually, we bind to  which is an interface we created.

## 017. Rootless Architecture Strategy

Date: 2025-12-27

### Status

Accepted

### Context

To date,  relies on  for:
1.  **Networking**: Creating TAP devices, CNI bridges, and 1: lo: <LOOPBACK,UP,LOWER_UP> mtu 65536 qdisc noqueue state UNKNOWN mode DEFAULT group default qlen 1000
    link/loopback 00:00:00:00:00:00 brd 00:00:00:00:00:00
2: eno1: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc fq_codel state DOWN mode DEFAULT group default qlen 1000
    link/ether ec:8e:b5:58:b8:6f brd ff:ff:ff:ff:ff:ff
    altname enp3s0
3: wlo1: <BROADCAST,MULTICAST,UP,LOWER_UP> mtu 1500 qdisc noqueue state UP mode DORMANT group default qlen 1000
    link/ether 30:e3:7a:0d:96:f7 brd ff:ff:ff:ff:ff:ff
    altname wlp5s0
4: br-0ea47b1ee736: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN mode DEFAULT group default 
    link/ether 32:25:64:eb:73:d3 brd ff:ff:ff:ff:ff:ff
5: br-17950950bbd0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN mode DEFAULT group default 
    link/ether 4a:fd:8c:20:4b:b7 brd ff:ff:ff:ff:ff:ff
6: br-8e2b42dd9bd7: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN mode DEFAULT group default 
    link/ether ae:57:5c:9a:19:d7 brd ff:ff:ff:ff:ff:ff
7: docker0: <NO-CARRIER,BROADCAST,MULTICAST,UP> mtu 1500 qdisc noqueue state DOWN mode DEFAULT group default 
    link/ether be:2c:49:00:5f:02 brd ff:ff:ff:ff:ff:ff commands.
2.  **Storage**: NAME        SIZELIMIT OFFSET AUTOCLEAR RO BACK-FILE                                                  DIO LOG-SEC
/dev/loop1          0      0         1  1 /var/lib/snapd/snaps/bare_5.snap                             0     512
/dev/loop29         0      0         1  1 /var/lib/snapd/snaps/snap-store_1270.snap                    0     512
/dev/loop19         0      0         1  1 /var/lib/snapd/snaps/gnome-42-2204_226.snap                  0     512
/dev/loop37         0      0         1  1 /var/lib/snapd/snaps/wine-platform-runtime-core22_104.snap   0     512
/dev/loop27         0      0         1  1 /var/lib/snapd/snaps/onlyoffice-desktopeditors_746.snap      0     512
/dev/loop17         0      0         1  1 /var/lib/snapd/snaps/gnome-3-38-2004_143.snap                0     512
/dev/loop8          0      0         1  1 /var/lib/snapd/snaps/cups_1130.snap                          0     512
/dev/loop35         0      0         1  1 /var/lib/snapd/snaps/vlc_3777.snap                           0     512
/dev/loop25         0      0         1  1 /var/lib/snapd/snaps/mesa-2404_1110.snap                     0     512
/dev/loop15         0      0         1  1 /var/lib/snapd/snaps/firmware-updater_210.snap               0     512
/dev/loop6          0      0         1  1 /var/lib/snapd/snaps/core22_2193.snap                        0     512
/dev/loop33         0      0         1  1 /var/lib/snapd/snaps/snapd-desktop-integration_315.snap      0     512
/dev/loop23         0      0         1  1 /var/lib/snapd/snaps/kf6-core24_34.snap                      0     512
/dev/loop13         0      0         1  1 /var/lib/snapd/snaps/firefox_7477.snap                       0     512
/dev/loop4          0      0         1  1 /var/lib/snapd/snaps/core22_2163.snap                        0     512
/dev/loop31         0      0         1  1 /var/lib/snapd/snaps/snapd_25577.snap                        0     512
/dev/loop21         0      0         1  1 /var/lib/snapd/snaps/gnome-46-2404_145.snap                  0     512
/dev/loop11         0      0         1  1 /var/lib/snapd/snaps/dbeaver-ce_415.snap                     0     512
/dev/loop2          0      0         1  1 /var/lib/snapd/snaps/core18_2959.snap                        0     512
/dev/loop38         0      0         1  1 /var/lib/snapd/snaps/wine-platform-runtime-core22_105.snap   0     512
/dev/loop0          0      0         1  1 /var/lib/snapd/snaps/core18_2976.snap                        0     512
/dev/loop28         0      0         1  1 /var/lib/snapd/snaps/onlyoffice-desktopeditors_821.snap      0     512
/dev/loop18         0      0         1  1 /var/lib/snapd/snaps/gnome-42-2204_202.snap                  0     512
/dev/loop9          0      0         1  1 /var/lib/snapd/snaps/core24_1243.snap                        0     512
/dev/loop36         0      0         1  1 /var/lib/snapd/snaps/wine-platform-9-devel-core22_33.snap    0     512
/dev/loop26         0      0         1  1 /var/lib/snapd/snaps/mesa-2404_1165.snap                     0     512
/dev/loop16         0      0         1  1 /var/lib/snapd/snaps/firmware-updater_216.snap               0     512
/dev/loop7          0      0         1  1 /var/lib/snapd/snaps/core24_1237.snap                        0     512
/dev/loop34         0      0         1  1 /var/lib/snapd/snaps/thincast-client_605.snap                0     512
/dev/loop24         0      0         1  1 /var/lib/snapd/snaps/kmahjongg_120.snap                      0     512
/dev/loop14         0      0         1  1 /var/lib/snapd/snaps/firefox_7559.snap                       0     512
/dev/loop5          0      0         1  1 /var/lib/snapd/snaps/core20_2686.snap                        0     512
/dev/loop32         0      0         1  1 /var/lib/snapd/snaps/thincast-client_575.snap                0     512
/dev/loop22         0      0         1  1 /var/lib/snapd/snaps/gtk-common-themes_1535.snap             0     512
/dev/loop12         0      0         1  1 /var/lib/snapd/snaps/dbeaver-ce_418.snap                     0     512
/dev/loop3          0      0         1  1 /var/lib/snapd/snaps/core20_2682.snap                        0     512
/dev/loop30         0      0         1  1 /var/lib/snapd/snaps/snapd_25202.snap                        0     512
/dev/loop20         0      0         1  1 /var/lib/snapd/snaps/gnome-46-2404_125.snap                  0     512
/dev/loop10         0      0         1  1 /var/lib/snapd/snaps/cups_1134.snap                          0     512 (Loop devices) and  (Device Mapper) for Copy-on-Write (COW) layering.
3.  **Building**: Mounting loop devices to populate EXT4 filesystems.

Users want to run VMs without elevated privileges ("Rootless Mode") for security and convenience.

### Decision

We will implement a Hybrid Rootless Model:

1.  **Networking**:
    - If running as non-root, bypass CNI.
    - Use Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit (standard tool used by Podman) to provide user-mode networking.
    - Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit creates a TAP device in a new User Namespace and proxies traffic to the Internet via the Host's network stack (NAT-like).

2.  **Storage (Runtime)**:
    - Device Mapper () requires root.
    - For rootless runtime, we will fallback to **Full File Copy** (or Reflink if filesystem supports it).
    - Instead of creating a  device over a  device, we simply copy  to  and run Firecracker directly on .
    - *Tradeoff*: Slower startup for large images on non-CoW filesystems (Ext4), but simpler and completely unprivileged.

3.  **Storage (Build)**:
    - Building images (writing files to Ext4) deeply requires sysfs on /sys type sysfs (rw,nosuid,nodev,noexec,relatime)
proc on /proc type proc (rw,nosuid,nodev,noexec,relatime)
udev on /dev type devtmpfs (rw,nosuid,relatime,size=6024292k,nr_inodes=1506073,mode=755,inode64)
devpts on /dev/pts type devpts (rw,nosuid,noexec,relatime,gid=5,mode=620,ptmxmode=000)
tmpfs on /run type tmpfs (rw,nosuid,nodev,noexec,relatime,size=1213172k,mode=755,inode64)
/dev/sda2 on / type ext4 (rw,relatime)
securityfs on /sys/kernel/security type securityfs (rw,nosuid,nodev,noexec,relatime)
tmpfs on /dev/shm type tmpfs (rw,nosuid,nodev,inode64)
tmpfs on /run/lock type tmpfs (rw,nosuid,nodev,noexec,relatime,size=5120k,inode64)
cgroup2 on /sys/fs/cgroup type cgroup2 (rw,nosuid,nodev,noexec,relatime,nsdelegate,memory_recursiveprot)
pstore on /sys/fs/pstore type pstore (rw,nosuid,nodev,noexec,relatime)
efivarfs on /sys/firmware/efi/efivars type efivarfs (rw,nosuid,nodev,noexec,relatime)
bpf on /sys/fs/bpf type bpf (rw,nosuid,nodev,noexec,relatime,mode=700)
systemd-1 on /proc/sys/fs/binfmt_misc type autofs (rw,relatime,fd=32,pgrp=1,timeout=0,minproto=5,maxproto=5,direct,pipe_ino=1329)
debugfs on /sys/kernel/debug type debugfs (rw,nosuid,nodev,noexec,relatime)
mqueue on /dev/mqueue type mqueue (rw,nosuid,nodev,noexec,relatime)
tracefs on /sys/kernel/tracing type tracefs (rw,nosuid,nodev,noexec,relatime)
hugetlbfs on /dev/hugepages type hugetlbfs (rw,nosuid,nodev,relatime,pagesize=2M)
fusectl on /sys/fs/fuse/connections type fusectl (rw,nosuid,nodev,noexec,relatime)
configfs on /sys/kernel/config type configfs (rw,nosuid,nodev,noexec,relatime)
/var/lib/snapd/snaps/core18_2976.snap on /snap/core18/2976 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/bare_5.snap on /snap/bare/5 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core18_2959.snap on /snap/core18/2959 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core20_2682.snap on /snap/core20/2682 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core22_2163.snap on /snap/core22/2163 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core20_2686.snap on /snap/core20/2686 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core22_2193.snap on /snap/core22/2193 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core24_1237.snap on /snap/core24/1237 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/cups_1130.snap on /snap/cups/1130 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/core24_1243.snap on /snap/core24/1243 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/cups_1134.snap on /snap/cups/1134 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/dbeaver-ce_415.snap on /snap/dbeaver-ce/415 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/dbeaver-ce_418.snap on /snap/dbeaver-ce/418 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/firefox_7477.snap on /snap/firefox/7477 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/firmware-updater_210.snap on /snap/firmware-updater/210 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/firmware-updater_216.snap on /snap/firmware-updater/216 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/firefox_7559.snap on /snap/firefox/7559 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gnome-3-38-2004_143.snap on /snap/gnome-3-38-2004/143 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gnome-42-2204_202.snap on /snap/gnome-42-2204/202 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gnome-42-2204_226.snap on /snap/gnome-42-2204/226 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gnome-46-2404_125.snap on /snap/gnome-46-2404/125 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gnome-46-2404_145.snap on /snap/gnome-46-2404/145 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/gtk-common-themes_1535.snap on /snap/gtk-common-themes/1535 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/kf6-core24_34.snap on /snap/kf6-core24/34 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/kmahjongg_120.snap on /snap/kmahjongg/120 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/mesa-2404_1110.snap on /snap/mesa-2404/1110 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/mesa-2404_1165.snap on /snap/mesa-2404/1165 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/onlyoffice-desktopeditors_746.snap on /snap/onlyoffice-desktopeditors/746 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/snap-store_1270.snap on /snap/snap-store/1270 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/onlyoffice-desktopeditors_821.snap on /snap/onlyoffice-desktopeditors/821 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/snapd_25202.snap on /snap/snapd/25202 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/snapd_25577.snap on /snap/snapd/25577 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/snapd-desktop-integration_315.snap on /snap/snapd-desktop-integration/315 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/thincast-client_575.snap on /snap/thincast-client/575 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/thincast-client_605.snap on /snap/thincast-client/605 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/vlc_3777.snap on /snap/vlc/3777 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/wine-platform-9-devel-core22_33.snap on /snap/wine-platform-9-devel-core22/33 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/wine-platform-runtime-core22_104.snap on /snap/wine-platform-runtime-core22/104 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/var/lib/snapd/snaps/wine-platform-runtime-core22_105.snap on /snap/wine-platform-runtime-core22/105 type squashfs (ro,nodev,relatime,errors=continue,threads=single,x-gdu.hide,x-gvfs-hide)
/dev/sda1 on /boot/efi type vfat (rw,relatime,fmask=0022,dmask=0022,codepage=437,iocharset=iso8859-1,shortname=mixed,errors=remount-ro)
binfmt_misc on /proc/sys/fs/binfmt_misc type binfmt_misc (rw,nosuid,nodev,noexec,relatime)
tmpfs on /run/snapd/ns type tmpfs (rw,nosuid,nodev,noexec,relatime,size=1213172k,mode=755,inode64)
nsfs on /run/snapd/ns/cups.mnt type nsfs (rw)
nsfs on /run/snapd/ns/snapd-desktop-integration.mnt type nsfs (rw)
tmpfs on /run/user/1000 type tmpfs (rw,nosuid,nodev,relatime,size=1213168k,nr_inodes=303292,mode=700,uid=1000,gid=1000,inode64)
portal on /run/user/1000/doc type fuse.portal (rw,nosuid,nodev,relatime,user_id=1000,group_id=1000)
gvfsd-fuse on /run/user/1000/gvfs type fuse.gvfsd-fuse (rw,nosuid,nodev,relatime,user_id=1000,group_id=1000) or specialized user-space tools ().
    - For v0.3, **Building will still require Sudo** (or we use a VM-based builder later).
    - **Running** will be rootless.

4.  **Cgroups**:
    - Rootless Cgroups (v2) require systemd delegation. We will attempt to use user-slice cgroups if available, or warn and disable resource limits if not delegated.

### Consequences

- **Pros**:  works without . Safer.
- **Cons**: Performance hit on startup (copying). Networking performance with Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit is lower than Bridge/TAP.
- **Dependency**: Requires Usage: slirp4netns [OPTION]... PID|PATH [TAPNAME]
User-mode networking for unprivileged network namespaces.

-c, --configure          bring up the interface
-e, --exit-fd=FD         specify the FD for terminating slirp4netns
-r, --ready-fd=FD        specify the FD to write to when the network is configured
-m, --mtu=MTU            specify MTU (default=1500, max=65521)
-6, --enable-ipv6        enable IPv6 (experimental)
-a, --api-socket=PATH    specify API socket path
--cidr=CIDR              specify network address CIDR (default=10.0.2.0/24)
--disable-host-loopback  prohibit connecting to 127.0.0.1:* on the host namespace
--netns-type=TYPE 	 specify network namespace type ([path|pid], default=pid)
--userns-path=PATH	 specify user namespace path
--enable-sandbox         create a new mount namespace (and drop all caps except CAP_NET_BIND_SERVICE if running as the root)
--enable-seccomp         enable seccomp to limit syscalls (experimental)
--outbound-addr=IPv4     sets outbound ipv4 address to bound to (experimental)
--outbound-addr6=IPv6    sets outbound ipv6 address to bound to (experimental)
--disable-dns            disables 10.0.2.3 (or configured internal ip) to host dns redirect (experimental)
--macaddress=MAC         specify the MAC address of the TAP (only valid with -c)
--target-type=TYPE       specify the target type ([netns|bess], default=netns)
-h, --help               show this help and exit
-v, --version            show version and exit installed on host.

## 018. Vyoma Compose Strategy
*   **Date**: 2026-01-23
*   **Decision**: Implement `vyoma-compose` crate and `vyoma up` command to support multi-VM orchestration using a Docker Compose-like YAML format.
*   **Reasoning**:
    *   **User Experience**: Adopting the familiar Compose spec reduces the learning curve for users migrating from Docker.
    *   **Separation of Concerns**: The orchestration logic (dependency resolution, config parsing) is separated into a dedicated library (`vyoma-compose`), keeping the core primitives (`vyoma-core`) focused on single-VM lifecycle.
    *   **Integrated Workflow**: Supporting `build` contexts allows for a seamless "Source to Running App" workflow, unlike a pure runtime orchestrator.
*   **Implementation**:
    *   **Crate**: `crates/vyoma-compose` for strong typing of the YAML schema.
    *   **CLI**: `vyoma up` acts as the entry point. It orchestrates the `vyoma-core` API (or daemon API) to start services.
    *   **Networking**: Services will eventually share a dedicated CNI network (bridge) to allow name-based resolution (Phase 16).
*   **Consequences**:
    *   Introduces `serde_yaml` dependency.
    *   Requires tracking "Stacks" or "Groups" of VMs (currently implicit via labels or naming conventions).

## 020. Initramfs-based Agent Injection
*   **Date**: 2026-05-10
*   **Decision**: Replace mount-based injection with gzip-compressed cpio initramfs passed to Cloud Hypervisor via `PayloadConfig::initramfs`.
*   **Reasoning**:
    *   **Eliminates race conditions**: No shared mount points between concurrent VM boots
    *   **Supports rootless mode**: Initramfs passed to CH, no root required for injection
    *   **Better performance**: No mount/unmount overhead (.vyoma500ms savings)
    *   **Atomic creation**: File written once, no partial state
*   **Details**: See `docs/decisions/020-initramfs-agent-injection.md`
*   **Consequences**:
    *   Larger memory footprint (initramfs in RAM)
    *   .vyoma400KB agent binary per VM (acceptable)
    *   Initramfs regenerated each boot (milliseconds, acceptable)
