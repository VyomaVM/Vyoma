package cri

import (
	"context"
	"fmt"
	"log"
	"net"
	"net/http"
	"os"
	"sync"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc"
	"google.golang.org/grpc/codes"
	"google.golang.org/grpc/status"

	"github.com/vyoma/vk8s/pkg/vyoma/client"
)

const (
	SocketPath       = "/var/run/vyoma-cri.sock"
	defaultVyomadGRPC = "localhost:7071"
	defaultVyomadHTTP = "http://localhost:8080"
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
	streamingServer   *http.Server
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
		httpClient:     &http.Client{Timeout: 10 * 60 * 1000000000},
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
	if s.streamingServer != nil {
		s.streamingServer.Close()
	}
	return s.grpcClient.Close()
}

func (s *VyomaCriServer) StartStreamingServer() error {
	ss := &streamingServer{manager: s}
	mux := http.NewServeMux()
	mux.HandleFunc("/exec/", ss.handleExec)
	mux.HandleFunc("/attach/", ss.handleAttach)
	mux.HandleFunc("/portforward/", ss.handlePortForward)

	addr := fmt.Sprintf(":%d", streamingPort)
	ss.server = &http.Server{
		Addr:    addr,
		Handler: mux,
	}

	go func() {
		if err := ss.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			s.logger.Printf("streaming server error: %v", err)
		}
	}()

	s.streamingServer = ss.server
	s.logger.Printf("streaming server started on %s", addr)
	return nil
}

func (s *VyomaCriServer) unaryLogger(ctx context.Context, req interface{}, info *grpc.UnaryServerInfo, handler grpc.UnaryHandler) (interface{}, error) {
	s.logger.Printf("gRPC [unary] %s", info.FullMethod)
	resp, err := handler(ctx, req)
	if err != nil {
		s.logger.Printf("gRPC [error] %s: %v", info.FullMethod, err)
	}
	return resp, err
}

func (s *VyomaCriServer) streamLogger(srv interface{}, ss grpc.ServerStream, info *grpc.StreamServerInfo, handler grpc.ServerStreamHandler) error {
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