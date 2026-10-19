# vk8s: Kubernetes Container Runtime Interface (CRI)

`vk8s` is a Kubernetes CRI plugin that allows Kubernetes clusters to run Pods as hardware-isolated Vyoma Micro-VMs instead of traditional Linux containers.

---

## 1. Architecture

```
┌────────────────────────────────────────────────────────┐
│                        Kubelet                         │
└───────────────────────────┬────────────────────────────┘
                            │ CRI gRPC API
                            ▼
┌────────────────────────────────────────────────────────┐
│                         vk8s                           │
│                 (CRI Shim Daemon in Go)                │
│             /var/run/vyoma-cri.sock                    │
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

---

## 2. CRI Mapping

| Kubernetes CRI Call | Vyoma Action | Implementation |
|---------------------|--------------|----------------|
| `RunPodSandbox` | Boots a dedicated Micro-VM with Pod CPU/memory limits | `vk8s/pkg/cri/server.go:RunPodSandbox` |
| `StopPodSandbox` | Sends graceful shutdown to VM | `vk8s/pkg/cri/server.go:StopPodSandbox` |
| `RemovePodSandbox` | Destroys VM and releases CNI network & storage | `vk8s/pkg/cri/server.go:RemovePodSandbox` |
| `CreateContainer` | Prepares container filesystem inside VM | `vk8s/pkg/cri/server.go:CreateContainer` |
| `StartContainer` | Starts container command via in-VM agent | `vk8s/pkg/cri/server.go:StartContainer` |
| `ExecSync` | Executes command inside container via guest agent | `vk8s/pkg/cri/server.go:ExecSync` |

---

## 3. Kubernetes RuntimeClass Configuration

To run workloads on Vyoma:

```yaml
apiVersion: node.k8s.io/v1
kind: RuntimeClass
metadata:
  name: vyoma-microvm
handler: vyoma
---
apiVersion: v1
kind: Pod
metadata:
  name: secure-workload
spec:
  runtimeClassName: vyoma-microvm
  containers:
    - name: app
      image: nginx:alpine
```
