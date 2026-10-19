# vk8s: Kubernetes Container Runtime Interface (CRI)

`vk8s` (`vyoma-cri`) is a Kubernetes CRI v1 shim daemon that allows Kubernetes clusters to run Pods as hardware-isolated Vyoma Micro-VMs with physical KVM isolation instead of traditional shared-kernel Linux containers.

---

## 1. Architecture

```
┌────────────────────────────────────────────────────────┐
│                        Kubelet                         │
└───────────────────────────┬────────────────────────────┘
                            │ CRI v1 gRPC API
                            ▼
┌────────────────────────────────────────────────────────┐
│                       vyoma-cri                        │
│                 (CRI Shim Daemon in Go)                │
│         Unix Socket: /var/run/vyoma-cri.sock           │
└───────────────────────────┬────────────────────────────┘
                            │ Vyoma gRPC / REST
                            ▼
┌────────────────────────────────────────────────────────┐
│                        vyomad                          │
│             (Micro-VM Lifecycle Manager)               │
└───────────────────────────┬────────────────────────────┘
                            │
              ┌─────────────┴─────────────┐
              ▼                           ▼
      ┌───────────────┐           ┌───────────────┐
      │  Pod Sandbox  │           │  Pod Sandbox  │
      │   (VM #1)     │           │   (VM #2)     │
      └───────────────┘           └───────────────┘
```

Each Pod sandbox maps directly to a high-speed, lightweight Cloud Hypervisor microVM with dedicated vCPUs, memory limits, and guest agent execution.

---

## 2. CRI Mapping

| Kubernetes CRI Call | Vyoma Action | Implementation |
|---------------------|--------------|----------------|
| `RunPodSandbox` | Boots a dedicated Micro-VM with Pod CPU/memory limits | `vk8s/pkg/cri/pod_sandbox.go:RunPodSandbox` |
| `StopPodSandbox` | Sends graceful shutdown to VM | `vk8s/pkg/cri/pod_sandbox.go:StopPodSandbox` |
| `RemovePodSandbox` | Destroys VM and releases CNI network & storage | `vk8s/pkg/cri/pod_sandbox.go:RemovePodSandbox` |
| `PodSandboxStatus` | Inspects VM status and network IPs | `vk8s/pkg/cri/pod_sandbox.go:PodSandboxStatus` |
| `CreateContainer` | Prepares container filesystem inside VM | `vk8s/pkg/cri/container.go:CreateContainer` |
| `StartContainer` | Starts container command via in-VM agent | `vk8s/pkg/cri/container.go:StartContainer` |
| `ExecSync` / `Exec` | Executes command inside container via guest agent | `vk8s/pkg/cri/streaming.go:Exec` |

---

## 3. Daemon Installation & Startup

### Building `vyoma-cri`
The `vyoma-cri` binary can be built via `make`:
```bash
make vk8s
```
The compiled binary is placed at `bin/vyoma-cri` and automatically packaged into official `.deb` and `.rpm` releases under `/usr/local/bin/vyoma-cri`.

### Starting the CRI Shim
Run `vyoma-cri` pointing to the running `vyomad` instance:
```bash
sudo vyoma-cri \
  -vyoma-grpc localhost:7071 \
  -vyoma-http http://localhost:8080
```
By default, `vyoma-cri` listens on Unix domain socket `/var/run/vyoma-cri.sock`.

### Systemd Service Setup
Create `/etc/systemd/system/vyoma-cri.service`:
```ini
[Unit]
Description=Vyoma Kubernetes CRI Shim
After=network.target vyomad.service
Requires=vyomad.service

[Service]
Type=simple
ExecStart=/usr/local/bin/vyoma-cri -vyoma-grpc localhost:7071 -vyoma-http http://localhost:8080
Restart=always
RestartSec=5
KillMode=process

[Install]
WantedBy=multi-user.target
```
Enable and start the service:
```bash
sudo systemctl daemon-reload
sudo systemctl enable --now vyoma-cri.service
```

---

## 4. Testing with `crictl`

Configure `crictl` to interact directly with the `vyoma-cri` socket:

Create or edit `/etc/crictl.yaml`:
```yaml
runtime-endpoint: unix:///var/run/vyoma-cri.sock
image-endpoint: unix:///var/run/vyoma-cri.sock
timeout: 10
debug: false
```

### Common `crictl` Workflows:

1. **Verify Runtime Status**:
   ```bash
   crictl info
   crictl status
   ```

2. **Run a Pod Sandbox**:
   Create `pod-config.json`:
   ```json
   {
     "metadata": {
       "name": "microvm-sandbox",
       "namespace": "default",
       "attempt": 1,
       "uid": "hd923nd-0129-vyoma"
     },
     "log_directory": "/tmp",
     "linux": {
       "security_context": {
         "namespace_options": {
           "network": 2
         }
       }
     },
     "annotations": {
       "vyoma.image": "vyoma/alpine:latest"
     }
   }
   ```
   Start the sandbox:
   ```bash
   POD_ID=$(crictl runp pod-config.json)
   echo "Sandbox running: $POD_ID"
   ```

3. **List & Inspect Running Pods**:
   ```bash
   crictl pods
   crictl inspectp $POD_ID
   ```

4. **Stop and Remove Sandbox**:
   ```bash
   crictl stopp $POD_ID
   crictl rmp $POD_ID
   ```

---

## 5. Kubernetes RuntimeClass Configuration

To configure Kubernetes nodes to schedule sensitive or multi-tenant Pods onto Vyoma microVMs:

### 1. Register RuntimeClass in Kubernetes
Apply the `RuntimeClass` resource to your cluster:
```yaml
apiVersion: node.k8s.io/v1
kind: RuntimeClass
metadata:
  name: vyoma-microvm
handler: vyoma
```

### 2. Configure Node Kubelet / Containerd
If using containerd with CRI proxy:
```toml
[plugins."io.containerd.grpc.v1.cri".containerd.runtimes.vyoma]
  runtime_type = "io.containerd.runc.v2"
  privileged_without_host_devices = false
  pod_annotations = ["vyoma.*"]
```
Or set Kubelet's runtime endpoint to point directly to `unix:///var/run/vyoma-cri.sock`:
```bash
--container-runtime-endpoint=unix:///var/run/vyoma-cri.sock
```

### 3. Deploy Workloads Using `vyoma-microvm`
```yaml
apiVersion: v1
kind: Pod
metadata:
  name: isolated-workload
  annotations:
    vyoma.image: "vyoma/alpine:latest"
spec:
  runtimeClassName: vyoma-microvm
  containers:
    - name: app
      image: alpine:latest
      command: ["/bin/sh", "-c", "echo 'Running inside hardware-isolated Vyoma microVM'; sleep 3600"]
      resources:
        limits:
          cpu: "2"
          memory: "512Mi"
        requests:
          cpu: "1"
          memory: "256Mi"
```
Once scheduled, the Pod will run inside a hardware-isolated Cloud Hypervisor VM with dedicated vCPUs, private memory space, and zero shared kernel surface.
