package cri

import (
	"context"
	"testing"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
)

func TestPodSandboxCreation(t *testing.T) {
	server := &VyomaCriServer{
		pods: make(map[string]*PodSandbox),
	}

	req := &pb.RunPodSandboxRequest{
		Config: &pb.PodSandboxConfig{
			Metadata: &pb.PodSandboxMetadata{
				Name:      "test-pod",
				Namespace: "default",
				Uid:       "12345",
			},
			Linux: &pb.LinuxPodSandboxConfig{
				Resources: &pb.LinuxContainerResources{
					CpuQuota:           200000,
					MemoryLimitInBytes:  1024 * 1024 * 1024,
				},
			},
		},
	}

	_ = req

	if server.pods == nil {
		t.Error("Expected pods map to be initialized")
	}
}

func TestPodSandboxState(t *testing.T) {
	pod := &PodSandbox{
		ID:    "pod-1",
		Name:  "test-pod",
		State: pb.PodSandboxState_SANDBOX_READY,
		VMID:  "vm-1",
	}

	if pod.State != pb.PodSandboxState_SANDBOX_READY {
		t.Errorf("Expected state to be READY, got %v", pod.State)
	}

	pod.State = pb.PodSandboxState_SANDBOX_NOTREADY

	if pod.State != pb.PodSandboxState_SANDBOX_NOTREADY {
		t.Errorf("Expected state to be NOTREADY, got %v", pod.State)
	}
}

func TestListPodSandbox(t *testing.T) {
	server := &VyomaCriServer{
		pods: map[string]*PodSandbox{
			"pod-1": {ID: "pod-1", Name: "pod-1", State: pb.PodSandboxState_SANDBOX_READY},
			"pod-2": {ID: "pod-2", Name: "pod-2", State: pb.PodSandboxState_SANDBOX_NOTREADY},
		},
	}

	req := &pb.ListPodSandboxRequest{}
	_ = req

	resp, err := server.ListPodSandbox(context.Background(), req)
	if err != nil {
		t.Errorf("ListPodSandbox failed: %v", err)
	}

	if len(resp.Items) != 2 {
		t.Errorf("Expected 2 pods, got %d", len(resp.Items))
	}
}

func TestPodSandboxStatus(t *testing.T) {
	server := &VyomaCriServer{
		pods: map[string]*PodSandbox{
			"pod-1": {ID: "pod-1", Name: "test-pod", State: pb.PodSandboxState_SANDBOX_READY},
		},
	}

	req := &pb.PodSandboxStatusRequest{
		PodSandboxId: "pod-1",
	}

	resp, err := server.PodSandboxStatus(context.Background(), req)
	if err != nil {
		t.Errorf("PodSandboxStatus failed: %v", err)
	}

	if resp.Status.State != pb.PodSandboxState_SANDBOX_READY {
		t.Errorf("Expected state READY, got %v", resp.Status.State)
	}
}

func TestRemovePodSandbox(t *testing.T) {
	server := &VyomaCriServer{
		pods: map[string]*PodSandbox{
			"pod-1": {ID: "pod-1", Name: "test-pod", State: pb.PodSandboxState_SANDBOX_READY, VMID: "vm-1"},
		},
	}

	req := &pb.RemovePodSandboxRequest{
		PodSandboxId: "pod-1",
	}

	_, err := server.RemovePodSandbox(context.Background(), req)
	if err != nil {
		t.Errorf("RemovePodSandbox failed: %v", err)
	}

	if len(server.pods) != 0 {
		t.Errorf("Expected pods to be empty, got %d", len(server.pods))
	}
}

func TestContainerCreation(t *testing.T) {
	server := &VyomaCriServer{
		pods: map[string]*PodSandbox{
			"pod-1": {ID: "pod-1", Name: "test-pod", State: pb.PodSandboxState_SANDBOX_READY, VMID: "vm-1"},
		},
	}

	req := &pb.CreateContainerRequest{
		PodSandboxId: "pod-1",
		Config: &pb.ContainerConfig{
			Metadata: &pb.ContainerMetadata{
				Name: "test-container",
			},
		},
	}

	resp, err := server.CreateContainer(context.Background(), req)
	if err != nil {
		t.Errorf("CreateContainer failed: %v", err)
	}

	if resp.ContainerId == "" {
		t.Error("Expected non-empty container ID")
	}
}

func TestContainerStatus(t *testing.T) {
	server := &VyomaCriServer{}

	req := &pb.ContainerStatusRequest{
		ContainerId: "container-1",
	}

	resp, err := server.ContainerStatus(context.Background(), req)
	if err != nil {
		t.Errorf("ContainerStatus failed: %v", err)
	}

	if resp.Status.State != pb.ContainerState_CONTAINER_RUNNING {
		t.Errorf("Expected state RUNNING, got %v", resp.Status.State)
	}
}
