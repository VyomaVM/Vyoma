package cri

import (
	"context"
	"fmt"
	"net"
	"sync"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc"

	ignite "github.com/ignite/vk8s/pkg/ignite"
)

const (
	SocketPath = "/var/run/ignite-cri.sock"
)

type IgniteCriServer struct {
	pb.UnimplementedRuntimeServiceServer
	pb.UnimplementedImageServiceServer

	client  *ignite.Client
	pods    map[string]*PodSandbox
	mu      sync.RWMutex
}

type PodSandbox struct {
	ID      string
	Name    string
	State   pb.PodSandboxState
	VMID    string
	Created int64
}

func NewIgniteCriServer(igniteAddr string) (*IgniteCriServer, error) {
	client, err := ignite.NewClient(igniteAddr)
	if err != nil {
		return nil, fmt.Errorf("failed to create ignite client: %w", err)
	}

	return &IgniteCriServer{
		client: client,
		pods:   make(map[string]*PodSandbox),
	}, nil
}

func (s *IgniteCriServer) RunPodSandbox(ctx context.Context, req *pb.RunPodSandboxRequest) (*pb.RunPodSandboxResponse, error) {
	config := req.GetConfig()

	vmConfig := &ignite.CreateVmRequest{
		Name:      config.GetMetadata().GetName(),
		Namespace: config.GetMetadata().GetNamespace(),
		Vcpus:     uint32(config.GetLinux().GetResources().GetCpuQuota() / 100000),
		MemoryMb:  uint64(config.GetLinux().GetResources().GetMemoryLimitInBytes() / 1024 / 1024),
		Labels: map[string]string{
			"k8s.io/pod-name":       config.GetMetadata().GetName(),
			"k8s.io/pod-namespace":  config.GetMetadata().GetNamespace(),
			"k8s.io/pod-uid":        config.GetMetadata().GetUid(),
		},
	}

	vmResp, err := s.client.CreateVm(ctx, vmConfig)
	if err != nil {
		return nil, fmt.Errorf("failed to create VM: %w", err)
	}

	vmID := vmResp.GetVmId()

	if err := s.client.StartVm(ctx, &ignite.VmIdRequest{VmId: vmID}); err != nil {
		return nil, fmt.Errorf("failed to start VM: %w", err)
	}

	podID := fmt.Sprintf("pod-%s", vmID)

	s.mu.Lock()
	s.pods[podID] = &PodSandbox{
		ID:      podID,
		Name:    config.GetMetadata().GetName(),
		State:   pb.PodSandboxState_SANDBOX_READY,
		VMID:    vmID,
		Created: 0,
	}
	s.mu.Unlock()

	return &pb.RunPodSandboxResponse{PodSandboxId: podID}, nil
}

func (s *IgniteCriServer) StopPodSandbox(ctx context.Context, req *pb.StopPodSandboxRequest) (*pb.StopPodSandboxResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	if err := s.client.StopVm(ctx, &ignite.VmIdRequest{VmId: pod.VMID}); err != nil {
		return nil, fmt.Errorf("failed to stop VM: %w", err)
	}

	s.mu.Lock()
	pod.State = pb.PodSandboxState_SANDBOX_NOTREADY
	s.mu.Unlock()

	return &pb.StopPodSandboxResponse{}, nil
}

func (s *IgniteCriServer) RemovePodSandbox(ctx context.Context, req *pb.RemovePodSandboxRequest) (*pb.RemovePodSandboxResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.Lock()
	pod, ok := s.pods[podID]
	if ok {
		if err := s.client.DeleteVm(ctx, &ignite.VmIdRequest{VmId: pod.VMID}); err != nil {
			s.mu.Unlock()
			return nil, fmt.Errorf("failed to delete VM: %w", err)
		}
		delete(s.pods, podID)
	}
	s.mu.Unlock()

	return &pb.RemovePodSandboxResponse{}, nil
}

func (s *IgniteCriServer) PodSandboxStatus(ctx context.Context, req *pb.PodSandboxStatusRequest) (*pb.PodSandboxStatusResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	state := pb.PodSandboxState_SANDBOX_NOTREADY
	if pod.State == pb.PodSandboxState_SANDBOX_READY {
		state = pb.PodSandboxState_SANDBOX_READY
	}

	return &pb.PodSandboxStatusResponse{
		Status: &pb.PodSandboxStatus{
			Id:          pod.ID,
			Metadata:    &pb.PodSandboxMetadata{Name: pod.Name},
			State:       state,
			CreatedAt:   pod.Created,
		},
	}, nil
}

func (s *IgniteCriServer) ListPodSandbox(ctx context.Context, req *pb.ListPodSandboxRequest) (*pb.ListPodSandboxResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	var pods []*pb.PodSandbox
	for _, pod := range s.pods {
		pods = append(pods, &pb.PodSandbox{
			Id:        pod.ID,
			Metadata:  &pb.PodSandboxMetadata{Name: pod.Name},
			State:     pod.State,
			CreatedAt: pod.Created,
		})
	}

	return &pb.ListPodSandboxResponse{Items: pods}, nil
}

func (s *IgniteCriServer) CreateContainer(ctx context.Context, req *pb.CreateContainerRequest) (*pb.CreateContainerResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	containerID := fmt.Sprintf("%s-%d", pod.VMID, 0)

	return &pb.CreateContainerResponse{ContainerId: containerID}, nil
}

func (s *IgniteCriServer) StartContainer(ctx context.Context, req *pb.StartContainerRequest) (*pb.StartContainerResponse, error) {
	return &pb.StartContainerResponse{}, nil
}

func (s *IgniteCriServer) StopContainer(ctx context.Context, req *pb.StopContainerRequest) (*pb.StopContainerResponse, error) {
	return &pb.StopContainerResponse{}, nil
}

func (s *IgniteCriServer) RemoveContainer(ctx context.Context, req *pb.RemoveContainerRequest) (*pb.RemoveContainerResponse, error) {
	return &pb.RemoveContainerResponse{}, nil
}

func (s *IgniteCriServer) ContainerStatus(ctx context.Context, req *pb.ContainerStatusRequest) (*pb.ContainerStatusResponse, error) {
	return &pb.ContainerStatusResponse{
		Status: &pb.ContainerStatus{
			Id:    req.ContainerId,
			State: pb.ContainerState_CONTAINER_RUNNING,
		},
	}, nil
}

func (s *IgniteCriServer) ListContainers(ctx context.Context, req *pb.ListContainersRequest) (*pb.ListContainersResponse, error) {
	return &pb.ListContainersResponse{}, nil
}

func (s *IgniteCriServer) Run(ctx context.Context) error {
	lis, err := net.Listen("unix", SocketPath)
	if err != nil {
		return fmt.Errorf("failed to listen: %w", err)
	}

	grpcServer := grpc.NewServer()
	pb.RegisterRuntimeServiceServer(grpcServer, s)
	pb.RegisterImageServiceServer(grpcServer, s)

	return grpcServer.Serve(lis)
}
