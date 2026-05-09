package cri

import (
	"bytes"
	"context"
	"crypto/rand"
	"encoding/hex"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"net/http"
	"strings"
	"sync"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"

	"github.com/vyoma/vk8s/pkg/agent"
)

const streamingPort = 9000

type streamingToken struct {
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
	Width       uint32
	Height      uint32
	Created     time.Time
	Expires     time.Time
}

type streamingHandler struct {
	manager *streamManager
	server  *http.Server
}

func (s *VyomaCriServer) initStreamManager() {
	s.streamManager = newStreamManager()
}

func (s *VyomaCriServer) StartStreamingServer() error {
	s.initStreamManager()

	h := &streamingHandler{manager: s.streamManager}
	mux := http.NewServeMux()
	mux.HandleFunc("/exec/", h.handleExec)
	mux.HandleFunc("/attach/", h.handleAttach)
	mux.HandleFunc("/portforward/", h.handlePortForward)

	addr := fmt.Sprintf(":%d", streamingPort)
	h.server = &http.Server{
		Addr:    addr,
		Handler: mux,
	}

	go func() {
		s.logger.Printf("streaming server starting on %s", addr)
		if err := h.server.ListenAndServe(); err != nil && err != http.ErrServerClosed {
			s.logger.Printf("streaming server error: %v", err)
		}
	}()

	s.streamServer = h.server
	s.logger.Printf("streaming server started on %s", addr)
	return nil
}

func (s *VyomaCriServer) StopStreamingServer() error {
	if s.streamServer != nil {
		return s.streamServer.Close()
	}
	return nil
}

func (s *VyomaCriServer) generateStreamToken(t *streamingToken) string {
	if t.Token == "" {
		bytes := make([]byte, 16)
		rand.Read(bytes)
		t.Token = hex.EncodeToString(bytes)
	}
	t.Created = time.Now()
	t.Expires = t.Created.Add(4 * time.Hour)

	s.tokens.Store(t.Token, t)
	go func() {
		<-time.After(time.Until(t.Expires))
		s.tokens.Delete(t.Token)
	}()

	return t.Token
}

func (s *VyomaCriServer) getStreamToken(token string) (*streamingToken, bool) {
	val, ok := s.tokens.Load(token)
	if !ok {
		return nil, false
	}
	st, ok := val.(*streamingToken)
	if !ok {
		return nil, false
	}
	if time.Now().After(st.Expires) {
		s.tokens.Delete(token)
		return nil, false
	}
	return st, true
}

func (h *streamingHandler) handleExec(w http.ResponseWriter, r *http.Request) {
	token := strings.TrimPrefix(r.URL.Path, "/exec/")

	st, ok := h.manager.getToken(token)
	if !ok {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	if st.Type != "exec" {
		http.Error(w, "invalid token type", http.StatusBadRequest)
		return
	}

	cli := agent.NewTCPClient(st.VMIP)
	ctx, cancel := context.WithTimeout(r.Context(), 5*time.Minute)
	defer cancel()

	if err := cli.Connect(ctx); err != nil {
		h.manager.logger.Printf("exec connect error: %v", err)
		http.Error(w, fmt.Sprintf("connect to VM: %v", err), http.StatusInternalServerError)
		return
	}
	defer cli.Close()

	if r.Method == http.MethodGet {
		h.execStream(w, r, cli, st)
		return
	}

	body, err := io.ReadAll(r.Body)
	if err != nil {
		http.Error(w, "read body", http.StatusBadRequest)
		return
	}

	var req struct {
		Value string `json:"value"`
	}
	json.Unmarshal(body, &req)

	cmd := st.Command
	if req.Value != "" {
		cmd = []string{"/bin/sh", "-c", req.Value}
	}

	stdout, stderr, exitCode, err := cli.ExecCommand(ctx, cmd, nil, "")
	if err != nil {
		h.manager.logger.Printf("exec error: %v", err)
		http.Error(w, err.Error(), http.StatusInternalServerError)
		return
	}

	h.manager.logger.Printf("exec done: exit=%d", exitCode)

	w.Header().Set("Content-Type", "application/json")
	json.NewEncoder(w).Encode(map[string]interface{}{
		"exitCode": exitCode,
		"stdout":   string(stdout),
		"stderr":   string(stderr),
	})
}

func (h *streamingHandler) execStream(w http.ResponseWriter, r *http.Request, cli *agent.Client, st *streamingToken) {
	h.setupHijack(w)

	if st.Tty {
		h.ttyStream(w, r, cli, st)
		return
	}

	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		io.Copy(w, r.Body)
	}()

	go func() {
		defer wg.Done()
		stdout, _, _ := cli.ExecCommand(r.Context(), st.Command, nil, "")
		w.Write(stdout)
	}()

	wg.Wait()
}

func (h *streamingHandler) ttyStream(w http.ResponseWriter, r *http.Request, cli *agent.Client, st *streamingToken) {
	conn, _, err := w.(http.Hijacker).Hijack()
	if err != nil {
		h.manager.logger.Printf("hijack error: %v", err)
		return
	}
	defer conn.Close()

	if st.Width > 0 && st.Height > 0 {
		resize := []string{"resize", "-s", fmt.Sprintf("%d", st.Height), fmt.Sprintf("%d", st.Width)}
		cli.ExecCommand(r.Context(), resize, nil, "")
	}

	ctx := r.Context()
	stdinDone := make(chan struct{})

	go func() {
		io.Copy(conn, r.Body)
		close(stdinDone)
	}()

	for {
		select {
		case <-stdinDone:
			return
		case <-ctx.Done():
			return
		case <-time.After(100 * time.Millisecond):
		}
	}
}

func (h *streamingHandler) handleAttach(w http.ResponseWriter, r *http.Request) {
	token := strings.TrimPrefix(r.URL.Path, "/attach/")

	st, ok := h.manager.getToken(token)
	if !ok {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	if st.Type != "attach" {
		http.Error(w, "invalid token type", http.StatusBadRequest)
		return
	}

	cli := agent.NewTCPClient(st.VMIP)
	ctx := r.Context()

	if err := cli.Connect(ctx); err != nil {
		h.manager.logger.Printf("attach connect error: %v", err)
		http.Error(w, fmt.Sprintf("connect to VM: %v", err), http.StatusInternalServerError)
		return
	}
	defer cli.Close()

	h.setupHijack(w)

	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		io.Copy(cli, r.Body)
	}()

	go func() {
		defer wg.Done()
		stdout, _, _ := cli.ExecCommand(ctx, []string{"cat", "/dev/console"}, nil, "")
		w.Write(stdout)
	}()

	wg.Wait()
}

func (h *streamingHandler) handlePortForward(w http.ResponseWriter, r *http.Request) {
	token := strings.TrimPrefix(r.URL.Path, "/portforward/")

	st, ok := h.manager.getToken(token)
	if !ok {
		http.Error(w, "invalid or expired token", http.StatusUnauthorized)
		return
	}

	if st.Type != "portforward" {
		http.Error(w, "invalid token type", http.StatusBadRequest)
		return
	}

	h.manager.logger.Printf("portforward: pod=%s ports=%v", st.PodID, st.Ports)

	if r.Method == http.MethodGet {
		w.Header().Set("Content-Type", "application/json")
		json.NewEncoder(w).Encode(map[string]interface{}{"ports": st.Ports})
		return
	}

	h.setupHijack(w)

	var wg sync.WaitGroup
	for _, port := range st.Ports {
		port := port
		wg.Add(1)
		go func() {
			defer wg.Done()
			h.forwardPort(r.Context(), w, st.VMIP, int(port))
		}()
	}
	wg.Wait()
}

func (h *streamingHandler) forwardPort(ctx context.Context, w http.ResponseWriter, vmIP string, port int) {
	dialer := net.Dialer{Timeout: 5 * time.Second}

	vmConn, err := dialer.DialContext(ctx, "tcp", fmt.Sprintf("%s:%d", vmIP, port))
	if err != nil {
		h.manager.logger.Printf("portforward dial %s:%d: %v", vmIP, port, err)
		return
	}
	defer vmConn.Close()

	hijacker, ok := w.(http.Hijacker)
	if !ok {
		return
	}

	clientConn, _, err := hijacker.Hijack()
	if err != nil {
		return
	}
	defer clientConn.Close()

	var wg sync.WaitGroup
	wg.Add(2)

	go func() {
		defer wg.Done()
		io.Copy(vmConn, clientConn)
	}()

	go func() {
		defer wg.Done()
		io.Copy(clientConn, vmConn)
	}()

	wg.Wait()
}

func (h *streamingHandler) setupHijack(w http.ResponseWriter) {
	w.Header().Set("Connection", "Upgrade")
	w.Header().Set("Upgrade", "tcp")
	w.WriteHeader(http.StatusSwitchingProtocols)
}

func (sm *streamManager) getToken(token string) (*streamingToken, bool) {
	val, ok := sm.tokens.Load(token)
	if !ok {
		return nil, false
	}
	st, ok := val.(*streamingToken)
	if !ok {
		return nil, false
	}
	if time.Now().After(st.Expires) {
		sm.tokens.Delete(token)
		return nil, false
	}
	return st, true
}

func (sm *streamManager) generateToken(t *streamingToken) string {
	if t.Token == "" {
		bytes := make([]byte, 16)
		rand.Read(bytes)
		t.Token = hex.EncodeToString(bytes)
	}
	t.Created = time.Now()
	t.Expires = t.Created.Add(4 * time.Hour)

	sm.tokens.Store(t.Token, t)
	go func() {
		<-time.After(time.Until(t.Expires))
		sm.tokens.Delete(t.Token)
	}()

	return t.Token
}

func (s *VyomaCriServer) Exec(ctx context.Context, req *pb.ExecRequest) (*pb.ExecResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("Exec: container=%s cmd=%v tty=%v", containerID, req.GetCmd(), req.GetTty())

	if containerID == "" {
		return nil, errorf(codes.InvalidArgument, "container ID required")
	}

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	s.mu.RLock()
	pod, _ := s.pods[container.PodID]
	s.mu.RUnlock()

	token := &streamingToken{
		Type:        "exec",
		ContainerID: containerID,
		VMID:        pod.VMID,
		VMIP:        pod.IP,
		Command:     req.GetCmd(),
		Tty:         req.GetTty(),
		Stdin:       req.GetStdin(),
		Stdout:      req.GetStdout(),
		Stderr:      req.GetStderr(),
	}

	tokenStr := s.streamManager.generateToken(token)

	url := fmt.Sprintf("http://localhost:%d/exec/%s", streamingPort, tokenStr)
	return &pb.ExecResponse{Url: url}, nil
}

func (s *VyomaCriServer) Attach(ctx context.Context, req *pb.AttachRequest) (*pb.AttachResponse, error) {
	containerID := req.GetContainerId()
	s.logger.Printf("Attach: container=%s tty=%v", containerID, req.GetTty())

	if containerID == "" {
		return nil, errorf(codes.InvalidArgument, "container ID required")
	}

	s.mu.RLock()
	container, ok := s.containers[containerID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "container not found: %s", containerID)
	}

	s.mu.RLock()
	pod, _ := s.pods[container.PodID]
	s.mu.RUnlock()

	token := &streamingToken{
		Type:        "attach",
		ContainerID: containerID,
		VMID:        pod.VMID,
		VMIP:        pod.IP,
		Tty:         req.GetTty(),
		Stdin:       req.GetStdin(),
		Stdout:      req.GetStdout(),
		Stderr:      req.GetStderr(),
	}

	tokenStr := s.streamManager.generateToken(token)

	url := fmt.Sprintf("http://localhost:%d/attach/%s", streamingPort, tokenStr)
	return &pb.AttachResponse{Url: url}, nil
}

func (s *VyomaCriServer) PortForward(ctx context.Context, req *pb.PortForwardRequest) (*pb.PortForwardResponse, error) {
	podID := req.GetPodSandboxId()
	ports := req.GetPort()
	s.logger.Printf("PortForward: pod=%s ports=%v", podID, ports)

	if podID == "" {
		return nil, errorf(codes.InvalidArgument, "pod sandbox ID required")
	}

	s.mu.RLock()
	pod, ok := s.pods[podID]
	s.mu.RUnlock()

	if !ok {
		return nil, errorf(codes.NotFound, "pod not found: %s", podID)
	}

	token := &streamingToken{
		Type:  "portforward",
		PodID: podID,
		VMID:  pod.VMID,
		VMIP:  pod.IP,
		Ports: ports,
	}

	tokenStr := s.streamManager.generateToken(token)

	url := fmt.Sprintf("http://localhost:%d/portforward/%s", streamingPort, tokenStr)
	return &pb.PortForwardResponse{Url: url}, nil
}

func (s *VyomaCriServer) ResizePty(ctx context.Context, req *pb.ResizePtyRequest) (*pb.ResizePtyResponse, error) {
	s.logger.Printf("ResizePty: container=%s size=%dx%d", req.GetContainerId(), req.GetWidth(), req.GetHeight())
	return &pb.ResizePtyResponse{}, nil
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