package cri

import (
	"context"
	"fmt"
	"net"
	"net/http"
	"sync"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc"

	vyomav1 "github.com/vyoma/vk8s/pkg/vyoma/proto"
	"github.com/vyoma/vk8s/pkg/vyoma/client"
)

const (
	SocketPath = "/var/run/vyoma-cri.sock"
)

type VyomaCriServer struct {
	pb.UnimplementedRuntimeServiceServer
	pb.UnimplementedImageServiceServer

	client          *client.Client
	pods            map[string]*PodSandbox
	mu              sync.RWMutex
	imageStore      map[string]*ImageInfo
	containers      map[string]*ContainerInfo
	tokens          sync.Map
	streamingServer *http.Server
}

type PodSandbox struct {
	ID        string
	Name      string
	Namespace string
	UID       string
	State     pb.PodSandboxState
	VMID      string
	Created   int64
	Labels    map[string]string
}

type ImageInfo struct {
	ID       string
	RepoTags []string
	Size     uint64
	Created  int64
}

type ContainerInfo struct {
	ID      string
	PodID   string
	Name    string
	Image   string
	Created int64
	State   pb.ContainerState
}

func NewVyomaCriServer(vyomaAddr string) (*VyomaCriServer, error) {
	c, err := client.NewClient(vyomaAddr)
	if err != nil {
		return nil, fmt.Errorf("failed to create vyoma client: %w", err)
	}

	return &VyomaCriServer{
		client:      c,
		pods:        make(map[string]*PodSandbox),
		imageStore:  make(map[string]*ImageInfo),
		containers:  make(map[string]*ContainerInfo),
		tokens:      sync.Map{},
	}, nil
}

func (s *VyomaCriServer) Run(ctx context.Context) error {
	lis, err := net.Listen("unix", SocketPath)
	if err != nil {
		return fmt.Errorf("failed to listen: %w", err)
	}

	grpcServer := grpc.NewServer()
	pb.RegisterRuntimeServiceServer(grpcServer, s)
	pb.RegisterImageServiceServer(grpcServer, s)

	go func() {
		<-ctx.Done()
		grpcServer.GracefulStop()
	}()

	return grpcServer.Serve(lis)
}

func (s *VyomaCriServer) Close() error {
	return s.client.Close()
}