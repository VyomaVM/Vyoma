package agent

import (
	"bytes"
	"context"
	"encoding/binary"
	"encoding/json"
	"fmt"
	"io"
	"net"
	"time"
)

const (
	VSOCKPort   = 9999
	TCPPort     = 9999
	DialTimeout = 5 * time.Second
	ReadTimeout = 30 * time.Second

	VMADDR_CID_HOST  = 2
	VMADDR_CID_LOCAL  = 1
	VMADDR_CID_ANY    = ^uint32(0)
)

type Request struct {
	Type    string            `json:"type"`
	Cmd     []string          `json:"cmd,omitempty"`
	Env     map[string]string `json:"env,omitempty"`
	Workdir string            `json:"workdir,omitempty"`
	Path    string            `json:"path,omitempty"`
}

type Response struct {
	Type      string        `json:"type"`
	Processes []ProcessInfo `json:"processes,omitempty"`
	Stdout    []byte        `json:"stdout,omitempty"`
	Stderr    []byte        `json:"stderr,omitempty"`
	ExitCode  int           `json:"exit_code,omitempty"`
	Metrics   *Metrics      `json:"metrics,omitempty"`
	Content   []byte        `json:"content,omitempty"`
	Message   string        `json:"message,omitempty"`
}

type ProcessInfo struct {
	PID   uint32 `json:"pid"`
	PPID  uint32 `json:"ppid"`
	Name  string `json:"name"`
	State string `json:"state"`
}

type Metrics struct {
	CPUUserMs    uint64 `json:"cpu_user_ms"`
	CPUSystemMs  uint64 `json:"cpu_system_ms"`
	MemUsedKb    uint64 `json:"mem_used_kb"`
	MemTotalKb   uint64 `json:"mem_total_kb"`
	ProcessCount int    `json:"process_count"`
}

type Client struct {
	vmIP string
	conn net.Conn
}

func NewTCPClient(vmIP string) *Client {
	return &Client{vmIP: vmIP}
}

func (c *Client) Connect(ctx context.Context) error {
	addr := fmt.Sprintf("%s:%d", c.vmIP, TCPPort)
	dialer := &net.Dialer{Timeout: DialTimeout}

	conn, err := dialer.DialContext(ctx, "tcp", addr)
	if err != nil {
		return fmt.Errorf("tcp dial %s: %w", addr, err)
	}

	c.conn = conn
	return nil
}

func (c *Client) Close() error {
	if c.conn != nil {
		return c.conn.Close()
	}
	return nil
}

func (c *Client) sendRequest(ctx context.Context, req Request) (Response, error) {
	if c.conn == nil {
		if err := c.Connect(ctx); err != nil {
			return Response{}, err
		}
	}

	reqData, err := json.Marshal(req)
	if err != nil {
		return Response{}, fmt.Errorf("marshal request: %w", err)
	}

	var length uint32
	if err := binary.Read(bytes.NewReader([]byte{0, 0, 0, 0}), binary.BigEndian, &length); err != nil {
	}
	_ = length

	if err := c.conn.SetWriteDeadline(time.Now().Add(ReadTimeout)); err != nil {
		return Response{}, fmt.Errorf("set write deadline: %w", err)
	}

	header := make([]byte, 4)
	binary.BigEndian.PutUint32(header, uint32(len(reqData)))

	if _, err := c.conn.Write(header); err != nil {
		return Response{}, fmt.Errorf("write header: %w", err)
	}
	if _, err := c.conn.Write(reqData); err != nil {
		return Response{}, fmt.Errorf("write request: %w", err)
	}

	respHeader := make([]byte, 4)
	if _, err := io.ReadFull(c.conn, respHeader); err != nil {
		return Response{}, fmt.Errorf("read header: %w", err)
	}
	respLen := binary.BigEndian.Uint32(respHeader)

	if err := c.conn.SetReadDeadline(time.Now().Add(ReadTimeout)); err != nil {
		return Response{}, fmt.Errorf("set read deadline: %w", err)
	}

	respData := make([]byte, respLen)
	if _, err := io.ReadFull(c.conn, respData); err != nil {
		return Response{}, fmt.Errorf("read response: %w", err)
	}

	var resp Response
	if err := json.Unmarshal(respData, &resp); err != nil {
		return Response{}, fmt.Errorf("unmarshal response: %w", err)
	}

	return resp, nil
}

func (c *Client) ExecCommand(ctx context.Context, cmd []string, env map[string]string, workdir string) (stdout, stderr []byte, exitCode int, err error) {
	req := Request{
		Type:    "ExecCommand",
		Cmd:     cmd,
		Env:     env,
		Workdir: workdir,
	}

	resp, err := c.sendRequest(ctx, req)
	if err != nil {
		return nil, nil, -1, err
	}

	if resp.Type == "Error" {
		return nil, nil, -1, fmt.Errorf("agent error: %s", resp.Message)
	}

	return resp.Stdout, resp.Stderr, resp.ExitCode, nil
}

func (c *Client) ListProcesses(ctx context.Context) ([]ProcessInfo, error) {
	req := Request{Type: "ProcessList"}

	resp, err := c.sendRequest(ctx, req)
	if err != nil {
		return nil, err
	}

	if resp.Type == "Error" {
		return nil, fmt.Errorf("agent error: %s", resp.Message)
	}

	return resp.Processes, nil
}

func (c *Client) GetMetrics(ctx context.Context) (*Metrics, error) {
	req := Request{Type: "GetMetrics"}

	resp, err := c.sendRequest(ctx, req)
	if err != nil {
		return nil, err
	}

	if resp.Type == "Error" {
		return nil, fmt.Errorf("agent error: %s", resp.Message)
	}

	return resp.Metrics, nil
}

func (c *Client) ReadFile(ctx context.Context, path string) ([]byte, error) {
	req := Request{Type: "FileRead", Path: path}

	resp, err := c.sendRequest(ctx, req)
	if err != nil {
		return nil, err
	}

	if resp.Type == "Error" {
		return nil, fmt.Errorf("agent error: %s", resp.Message)
	}

	return resp.Content, nil
}