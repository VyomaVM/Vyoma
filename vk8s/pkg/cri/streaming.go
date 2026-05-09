package cri

import (
	"context"
	"fmt"
	"io"
	"net/http"
	"net/url"
	"sync"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
)

const (
	streamingPort = 9000
)

type streamingServer struct {
	server  *http.Server
	tokens  sync.Map
	timeout int64
}

type execRequest struct {
	ContainerID string
	ExecID      string
	Cmd         []string
	Stdin       bool
	Stdout      bool
	Stderr      bool
	Tty         bool
}

func (s *VyomaCriServer) Exec(ctx context.Context, req *pb.ExecRequest) (*pb.ExecResponse, error) {
	execReq := &execRequest{
		ContainerID: req.GetContainerId(),
		ExecID:      fmt.Sprintf("exec-%s-%d", req.GetContainerId(), ctx.Value("request-id")),
		Cmd:         req.GetCmd(),
		Stdin:       req.GetStdin(),
		Stdout:      req.GetStdout(),
		Stderr:      req.GetStderr(),
		Tty:         req.GetTty(),
	}

	token := fmt.Sprintf("%s-%d", execReq.ExecID, ctx.Value("request-id"))
	s.tokens.Store(token, execReq)

	u := url.URL{
		Scheme: "http",
		Host:   fmt.Sprintf("localhost:%d", streamingPort),
		Path:   fmt.Sprintf("/exec/%s", token),
	}

	return &pb.ExecResponse{
		Url: u.String(),
	}, nil
}

func (s *VyomaCriServer) Attach(ctx context.Context, req *pb.AttachRequest) (*pb.AttachResponse, error) {
	attachReq := map[string]interface{}{
		"container_id": req.GetContainerId(),
		"stdin":        req.GetStdin(),
		"stdout":       req.GetStdout(),
		"stderr":       req.GetStderr(),
		"tty":          req.GetTty(),
	}

	token := fmt.Sprintf("attach-%s-%d", req.GetContainerId(), ctx.Value("request-id"))
	s.tokens.Store(token, attachReq)

	u := url.URL{
		Scheme: "http",
		Host:   fmt.Sprintf("localhost:%d", streamingPort),
		Path:   fmt.Sprintf("/attach/%s", token),
	}

	return &pb.AttachResponse{
		Url: u.String(),
	}, nil
}

func (s *VyomaCriServer) PortForward(ctx context.Context, req *pb.PortForwardRequest) (*pb.PortForwardResponse, error) {
	pfReq := map[string]interface{}{
		"pod_sandbox_id": req.GetPodSandboxId(),
		"port":           req.GetPort(),
	}

	token := fmt.Sprintf("pf-%s-%d", req.GetPodSandboxId(), ctx.Value("request-id"))
	s.tokens.Store(token, pfReq)

	u := url.URL{
		Scheme: "http",
		Host:   fmt.Sprintf("localhost:%d", streamingPort),
		Path:   fmt.Sprintf("/portforward/%s", token),
	}

	return &pb.PortForwardResponse{
		Url: u.String(),
	}, nil
}

func (s *VyomaCriServer) StartStreamingServer(addr string) error {
	mux := http.NewServeMux()
	mux.HandleFunc("/exec/", s.handleExec)
	mux.HandleFunc("/attach/", s.handleAttach)
	mux.HandleFunc("/portforward/", s.handlePortForward)

	s.streamingServer = &http.Server{
		Addr:    addr,
		Handler: mux,
	}

	return s.streamingServer.ListenAndServe()
}

func (s *VyomaCriServer) handleExec(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Path[len("/exec/"):]

	val, ok := s.tokens.Load(token)
	if !ok {
		http.Error(w, "exec request not found", http.StatusNotFound)
		return
	}

	execReq := val.(*execRequest)

	if execReq.Tty {
		io.Copy(w, r.Body)
	} else {
		io.Copy(w, r.Body)
	}
}

func (s *VyomaCriServer) handleAttach(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Path[len("/attach/"):]

	_, ok := s.tokens.Load(token)
	if !ok {
		http.Error(w, "attach request not found", http.StatusNotFound)
		return
	}

	io.Copy(w, r.Body)
}

func (s *VyomaCriServer) handlePortForward(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Path[len("/portforward/"):]

	val, ok := s.tokens.Load(token)
	if !ok {
		http.Error(w, "port forward request not found", http.StatusNotFound)
		return
	}

	_ = val
	w.WriteHeader(http.StatusOK)
}

func (s *VyomaCriServer) StopStreamingServer() error {
	if s.streamingServer != nil {
		return s.streamingServer.Close()
	}
	return nil
}

func (s *VyomaCriServer) handleResize(ctx context.Context, req *pb.ResizePtyRequest) (*pb.ResizePtyResponse, error) {
	return &pb.ResizePtyResponse{}, nil
}