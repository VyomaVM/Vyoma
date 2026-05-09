package cri

import (
	"context"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"sync"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/vyoma/vk8s/pkg/vyoma/client"
)

const (
	SocketPath         = "/var/run/vyoma-cri.sock"
	defaultVyomadGRPC  = "localhost:7071"
	defaultVyomadHTTP  = "http://localhost:8080"
	defaultHTTPTimeout = 10 * time.Minute
)

type VyomaCriServer struct {
	pb.UnimplementedRuntimeServiceServer
	pb.UnimplementedImageServiceServer

	logger            *log.Logger
	grpcClient        *client.Client
	httpClient        *http.Client
	vyomadHTTPAddr    string
	pods              map[string]*PodSandbox
	mu                sync.RWMutex
	containers        map[string]*ContainerInfo
	tokens            sync.Map
	streamManager     *streamManager
	streamServer      *http.Server
}

type PodSandbox struct {
	ID          string
	Name        string
	Namespace   string
	UID         string
	State       pb.PodSandboxState
	VMID        string
	IP          string
	Created     int64
	Labels      map[string]string
	Annotations map[string]string
}

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

func NewVyomaCriServer(vyomaGRPCAddr, vyomaHTTPAddr string) (*VyomaCriServer, error) {
	if vyomaGRPCAddr == "" {
		vyomaGRPCAddr = defaultVyomadGRPC
	}
	if vyomaHTTPAddr == "" {
		vyomaHTTPAddr = defaultVyomadHTTP
	}

	grpcClient, err := client.NewClient(vyomaGRPCAddr)
	if err != nil {
		return nil, fmt.Errorf("create gRPC client: %w", err)
	}

	return &VyomaCriServer{
		logger:         log.New(os.Stdout, "[vk8s] ", log.LstdFlags),
		grpcClient:     grpcClient,
		httpClient:     &http.Client{Timeout: defaultHTTPTimeout},
		vyomadHTTPAddr: vyomaHTTPAddr,
		pods:           make(map[string]*PodSandbox),
		containers:     make(map[string]*ContainerInfo),
		tokens:         sync.Map{},
	}, nil
}

func (s *VyomaCriServer) Run(ctx context.Context) error {
	if err := os.RemoveAll(SocketPath); err != nil {
		s.logger.Printf("warn: remove socket: %v", err)
	}

	lis, err := net.Listen("unix", SocketPath)
	if err != nil {
		return fmt.Errorf("listen %s: %w", SocketPath, err)
	}
	if err := os.Chmod(SocketPath, 0666); err != nil {
		s.logger.Printf("warn: chmod socket: %v", err)
	}

	s.logger.Printf("gRPC server listening on %s", SocketPath)

	grpcServer := grpc.NewServer(
		grpc.UnaryInterceptor(s.unaryLogger),
		grpc.StreamInterceptor(s.streamLogger),
	)
	pb.RegisterRuntimeServiceServer(grpcServer, s)
	pb.RegisterImageServiceServer(grpcServer, s)

	go func() {
		<-ctx.Done()
		s.logger.Println("shutting down gRPC server")
		grpcServer.GracefulStop()
	}()

	return grpcServer.Serve(lis)
}

func (s *VyomaCriServer) Close() error {
	if s.streamServer != nil {
		s.streamServer.Close()
	}
	return s.grpcClient.Close()
}

func (s *VyomaCriServer) unaryLogger(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
	s.logger.Printf("gRPC [unary] %s", info.FullMethod)
	resp, err := handler(ctx, req)
	if err != nil {
		s.logger.Printf("gRPC [error] %s: %v", info.FullMethod, err)
	}
	return resp, err
}

func (s *VyomaCriServer) streamLogger(srv interface{}, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.StreamHandler) error {
	s.logger.Printf("gRPC [stream] %s", info.FullMethod)
	return handler(srv, ss)
}

func (s *VyomaCriServer) logError(ctx context.Context, method string, err error) {
	if err != nil {
		s.logger.Printf("ERROR %s: %v", method, err)
	}
}

func errorf(c codes.Code, format string, args ...interface{}) error {
	return status.Errorf(c, format, args...)
}

type streamManager struct {
	tokens   sync.Map
	vmIPs    map[string]string
	mu       sync.RWMutex
	logger   *log.Logger
}

type streamToken struct {
	Token       string
	Type        string
	ContainerID string
	PodID       string
	VMID        string
	VMIP        string
	Command     []string
	Tty         bool
	Stdin       bool
	Stdout      bool
	Stderr      bool
	Ports       []int32
	Created     time.Time
	Expires     time.Time
}

func newStreamManager() *streamManager {
	return &streamManager{
		vmIPs:  make(map[string]string),
		logger: log.New(os.Stdout, "[streaming] ", log.LstdFlags),
	}
}

func (sm *streamManager) getVMIP(vmID string) string {
	sm.mu.RLock()
	defer sm.mu.RUnlock()
	if ip, ok := sm.vmIPs[vmID]; ok {
		return ip
	}
	return "10.0.0.2"
}

func (sm *streamManager) setVMIP(vmID, ip string) {
	sm.mu.Lock()
	defer sm.mu.Unlock()
	sm.vmIPs[vmID] = ip
}