package cri

import (
	"context"
	"encoding/json"
	"fmt"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc/codes"
)

func (s *VyomaCriServer) CreateContainer(ctx context.Context, req *pb.CreateContainerRequest) (*pb.CreateContainerResponse, error) {
	podID := req.GetPodSandboxId()
	config := req.GetConfig()
	metadata := config.GetMetadata()

	s.logger.Printf("CreateContainer: pod=%s name=%s image=%s", podID, metadata.GetName(), config.GetImage().GetImage())

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "pod not found: %s", podID)
	}

	containerID := fmt.Sprintf("%s-%s", pod.VMID, metadata.GetName())

	execReq := map[string]interface{}{
		"vm_id":     pod.VMID,
		"command":   []string{"mkdir", "-p", "/var/lib/vyoma/containers/" + containerID},
	}
	if _, err := s.httpRequest(ctx, "POST", "/exec", execReq); err != nil {
		s.logError(ctx, "CreateContainer.mkdir", err)
	}

	s.mu.Lock()
	s.containers[containerID] = &ContainerInfo{
		ID:      containerID,
		PodID:   podID,
		Name:    metadata.GetName(),
		Image:   config.GetImage().GetImage(),
		Created: 0,
		State:   pb.ContainerState_CONTAINER_CREATED,
		Config:  config,
	}
	s.mu.Unlock()

	return &pb.CreateContainerResponse{ContainerId: containerID}, nil
}

func (s *VyomaCriServer) StartContainer(ctx context.Context, req *pb.StartContainerRequest) (*pb.StartContainerResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("StartContainer: %s", containerID)

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	s.mu.RLock()
	pod, ok := s.pods[container.PodID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "pod not found for container: %s", containerID)
	}

	cmd := []string{"/bin/sh", "-c", "echo container started"}
	if container.Config != nil && container.Config.Command != nil {
		cmd = container.Config.Command
	}

	execReq := map[string]interface{}{
		"vm_id":   pod.VMID,
		"command": cmd,
	}

	if _, err := s.httpRequest(ctx, "POST", "/exec", execReq); err != nil {
		s.logError(ctx, "StartContainer.exec", err)
	}

	s.mu.Lock()
	container.State = pb.ContainerState_CONTAINER_RUNNING
	s.mu.Unlock()

	return &pb.StartContainerResponse{}, nil
}

func (s *VyomaCriServer) StopContainer(ctx context.Context, req *pb.StopContainerRequest) (*pb.StopContainerResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("StopContainer: %s", containerID)

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return &pb.StopContainerResponse{}, nil
	}

	s.mu.RLock()
	pod, ok := s.pods[container.PodID]
	s.mu.RUnlock()

	if ok {
		execReq := map[string]interface{}{
			"vm_id":   pod.VMID,
			"command": []string{"kill", "1"},
		}
		if _, err := s.httpRequest(ctx, "POST", "/exec", execReq); err != nil {
			s.logError(ctx, "StopContainer.exec", err)
		}
	}

	s.mu.Lock()
	container.State = pb.ContainerState_CONTAINER_EXITED
	s.mu.Unlock()

	return &pb.StopContainerResponse{}, nil
}

func (s *VyomaCriServer) RemoveContainer(ctx context.Context, req *pb.RemoveContainerRequest) (*pb.RemoveContainerResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("RemoveContainer: %s", containerID)

	s.mu.Lock()
	delete(s.containers, containerID)
	s.mu.Unlock()

	return &pb.RemoveContainerResponse{}, nil
}

func (s *VyomaCriServer) ContainerStatus(ctx context.Context, req *pb.ContainerStatusRequest) (*pb.ContainerStatusResponse, error) {
	containerID := req.GetContainerId()

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	reason := ""
	switch container.State {
	case pb.ContainerState_CONTAINER_CREATED:
		reason = "Created"
	case pb.ContainerState_CONTAINER_RUNNING:
		reason = "Running"
	case pb.ContainerState_CONTAINER_EXITED:
		reason = "Exited"
	}

	return &pb.ContainerStatusResponse{
		Status: &pb.ContainerStatus{
			Id:                container.ID,
			Metadata:           &pb.ContainerMetadata{Name: container.Name},
			State:             container.State,
			CreatedAt:          container.Created,
			Image:              &pb.ImageSpec{Image: container.Image},
			ImageRef:           container.Image,
			Reason:             reason,
			StartedAt:          0,
			FinishedAt:         0,
			ExitCode:           0,
			Signal:             0,
			RestartCount:       0,
			LogPath:            fmt.Sprintf("/var/log/pods/%s/%s/*.log", container.PodID, container.ID),
		},
	}, nil
}

func (s *VyomaCriServer) ListContainers(ctx context.Context, req *pb.ListContainersRequest) (*pb.ListContainersResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	filter := req.GetFilter()
	containers := make([]*pb.Container, 0, len(s.containers))

	for _, c := range s.containers {
		if filter != nil {
			if filter.Id != "" && c.ID != filter.Id {
				continue
			}
			if filter.PodSandboxId != "" && c.PodID != filter.PodSandboxId {
				continue
			}
			if len(filter.LabelSelectors) > 0 {
				match := true
				for _, sel := range filter.LabelSelectors {
					if c.Config == nil || c.Config.Labels == nil {
						match = false
						break
					}
					if val, ok := c.Config.Labels[sel]; !ok || val == "" {
						match = false
						break
					}
				}
				if !match {
					continue
				}
			}
		}

		containers = append(containers, &pb.Container{
			Id:        c.ID,
			PodSandboxId: c.PodID,
			Metadata:  &pb.ContainerMetadata{Name: c.Name},
			Image:     c.Image,
			ImageRef:  c.Image,
			State:     c.State,
			CreatedAt: c.Created,
		})
	}

	return &pb.ListContainersResponse{Containers: containers}, nil
}

func (s *VyomaCriServer) UpdateContainerResources(ctx context.Context, req *pb.UpdateContainerResourcesRequest) (*pb.UpdateContainerResourcesResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("UpdateContainerResources: %s", containerID)

	return &pb.UpdateContainerResourcesResponse{}, nil
}

func (s *VyomaCriServer) ReopenContainerLog(ctx context.Context, req *pb.ReopenContainerLogRequest) (*pb.ReopenContainerLogResponse, error) {
	return &pb.ReopenContainerLogResponse{}, nil
}

type execRequest struct {
	VMID   string   `json:"vm_id"`
	Command []string `json:"command"`
}

type execResponse struct {
	Stdout   string `json:"stdout"`
	Stderr   string `json:"stderr"`
	ExitCode int    `json:"exit_code"`
}

func (s *VyomaCriServer) execInVM(ctx context.Context, vmID string, command []string) (*execResponse, error) {
	data, err := s.httpRequest(ctx, "POST", "/exec", &execRequest{
		VMID:    vmID,
		Command: command,
	})
	if err != nil {
		return nil, err
	}

	var resp execResponse
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, err
	}

	return &resp, nil
}

func (s *VyomaCriServer) syncContainersFromVyomad(ctx context.Context) error {
	s.mu.RLock()
	podIDs := make([]string, 0, len(s.pods))
	for podID := range s.pods {
		podIDs = append(podIDs, podID)
	}
	s.mu.RUnlock()

	for _, podID := range podIDs {
		s.mu.RLock()
		pod := s.pods[podID]
		s.mu.RUnlock()

		if pod == nil {
			continue
		}

		resp, err := s.execInVM(ctx, pod.VMID, []string{"cat", "/proc/1/cmdline"})
		if err != nil {
			continue
		}

		s.mu.Lock()
		for containerID, container := range s.containers {
			if container.PodID == podID {
				if resp.ExitCode == 0 {
					container.State = pb.ContainerState_CONTAINER_RUNNING
				}
				_ = resp
			}
		}
		s.mu.Unlock()
	}

	return nil
}

type volumeMount struct {
	HostPath      string `json:"host_path"`
	ContainerPath string `json:"container_path"`
	Readonly      bool   `json:"readonly"`
}

type portMapping struct {
	Protocol      string `json:"protocol"`
	ContainerPort uint32 `json:"container_port"`
	HostPort      uint32 `json:"host_port"`
}