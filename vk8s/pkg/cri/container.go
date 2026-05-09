package cri

import (
	"context"
	"fmt"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
)

type ContainerInfo struct {
	ID        string
	PodID     string
	Name      string
	Image     string
	Created   int64
	State     pb.ContainerState
	Config    *pb.ContainerConfig
}

func (s *VyomaCriServer) CreateContainer(ctx context.Context, req *pb.CreateContainerRequest) (*pb.CreateContainerResponse, error) {
	podID := req.GetPodSandboxId()
	config := req.GetConfig()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	containerID := fmt.Sprintf("%s-%s", pod.VMID, config.GetMetadata().GetName())

	return &pb.CreateContainerResponse{ContainerId: containerID}, nil
}

func (s *VyomaCriServer) StartContainer(ctx context.Context, req *pb.StartContainerRequest) (*pb.StartContainerResponse, error) {
	return &pb.StartContainerResponse{}, nil
}

func (s *VyomaCriServer) StopContainer(ctx context.Context, req *pb.StopContainerRequest) (*pb.StopContainerResponse, error) {
	return &pb.StopContainerResponse{}, nil
}

func (s *VyomaCriServer) RemoveContainer(ctx context.Context, req *pb.RemoveContainerRequest) (*pb.RemoveContainerResponse, error) {
	return &pb.RemoveContainerResponse{}, nil
}

func (s *VyomaCriServer) ContainerStatus(ctx context.Context, req *pb.ContainerStatusRequest) (*pb.ContainerStatusResponse, error) {
	return &pb.ContainerStatusResponse{
		Status: &pb.ContainerStatus{
			Id:    req.ContainerId,
			State: pb.ContainerState_CONTAINER_RUNNING,
		},
	}, nil
}

func (s *VyomaCriServer) ListContainers(ctx context.Context, req *pb.ListContainersRequest) (*pb.ListContainersResponse, error) {
	return &pb.ListContainersResponse{}, nil
}

func (s *VyomaCriServer) UpdateContainerResources(ctx context.Context, req *pb.UpdateContainerResourcesRequest) (*pb.UpdateContainerResourcesResponse, error) {
	return &pb.UpdateContainerResourcesResponse{}, nil
}

func (s *VyomaCriServer) ReopenContainerLog(ctx context.Context, req *pb.ReopenContainerLogRequest) (*pb.ReopenContainerLogResponse, error) {
	return &pb.ReopenContainerLogResponse{}, nil
}