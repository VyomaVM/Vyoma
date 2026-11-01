# Website Alignment Patch & Pull Request Guide (v2.9.0)

This patch document contains the exact unified diffs and copy corrections required for the marketing website (`Vyom-Site`) to bring 100% truth-in-advertising alignment with the actual Vyoma v2.9.0 engine.

---

## Summary of Changes

| Section | Current Marketing Claim | Aligned Reality (v2.9.0) |
|---|---|---|
| **Version** | `v2.8.0` / `v2.1.2` | **`v2.9.0`** across all release cards, badges, and downloads |
| **Boot Time** | `Under 150ms boot` / `from pull` | **`<250ms Cold Boot`** (local image; pulls depend on network) |
| **Memory Overhead** | `~5 MB VM overhead` | **`~30 MB Tiny Overhead`** (VMM RSS + Linux guest kernel) |
| **Security Architecture**| `7 layers of defense` | **`4 Enterprise Security Pillars`** (KVM, PrivDrop, TPM2, Mesh) |
| **Hibernation & Resume** | `vyoma start staging-env` | **`vyoma resume staging-env`** (strictly `resume`, NO `start`) |
| **Snapshots & Restore** | `vyoma restore` / `time-travel` | Unified `vyoma restore <id> --from <tag>` |
| **Live Teleportation** | `vyoma teleport <id> <ip>` | `vyoma teleport <id> --to <node-name>` |
| **Compose Discovery** | Requires `vyoma-compose.yml` | Auto-discovers `docker-compose.yml` and `compose.yaml` |
| **Ecosystem: Studio** | Tagged *"Planned"* | Upgraded badge: **`Active / Built-in`** (live in dashboard) |
| **Ecosystem: SDK** | Go, Rust | **Python SDK (`pip install vyoma`)**, Go, and Rust |
| **Kubernetes CRI** | Tagged *"Alpha"* | **`vyoma-cri` v1 shim** with `RuntimeClass: vyoma-microvm` |

---

## Ready-to-Apply Diffs

### 1. Hero & Metrics Section (`src/components/Hero.astro` / `src/pages/index.astro`)

```diff
--- a/src/components/Hero.astro
+++ b/src/components/Hero.astro
@@ -15,11 +15,11 @@
-    <span class="badge">v2.8.0 Released</span>
+    <span class="badge">v2.9.0 Released</span>
     <h1>Hardware Isolation. Docker Ergonomics.</h1>
     <p class="hero-subtext">Spin up hardware-isolated microVMs in milliseconds using standard OCI images.</p>
     
     <div class="stats-grid">
       <div class="stat-card">
-        <span class="stat-value">&lt;150ms</span>
-        <span class="stat-label">Cold boot (from pull)</span>
+        <span class="stat-value">&lt;250ms</span>
+        <span class="stat-label">Cold boot (local image)</span>
       </div>
       <div class="stat-card">
-        <span class="stat-value">~5 MB</span>
-        <span class="stat-label">Memory overhead per VM</span>
+        <span class="stat-value">~30 MB</span>
+        <span class="stat-label">Memory overhead per VM</span>
       </div>
       <div class="stat-card">
         <span class="stat-value">0</span>
         <span class="stat-label">Config files required</span>
       </div>
     </div>
```

---

### 2. Interactive Terminal / CLI Snippets (`src/components/TerminalDemo.astro`)

```diff
--- a/src/components/TerminalDemo.astro
+++ b/src/components/TerminalDemo.astro
@@ -28,8 +28,8 @@
 # Scale to zero when idle
 $ vyoma hibernate staging-env
-  💤 0 vCPUs, 0 MB — state saved
-$ vyoma start staging-env
-  ⚡ Resumed in 180ms
+  💤 0 vCPUs, 0 MB — state saved
+$ vyoma resume staging-env
+  ⚡ Resumed in 180ms
 
 # Live teleport across cluster nodes
-$ vyoma teleport api-server 192.168.1.10
+$ vyoma teleport api-server --to node-eu-west-1
   🚚 Teleporting memory pages... [====================] 100%
   ⚡ Switched execution in 42ms
```

---

### 3. Security Architecture Section: 4 Pillars (`src/components/Security.astro`)

```diff
--- a/src/components/Security.astro
+++ b/src/components/Security.astro
@@ -5,28 +5,36 @@
-  <h2>7 Layers of Defense. Zero Trust Architecture.</h2>
-  <p>Every microVM is protected by seven independent security layers.</p>
+  <h2>The 4 Enterprise Security Pillars</h2>
+  <p>Hardware-rooted defense in depth engineered into every microVM lifecycle.</p>
   
   <div class="security-pillars-grid">
-    <div class="pillar-card">Layer 1: KVM Kernel Isolation</div>
-    <div class="pillar-card">Layer 2: Cloud Hypervisor Minimal VMM</div>
-    <div class="pillar-card">Layer 3: Minimal Virtio Device Model</div>
-    <div class="pillar-card">Layer 4: Least-Privilege Daemon</div>
-    <div class="pillar-card">Layer 5: WireGuard Mesh Encryption</div>
-    <div class="pillar-card">Layer 6: Image Signing Ed25519</div>
-    <div class="pillar-card">Layer 7: Capability API Auth</div>
+    <div class="pillar-card">
+      <h3>1. Hardware MicroVM Isolation</h3>
+      <p>Dedicated Linux 6.16.9+ guest kernel per VM instance on KVM. 100% independent syscall boundary immune to container escapes with virtio-only devices.</p>
+    </div>
+    <div class="pillar-card">
+      <h3>2. Least-Privilege Daemon</h3>
+      <p>Daemon immediately drops root to dedicated 'vyoma' user (privdrop.rs) with minimal ambient capabilities (CAP_NET_ADMIN, CAP_SYS_ADMIN).</p>
+    </div>
+    <div class="pillar-card">
+      <h3>3. Measured Boot & Attestation</h3>
+      <p>Virtual TPM 2.0 (swtpm) with PCR measurement registers and cryptographic remote attestation quotes (vyoma attest).</p>
+    </div>
+    <div class="pillar-card">
+      <h3>4. Encrypted Mesh & Auth</h3>
+      <p>L2/L3 TAP network isolation, WireGuard encrypted inter-node tunnels, VXLAN clustering, and Bearer token API authorization.</p>
+    </div>
   </div>
```

---

### 4. Ecosystem & Platform Cards (`src/components/Ecosystem.astro`)

```diff
--- a/src/components/Ecosystem.astro
+++ b/src/components/Ecosystem.astro
@@ -20,7 +20,7 @@
     <div class="card">
       <h3>CLI</h3>
-      <span class="badge active">Shipped</span>
-      <p>Web UI at localhost:8080...</p>
+      <span class="badge active">Shipped</span>
+      <p>25+ commands for full microVM lifecycle, snapshots, live teleportation, and compose stacks directly from terminal.</p>
     </div>
 
     <div class="card">
       <h3>Studio</h3>
-      <span class="badge planned">Planned</span>
-      <p>Visual workspace for topology and stacks.</p>
+      <span class="badge active">Active / Built-in</span>
+      <p>Visual compose stack editor and real-time network topology visualizer built directly into the Web Dashboard.</p>
     </div>
 
     <div class="card">
       <h3>SDKs</h3>
       <span class="badge alpha">Alpha</span>
-      <p>Official client libraries for Go and Rust.</p>
+      <p>Official client libraries for Python (pip install vyoma), Go, and Rust.</p>
     </div>
```

---

### 5. Download & Packages Section (`src/components/Download.astro`)

```diff
--- a/src/components/Download.astro
+++ b/src/components/Download.astro
@@ -10,8 +10,8 @@
   <div class="download-buttons">
-    <a href="https://github.com/Subeshrock/vyoma/releases/download/v2.8.0/vyoma_2.8.0_amd64.deb" class="btn">
-      Download .deb (v2.8.0)
+    <a href="https://github.com/Subeshrock/vyoma/releases/download/v2.9.0/vyoma_2.9.0_amd64.deb" class="btn">
+      Download .deb (v2.9.0)
     </a>
-    <a href="https://github.com/Subeshrock/vyoma/releases/download/v2.8.0/vyoma-2.8.0-1.x86_64.rpm" class="btn">
-      Download .rpm (v2.8.0)
+    <a href="https://github.com/Subeshrock/vyoma/releases/download/v2.9.0/vyoma-2.9.0-1.x86_64.rpm" class="btn">
+      Download .rpm (v2.9.0)
     </a>
   </div>
```
