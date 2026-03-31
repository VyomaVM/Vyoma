# ADR-038: vk8s - Kubernetes CRI Plugin

**Status**: Accepted | Phase 4.3 (v1.8)

## Summary
Implement Kubernetes CRI (Container Runtime Interface) plugin to run Ignite MicroVMs as Pods in Kubernetes.

## Context
As part of Phase 4, we need to enable Kubernetes integration. This allows:
- Running MicroVMs as Kubernetes Pods
- Using Ignite with Kubernetes scheduling
- Container isolation with VM-level security

## Decision
Implement vk8s as per the technical spec:

### Architecture
- Go-based CRI server implementation
- Uses gRPC to communicate with kubelet
- Communicates with ignited via gRPC client

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
type IgniteCriServer struct {
    client *ignite.Client  // gRPC client to ignited
}

// RunPodSandbox creates a VM for a Kubernetes Pod
func (s *IgniteCriServer) RunPodSandbox(ctx context.Context, req *pb.RunPodSandboxRequest) (*pb.RunPodSandboxResponse, error) {
    // Create VM configuration from pod spec
    vmConfig := &ignite.CreateVmRequest{
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
- Socket: `unix:///var/run/ignite-cri.sock`
- RuntimeClass: `ignite-microvm`

## Consequences
- Kubernetes-native MicroVM management
- Pod scheduling with VM isolation
- Integration with Kubernetes ecosystem
