package cri

import (
	"context"
	"encoding/json"
	"fmt"
	"sync"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"github.com/vyoma/vk8s/pkg/agent"
)

type ContainerInfo struct {
	ID        string
	PodID     string
	Name      string
	Image     string
	Created   int64
	State     pb.ContainerState
	Config    *pb.ContainerConfig
	Pid       uint32
	StartTime int64
}

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

	agentCli := agent.NewTCPClient(s.getVMIP(pod.VMID))
	if err := agentCli.Connect(ctx); err != nil {
		s.logError(ctx, "CreateContainer.agentConnect", err)
	}

	containerDir := fmt.Sprintf("/var/lib/vyoma/containers/%s", containerID)
	env := make(map[string]string)
	for _, e := range config.GetEnvs() {
		env[e.GetKey()] = e.GetValue()
	}

	mkdirCmd := []string{"mkdir", "-p", containerDir}
	if _, _, _, err := agentCli.ExecCommand(ctx, mkdirCmd, env, "/"); err != nil {
		s.logError(ctx, "CreateContainer.mkdir", err)
	}

	s.mu.Lock()
	s.containers[containerID] = &ContainerInfo{
		ID:      containerID,
		PodID:   podID,
		Name:    metadata.GetName(),
		Image:   config.GetImage().GetImage(),
		Created: time.Now().Unix(),
		State:   pb.ContainerState_CONTAINER_CREATED,
		Config:  config,
	}
	s.mu.Unlock()

	agentCli.Close()

	s.logger.Printf("Container created: %s", containerID)
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

	agentCli := agent.NewTCPClient(s.getVMIP(pod.VMID))
	if err := agentCli.Connect(ctx); err != nil {
		return nil, errorf(codes.Internal, "connect to VM agent: %v", err)
	}
	defer agentCli.Close()

	env := make(map[string]string)
	for _, e := range container.Config.GetEnvs() {
		env[e.GetKey()] = e.GetValue()
	}

	cmd := []string{"/bin/sh", "-c", "echo container started"}
	workdir := "/"
	if container.Config != nil {
		if len(container.Config.Command) > 0 {
			cmd = container.Config.Command
		}
		if container.Config.WorkingDir != "" {
			workdir = container.Config.WorkingDir
		}
	}

	stdout, stderr, exitCode, err := agentCli.ExecCommand(ctx, cmd, env, workdir)
	if err != nil {
		s.logError(ctx, "StartContainer.exec", err)
	}

	s.logger.Printf("Container start: %s exit=%d stdout=%s stderr=%s", containerID, exitCode, string(stdout), string(stderr))

	s.mu.Lock()
	container.State = pb.ContainerState_CONTAINER_RUNNING
	container.StartTime = time.Now().Unix()
	s.mu.Unlock()

	return &pb.StartContainerResponse{}, nil
}

func (s *VyomaCriServer) StopContainer(ctx context.Context, req *pb.StopContainerRequest) (*pb.StopContainerResponse, error) {
	containerID := req.GetContainerId()
	timeout := req.GetTimeout()
	if timeout == 0 {
		timeout = 10
	}

	s.logger.Printf("StopContainer: %s timeout=%ds", containerID, timeout)

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
		agentCli := agent.NewTCPClient(s.getVMIP(pod.VMID))
		if err := agentCli.Connect(ctx); err == nil {
			defer agentCli.Close()

			sigTerm := []string{"kill", "-TERM", "1"}
			agentCli.ExecCommand(ctx, sigTerm, nil, "/")

			time.Sleep(time.Duration(timeout) * time.Second)

			sigKill := []string{"kill", "-KILL", "1"}
			agentCli.ExecCommand(ctx, sigKill, nil, "/")
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
	container, ok := s.containers[containerID]
	if ok {
		s.mu.RUnlock()
		s.mu.RLock()
		pod, podOk := s.pods[container.PodID]
		s.mu.RUnlock()

		if podOk {
			agentCli := agent.NewTCPClient(s.getVMIP(pod.VMID))
			if err := agentCli.Connect(ctx); err == nil {
				defer agentCli.Close()

				cleanupCmd := []string{"rm", "-rf", fmt.Sprintf("/var/lib/vyoma/containers/%s", containerID)}
				agentCli.ExecCommand(ctx, cleanupCmd, nil, "/")
			}
		}
		s.mu.Lock()
	}
	delete(s.containers, containerID)
	s.mu.Unlock()

	return &pb.RemoveContainerResponse{}, nil
}

func (s *VyomaCriServer) ContainerStatus(ctx context.Context, req *pb.ContainerStatusRequest) (*pb.ContainerStatusResponse, error) {
	containerID := req.GetContainerId()
	verbose := req.GetVerbose()

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	state := container.State
	reason := ""
	startedAt := container.StartTime
	finishedAt := int64(0)
	exitCode := int32(0)
	signal := int32(0)
	restartCount := uint32(0)

	switch state {
	case pb.ContainerState_CONTAINER_CREATED:
		reason = "Created"
	case pb.ContainerState_CONTAINER_RUNNING:
		reason = "Running"
	case pb.ContainerState_CONTAINER_EXITED:
		reason = "Exited"
		finishedAt = time.Now().Unix()
		exitCode = 0
	}

	var info map[string]string
	if verbose {
		s.mu.RLock()
		pod, ok := s.pods[container.PodID]
		s.mu.RUnlock()

		if ok {
			agentCli := agent.NewTCPClient(s.getVMIP(pod.VMID))
			if err := agentCli.Connect(ctx); err == nil {
				defer agentCli.Close()

				if metrics, err := agentCli.GetMetrics(ctx); err == nil {
					info = map[string]string{
						"mem_used_kb":    fmt.Sprintf("%d", metrics.MemUsedKb),
						"mem_total_kb":   fmt.Sprintf("%d", metrics.MemTotalKb),
						"process_count":  fmt.Sprintf("%d", metrics.ProcessCount),
					}
				}
			}
		}
	}

	return &pb.ContainerStatusResponse{
		Status: &pb.ContainerStatus{
			Id:                container.ID,
			Metadata:          &pb.ContainerMetadata{Name: container.Name},
			State:             state,
			CreatedAt:         container.Created,
			StartedAt:         startedAt,
			FinishedAt:        finishedAt,
			ExitCode:          exitCode,
			Signal:            signal,
			RestartCount:      restartCount,
			Image:             &pb.ImageSpec{Image: container.Image},
			ImageRef:          container.Image,
			Reason:            reason,
			LogPath:           fmt.Sprintf("/var/log/pods/%s/%s.log", container.PodID, container.ID),
			Labels:            container.Config.GetLabels(),
			Annotations:       container.Config.GetAnnotations(),
		},
		Info: info,
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
			if filter.State != nil && c.State != filter.State.State {
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
			Id:          c.ID,
			PodSandboxId: c.PodID,
			Metadata:    &pb.ContainerMetadata{Name: c.Name},
			Image:       c.Image,
			ImageRef:    c.Image,
			State:       c.State,
			CreatedAt:   c.Created,
			Labels:      c.Config.GetLabels(),
		})
	}

	return &pb.ListContainersResponse{Containers: containers}, nil
}

func (s *VyomaCriServer) UpdateContainerResources(ctx context.Context, req *pb.UpdateContainerResourcesRequest) (*pb.UpdateContainerResourcesResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("UpdateContainerResources: %s", containerID)

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	if req.GetLinux() != nil {
		s.logger.Printf("  linux resources: cpu=%v memory=%v", req.Linux.CPU, req.Linux.Memory)
	}

	return &pb.UpdateContainerResourcesResponse{}, nil
}

func (s *VyomaCriServer) ReopenContainerLog(ctx context.Context, req *pb.ReopenContainerLogRequest) (*pb.ReopenContainerLogResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("ReopenContainerLog: %s", containerID)

	return &pb.ReopenContainerLogResponse{}, nil
}

func (s *VyomaCriServer) getVMIP(vmID string) string {
	return "10.0.0.2"
}

func (s *VyomaCriServer) getVMIPWithLookup(ctx context.Context, vmID string) string {
	data, err := s.httpRequest(ctx, "GET", "/vms/"+vmID, nil)
	if err != nil {
		return "10.0.0.2"
	}

	var vmInfo struct {
		IP string `json:"ip"`
	}
	if json.Unmarshal(data, &vmInfo) == nil && vmInfo.IP != "" {
		return vmInfo.IP
	}

	return "10.0.0.2"
}

type containerCreateRequest struct {
	ContainerID string                 `json:"container_id"`
	Image       string                 `json:"image"`
	Cmd         []string               `json:"cmd"`
	Env         []string               `json:"env"`
	Workdir     string                 `json:"workdir"`
	Mounts      []*containerMount      `json:"mounts"`
}

type containerMount struct {
	HostPath      string `json:"host_path"`
	ContainerPath string `json:"container_path"`
	Readonly      bool   `json:"readonly"`
}

type containerCreateResponse struct {
	ContainerID string `json:"container_id"`
	PID         uint32 `json:"pid"`
}