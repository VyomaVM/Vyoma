package cri

import (
	"context"
	"fmt"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"

	vyomav1 "github.com/vyoma/vk8s/pkg/vyoma/proto"
)

func (s *VyomaCriServer) RunPodSandbox(ctx context.Context, req *pb.RunPodSandboxRequest) (*pb.RunPodSandboxResponse, error) {
	config := req.GetConfig()
	metadata := config.GetMetadata()

	vmReq := &vyomav1.CreateVmRequest{
		Name:      fmt.Sprintf("pod-%s", metadata.GetName()),
		Vcpus:     2,
		MemoryMb:  2048,
		Networks: []string{},
	}

	if config.GetLinux() != nil {
		resources := config.GetLinux().GetResources()
		if resources != nil {
			if cpuQuota := resources.GetCpuQuota(); cpuQuota > 0 {
				vmReq.Vcpus = uint32(cpuQuota / 100000)
			}
			if memLimit := resources.GetMemoryLimitInBytes(); memLimit > 0 {
				vmReq.MemoryMb = memLimit / 1024 / 1024
			}
		}
	}

	vmResp, err := s.client.CreateVm(ctx, vmReq)
	if err != nil {
		return nil, fmt.Errorf("failed to create VM: %w", err)
	}

	vmID := vmResp.GetVmId()

	if err := s.client.StartVm(ctx, &vyomav1.VmIdRequest{VmId: vmID}); err != nil {
		return nil, fmt.Errorf("failed to start VM: %w", err)
	}

	podID := vmID

	s.mu.Lock()
	s.pods[podID] = &PodSandbox{
		ID:        podID,
		Name:      metadata.GetName(),
		Namespace: metadata.GetNamespace(),
		UID:       metadata.GetUid(),
		State:     pb.PodSandboxState_SANDBOX_READY,
		VMID:      vmID,
		Created:   0,
		Labels:    config.GetLabels(),
	}
	s.mu.Unlock()

	return &pb.RunPodSandboxResponse{PodSandboxId: podID}, nil
}

func (s *VyomaCriServer) StopPodSandbox(ctx context.Context, req *pb.StopPodSandboxRequest) (*pb.StopPodSandboxResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	if err := s.client.StopVm(ctx, &vyomav1.VmIdRequest{VmId: pod.VMID}); err != nil {
		return nil, fmt.Errorf("failed to stop VM: %w", err)
	}

	s.mu.Lock()
	pod.State = pb.PodSandboxState_SANDBOX_NOTREADY
	s.mu.Unlock()

	return &pb.StopPodSandboxResponse{}, nil
}

func (s *VyomaCriServer) RemovePodSandbox(ctx context.Context, req *pb.RemovePodSandboxRequest) (*pb.RemovePodSandboxResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.Lock()
	defer s.mu.Unlock()

	pod, ok := s.pods[podID]
	if ok {
		if _, err := s.client.DeleteVm(ctx, &vyomav1.VmIdRequest{VmId: pod.VMID}); err != nil {
			return nil, fmt.Errorf("failed to delete VM: %w", err)
		}
		delete(s.pods, podID)
	}

	return &pb.RemovePodSandboxResponse{}, nil
}

func (s *VyomaCriServer) PodSandboxStatus(ctx context.Context, req *pb.PodSandboxStatusRequest) (*pb.PodSandboxStatusResponse, error) {
	podID := req.GetPodSandboxId()

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, fmt.Errorf("pod not found: %s", podID)
	}

	annotations := pod.Labels

	return &pb.PodSandboxStatusResponse{
		Status: &pb.PodSandboxStatus{
			Id:                pod.ID,
			Metadata:          &pb.PodSandboxMetadata{Name: pod.Name, Namespace: pod.Namespace, Uid: pod.UID},
			State:             pod.State,
			CreatedAt:         pod.Created,
			Labels:            pod.Labels,
			Annotations:       annotations,
			Linux:             &pb.LinuxPodSandboxStatus{},
		},
	}, nil
}

func (s *VyomaCriServer) ListPodSandbox(ctx context.Context, req *pb.ListPodSandboxRequest) (*pb.ListPodSandboxResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	filter := req.GetFilter()
	var pods []*pb.PodSandbox

	for _, pod := range s.pods {
		if filter != nil {
			if filter.Id != "" && pod.ID != filter.Id {
				continue
			}
			if filter.State != nil && pod.State != filter.State.State {
				continue
			}
			if len(filter.LabelSelectors) > 0 {
				match := true
				for _, selector := range filter.LabelSelectors {
					if val, ok := pod.Labels[selector]; !ok || val == "" {
						match = false
						break
					}
				}
				if !match {
					continue
				}
			}
		}

		pods = append(pods, &pb.PodSandbox{
			Id:        pod.ID,
			Metadata:  &pb.PodSandboxMetadata{Name: pod.Name, Namespace: pod.Namespace, Uid: pod.UID},
			State:     pod.State,
			CreatedAt: pod.Created,
			Labels:    pod.Labels,
		})
	}

	if pods == nil {
		pods = []*pb.PodSandbox{}
	}

	return &pb.ListPodSandboxResponse{Items: pods}, nil
}

func (s *VyomaCriServer) UpdateRuntimeConfig(ctx context.Context, req *pb.UpdateRuntimeConfigRequest) (*pb.UpdateRuntimeConfigResponse, error) {
	return &pb.UpdateRuntimeConfigResponse{}, nil
}

func (s *VyomaCriServer) Status(ctx context.Context, req *pb.StatusRequest) (*pb.StatusResponse, error) {
	return &pb.StatusResponse{
		Enabled:    true,
		ApiVersion: "v1",
	}, nil
}