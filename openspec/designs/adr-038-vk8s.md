# ADR-038: vk8s - Kubernetes CRI Plugin

**Status**: Accepted | Phase 4.3 (v1.8)

## Summary
Implement Kubernetes CRI (Container Runtime Interface) plugin to run Vyoma MicroVMs as Pods in Kubernetes.

## Context
As part of Phase 4, we need to enable Kubernetes integration. This allows:
- Running MicroVMs as Kubernetes Pods
- Using Vyoma with Kubernetes scheduling
- Container isolation with VM-level security

## Decision
Implement vk8s as per the technical spec:

### Architecture
- Go-based CRI server implementation
- Uses gRPC to communicate with kubelet
- Communicates with vyomad via gRPC client

### Key Functions
- `RunPodSandbox` - Create VM for Pod
- `CreateContainer` - Run container inside VM
- `StartContainer`, `StopContainer`, `RemoveContainer`
- `PodSandboxStatus`, `ContainerStatus`
- `ListPods`, `ListContainers`

## Implementation

### Location
- `vk8s/` - Go project directory

### Key Components
```go
type VyomaCriServer struct {
    client *vyoma.Client  // gRPC client to vyomad
}

// RunPodSandbox creates a VM for a Kubernetes Pod
func (s *VyomaCriServer) RunPodSandbox(ctx context.Context, req *pb.RunPodSandboxRequest) (*pb.RunPodSandboxResponse, error) {
    // Create VM configuration from pod spec
    vmConfig := &vyoma.CreateVmRequest{
        Name:       config.Metadata.Name,
        Namespace:  config.Metadata.Namespace,
        Vcpus:      uint32(config.Linux.Resources.CpuQuota / 100000),
        MemoryMb:   uint64(config.Linux.Resources.MemoryLimitInBytes / 1024 / 1024),
        Labels:     map[string]string{
            "k8s.io/pod-name":       config.Metadata.Name,
            "k8s.io/pod-namespace": config.Metadata.Namespace,
        },
    }
}
```

### Kubernetes Integration
- Socket: `unix:///var/run/vyoma-cri.sock`
- RuntimeClass: `vyoma-microvm`

## Consequences
- Kubernetes-native MicroVM management
- Pod scheduling with VM isolation
- Integration with Kubernetes ecosystem
