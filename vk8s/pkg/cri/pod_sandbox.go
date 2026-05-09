package cri

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"

	vyomav1 "github.com/vyoma/vk8s/pkg/vyoma/proto"
)

func (s *VyomaCriServer) RunPodSandbox(ctx context.Context, req *pb.RunPodSandboxRequest) (*pb.RunPodSandboxResponse, error) {
	config := req.GetConfig()
	metadata := config.GetMetadata()

	s.logger.Printf("RunPodSandbox: name=%s namespace=%s", metadata.GetName(), metadata.GetNamespace())

	image := "vyoma/alpine:latest"
	if img := req.GetConfig().GetImage().GetImage(); img != "" {
		image = img
	}

	vmReq := &vyomav1.CreateVmRequest{
		Image:    image,
		Name:     fmt.Sprintf("pod-%s", metadata.GetName()),
		Vcpus:    2,
		MemoryMb: 2048,
	}

	if config.GetLinux() != nil {
		resources := config.GetLinux().GetResources()
		if resources != nil {
			if cpuQuota := resources.GetCpuQuota(); cpuQuota > 0 && cpuQuota != -1 {
				vmReq.Vcpus = uint32(cpuQuota / 100000)
				if vmReq.Vcpus < 1 {
					vmReq.Vcpus = 1
				}
			}
			if memLimit := resources.GetMemoryLimitInBytes(); memLimit > 0 {
				vmReq.MemoryMb = memLimit / 1024 / 1024
				if vmReq.MemoryMb < 128 {
					vmReq.MemoryMb = 128
				}
			}
		}
	}

	for _, port := range config.GetPortMappings() {
		vmReq.Ports = append(vmReq.Ports, &vyomav1.PortMapping{
			Host: port.GetHostPort(),
			Vm:   port.GetContainerPort(),
		})
	}

	for _, mount := range config.GetMounts() {
		vmReq.Volumes = append(vmReq.Volumes, &vyomav1.VolumeMapping{
			HostPath: mount.GetHostPath(),
			VmPath:   mount.GetContainerPath(),
		})
	}

	vmResp, err := s.grpcClient.CreateVm(ctx, vmReq)
	if err != nil {
		s.logError(ctx, "RunPodSandbox.CreateVm", err)
		return nil, errorf(codes.Internal, "create VM: %v", err)
	}

	vmID := vmResp.GetVmId()
	s.logger.Printf("VM created: %s", vmID)

	if err := s.grpcClient.StartVm(ctx, &vyomav1.VmIdRequest{VmId: vmID}); err != nil {
		s.logError(ctx, "RunPodSandbox.StartVm", err)
		s.grpcClient.DeleteVm(ctx, &vyomav1.VmIdRequest{VmId: vmID})
		return nil, errorf(codes.Internal, "start VM: %v", err)
	}
	s.logger.Printf("VM started: %s", vmID)

	podID := vmID

	s.mu.Lock()
	s.pods[podID] = &PodSandbox{
		ID:          podID,
		Name:        metadata.GetName(),
		Namespace:   metadata.GetNamespace(),
		UID:         metadata.GetUid(),
		State:       pb.PodSandboxState_SANDBOX_READY,
		VMID:        vmID,
		Created:     0,
		Labels:      config.GetLabels(),
		Annotations: config.GetAnnotations(),
	}
	s.mu.Unlock()

	return &pb.RunPodSandboxResponse{PodSandboxId: podID}, nil
}

func (s *VyomaCriServer) StopPodSandbox(ctx context.Context, req *pb.StopPodSandboxRequest) (*pb.StopPodSandboxResponse, error) {
	podID := req.GetPodSandboxId()
	s.logger.Printf("StopPodSandbox: %s", podID)

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "pod not found: %s", podID)
	}

	if err := s.grpcClient.StopVm(ctx, &vyomav1.VmIdRequest{VmId: pod.VMID}); err != nil {
		s.logError(ctx, "StopPodSandbox", err)
	}

	s.mu.Lock()
	pod.State = pb.PodSandboxState_SANDBOX_NOTREADY
	s.mu.Unlock()

	return &pb.StopPodSandboxResponse{}, nil
}

func (s *VyomaCriServer) RemovePodSandbox(ctx context.Context, req *pb.RemovePodSandboxRequest) (*pb.RemovePodSandboxResponse, error) {
	podID := req.GetPodSandboxId()
	s.logger.Printf("RemovePodSandbox: %s", podID)

	s.mu.Lock()
	defer s.mu.Unlock()

	pod, ok := s.pods[podID]
	if ok {
		if _, err := s.grpcClient.DeleteVm(ctx, &vyomav1.VmIdRequest{VmId: pod.VMID}); err != nil {
			s.logError(ctx, "RemovePodSandbox.DeleteVm", err)
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
		return nil, errorf(codes.NotFound, "pod not found: %s", podID)
	}

	linuxStatus := &pb.LinuxPodSandboxStatus{}
	if req.GetVerbose() {
		linuxStatus.Namespaces = &pb.Namespace{
			Options: &pb.NamespaceOption{},
		}
	}

	return &pb.PodSandboxStatusResponse{
		Status: &pb.PodSandboxStatus{
			Id:                pod.ID,
			Metadata:          &pb.PodSandboxMetadata{Name: pod.Name, Namespace: pod.Namespace, Uid: pod.UID},
			State:             pod.State,
			CreatedAt:         pod.Created,
			Labels:            pod.Labels,
			Annotations:       pod.Annotations,
			Linux:             linuxStatus,
		},
	}, nil
}

func (s *VyomaCriServer) ListPodSandbox(ctx context.Context, req *pb.ListPodSandboxRequest) (*pb.ListPodSandboxResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	filter := req.GetFilter()
	pods := make([]*pb.PodSandbox, 0)

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

	return &pb.ListPodSandboxResponse{Items: pods}, nil
}

func (s *VyomaCriServer) UpdateRuntimeConfig(ctx context.Context, req *pb.UpdateRuntimeConfigRequest) (*pb.UpdateRuntimeConfigResponse, error) {
	s.logger.Printf("UpdateRuntimeConfig")
	return &pb.UpdateRuntimeConfigResponse{}, nil
}

func (s *VyomaCriServer) Status(ctx context.Context, req *pb.StatusRequest) (*pb.StatusResponse, error) {
	return &pb.StatusResponse{
		Enabled:    true,
		ApiVersion: "v1",
		Conditions: []*pb.RuntimeCondition{
			{Type: "RuntimeReady", Status: true, Reason: "ok"},
			{Type: "NetworkReady", Status: true, Reason: "ok"},
		},
	}, nil
}

type psResponse struct {
	Vms []struct {
		ID     string            `json:"id"`
		Status string            `json:"status"`
		Labels map[string]string `json:"labels"`
	} `json:"vms"`
}

func (s *VyomaCriServer) syncPodsFromVyomad(ctx context.Context) error {
	req, err := http.NewRequestWithContext(ctx, "GET", s.vyomadHTTPAddr+"/ps", nil)
	if err != nil {
		return err
	}

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return err
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return fmt.Errorf("vyomad /ps: %d", resp.StatusCode)
	}

	var result psResponse
	if err := json.NewDecoder(resp.Body).Decode(&result); err != nil {
		return err
	}

	s.mu.Lock()
	defer s.mu.Unlock()

	for _, vm := range result.Vms {
		podName := vm.Labels["k8s.io/pod-name"]
		if podName == "" {
			continue
		}

		state := pb.PodSandboxState_SANDBOX_NOTREADY
		if vm.Status == "Running" {
			state = pb.PodSandboxState_SANDBOX_READY
		}

		s.pods[vm.ID] = &PodSandbox{
			ID:        vm.ID,
			Name:      podName,
			Namespace: vm.Labels["k8s.io/pod-namespace"],
			UID:       vm.Labels["k8s.io/pod-uid"],
			State:     state,
			VMID:      vm.ID,
			Labels:    vm.Labels,
		}
	}

	return nil
}

type httpRequest struct {
	Method string
	Path   string
	Body   interface{}
}

func (s *VyomaCriServer) httpRequest(ctx context.Context, method, path string, body interface{}) ([]byte, error) {
	var bodyReader *bytes.Reader
	if body != nil {
		data, err := json.Marshal(body)
		if err != nil {
			return nil, err
		}
		bodyReader = bytes.NewReader(data)
	} else {
		bodyReader = bytes.NewReader(nil)
	}

	req, err := http.NewRequestWithContext(ctx, method, s.vyomadHTTPAddr+path, bodyReader)
	if err != nil {
		return nil, err
	}
	req.Header.Set("Content-Type", "application/json")

	resp, err := s.httpClient.Do(req)
	if err != nil {
		return nil, err
	}
	defer resp.Body.Close()

	data, err := io.ReadAll(resp.Body)
	if err != nil {
		return nil, err
	}

	if resp.StatusCode >= 400 {
		return nil, fmt.Errorf("vyomad %s %s: %d - %s", method, path, resp.StatusCode, string(data))
	}

	return data, nil
}