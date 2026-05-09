package cri

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"math/rand"
	"net/http"
	"strings"
	"sync"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
	"google.golang.org/grpc/codes"
)

const (
	streamTimeout = 4 * time.Hour
)

type streamingToken struct {
	Token       string
	Type        string
	ContainerID string
	PodID       string
	Command     []string
	Tty         bool
	Stdin       bool
	Stdout      bool
	Stderr      bool
	Ports       []int32
	Created     time.Time
}

type streamingServer struct {
	server     *http.Server
	manager    *VyomaCriServer
	tokenStore sync.Map
}

func generateToken() string {
	const letters = "abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ0123456789"
	b := make([]byte, 32)
	for i := range b {
		b[i] = letters[rand.Intn(len(letters))]
	}
	return string(b)
}

func (ss *streamingServer) getToken(path string) *streamingToken {
	parts := strings.SplitN(strings.TrimPrefix(path, "/"), "/", 2)
	if len(parts) < 2 {
		return nil
	}
	token := parts[1]

	val, ok := ss.manager.tokens.Load(token)
	if !ok {
		return nil
	}

	st, ok := val.(*streamingToken)
	if !ok {
		return nil
	}

	if time.Since(st.Created) > streamTimeout {
		ss.manager.tokens.Delete(token)
		return nil
	}

	return st
}

func (ss *streamingServer) handleExec(w http.ResponseWriter, r *http.Request) {
	st := ss.getToken(r.URL.Path)
	if st == nil {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	if r.Method == http.MethodGet {
		ss.handleExecStream(w, r, st)
		return
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "read body", http.StatusBadRequest)
		return
	}

	resp, err := ss.manager.execInVM(r.Context(), st.ContainerID, st.Command)
	if err != nil {
		w.WriteHeader(http.StatusInternalServerError)
		json.NewEncoder(w).Encode(map[string]string{"error": err.Error()})
		return
	}

	json.NewEncoder(w).Encode(resp)
}

func (ss *streamingServer) handleExecStream(w http.ResponseWriter, r *http.Request, st *streamingToken) {
	if st.Tty {
		w.Header().Set("Content-Type", "application/octet-stream")
		io.Copy(w, r.Body)
		return
	}

	w.Header().Set("Content-Type", "application/json")
	resp, _ := json.Marshal(map[string]interface{}{
		"stdout":   "",
		"stderr":   "",
		"exitCode": 0,
	})
	w.Write(resp)
}

func (ss *streamingServer) handleAttach(w http.ResponseWriter, r *http.Request) {
	st := ss.getToken(r.URL.Path)
	if st == nil {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	w.Header().Set("Content-Type", "application/octet-stream")
	if r.Method == http.MethodGet {
		io.Copy(w, r.Body)
	} else {
		io.Copy(io.Discard, r.Body)
	}
}

func (ss *streamingServer) handlePortForward(w http.ResponseWriter, r *http.Request) {
	st := ss.getToken(r.URL.Path)
	if st == nil {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	for _, port := range st.Ports {
		fmt.Fprintf(w, "forwarding port %d\n", port)
	}
}

func (s *VyomaCriServer) Exec(ctx context.Context, req *pb.ExecRequest) (*pb.ExecResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("Exec: container=%s cmd=%v tty=%v", containerID, req.GetCmd(), req.GetTty())

	if err := s.validateContainerID(containerID); err != nil {
		return nil, err
	}

	token := generateToken()
	st := &streamingToken{
		Token:       token,
		Type:        "exec",
		ContainerID: containerID,
		Command:     req.GetCmd(),
		Tty:         req.GetTty(),
		Stdin:       req.GetStdin(),
		Stdout:      req.GetStdout(),
		Stderr:      req.GetStderr(),
		Created:     time.Now(),
	}
	s.tokens.Store(token, st)

	url := fmt.Sprintf("http://localhost:%d/exec/%s", streamingPort, token)
	return &pb.ExecResponse{Url: url}, nil
}

func (s *VyomaCriServer) Attach(ctx context.Context, req *pb.AttachRequest) (*pb.AttachResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("Attach: container=%s tty=%v", containerID, req.GetTty())

	if err := s.validateContainerID(containerID); err != nil {
		return nil, err
	}

	token := generateToken()
	st := &streamingToken{
		Token:       token,
		Type:        "attach",
		ContainerID: containerID,
		Tty:         req.GetTty(),
		Stdin:       req.GetStdin(),
		Stdout:      req.GetStdout(),
		Stderr:      req.GetStderr(),
		Created:     time.Now(),
	}
	s.tokens.Store(token, st)

	url := fmt.Sprintf("http://localhost:%d/attach/%s", streamingPort, token)
	return &pb.AttachResponse{Url: url}, nil
}

func (s *VyomaCriServer) PortForward(ctx context.Context, req *pb.PortForwardRequest) (*pb.PortForwardResponse, error) {
	podID := req.GetPodSandboxId()
	ports := req.GetPort()
	s.logger.Printf("PortForward: pod=%s ports=%v", podID, ports)

	token := generateToken()
	st := &streamingToken{
		Token:       token,
		Type:        "portforward",
		PodID:       podID,
		Ports:       ports,
		Created:     time.Now(),
	}
	s.tokens.Store(token, st)

	url := fmt.Sprintf("http://localhost:%d/portforward/%s", streamingPort, token)
	return &pb.PortForwardResponse{Url: url}, nil
}

func (s *VyomaCriServer) validateContainerID(id string) error {
	if id == "" {
		return errorf(codes.InvalidArgument, "container ID is empty")
	}
	return nil
}

func (s *VyomaCriServer) handleResize(ctx context.Context, req *pb.ResizePtyRequest) (*pb.ResizePtyResponse, error) {
	s.logger.Printf("ResizePty: container=%s size=%dx%d", req.GetContainerId(), req.GetHeight(), req.GetWidth())
	return &pb.ResizePtyResponse{}, nil
}