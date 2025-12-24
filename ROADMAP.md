# Ignite: Road to v1.0 (The "Docker Killer" Roadmap)

To bridge the gap between "Cool Prototype" and "Daily Driver", we need to address specific developer workflows that Docker handles seamlessly.

## 1. Networking (Critical for Web Devs) 🌐
*   **Port Mapping (`-p 8080:80`)**: currently, VMs get their own IP. Developers are used to `localhost:8080`. We need a proxy (like `socat` or a custom Rust proxy) to forward host ports to the VM IP.
*   **DNS Resolution**: VMs currently struggle to resolve hostnames if not configured. We need a proper DNS forwarder.
*   **CNI Support**: To play nice with Kubernetes eventually, we should implement the Container Network Interface (CNI) instead of our custom `ip` commands.

## 2. Storage & Filesystem (Critical for Persistence) 💾
*   **Volume Mounts (`-v ./code:/app`)**: This is the #1 feature for developers. They want to edit code on their host (VS Code) and see it update in the VM instantly.
    *   *Tech*: Implement **VirtioFS** support in Ignite. Firecracker supports it! This allows sharing host directories into the VM.
*   **Rootless Execution**: demanding `sudo` for `ignited` is a security risk and friction point. We should explore User Namespaces or rely on `sudo` wrapper helpers only for specific operations.

## 3. Developer Experience (DX) 🚀
*   **`Ignitefile` (Build System)**: We currently *pull* images. We need to *build* them.
    *   Need a DSL (like Dockerfile) that allows running commands to install packages, creating a new snapshot as the base for future VMs.
*   **`ign logs -f`**: Real-time log streaming from the VM console. Currently, we just dump stdout.
*   **`ign exec -it`**: Interactive shell access *into* a running VM without SSH. (Firecracker enables this via vsock console or serial console anchoring).

## 4. Security & Performance 🔒
*   **Cgroups (Resource Limits)**: Allow `ign run --cpus 2 --memory 4g`. We need to wire this up to the Linux Cgroup v2 API.
*   **Seccomp Profiles**: Harden the process syscalls.

## 5. The "Killer Feature" Expansions ⚡
*   **Time Travel UI**: A visual graph (TUI or Web) of the Git history of a VM. "Revert to 10 minutes ago" with one click.
*   **Instant Resume**: "Hibernate" a VM to disk and wake it up in 50ms on incoming network traffic (Serverless style).

## Impact Matrix
| Feature | Complexity | Value | Next Step |
| :--- | :---: | :---: | :--- |
| **Port Mapping** | Medium | High | Implement user-space proxy |
| **Volume Mounts** | High | Critical | Research VirtioFS |
| **Ignitefile** | Very High | High | Design Spec |
| **Resource Limits** | Low | Medium | Add Cgroups |
