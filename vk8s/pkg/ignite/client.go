package ignite

import (
	"context"
	"fmt"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"
)

type Client struct {
	conn   *grpc.ClientConn
	vmSvc  VmServiceClient
}

type VmServiceClient interface {
	CreateVm(ctx context.Context, in *CreateVmRequest, opts ...grpc.CallOption) (*CreateVmResponse, error)
	StartVm(ctx context.Context, in *VmIdRequest, opts ...grpc.CallOption) (*VmStatusResponse, error)
	StopVm(ctx context.Context, in *VmIdRequest, opts ...grpc.CallOption) (*VmStatusResponse, error)
	DeleteVm(ctx context.Context, in *VmIdRequest, opts ...grpc.CallOption) (*Empty, error)
	ListVms(ctx context.Context, in *ListVmsRequest, opts ...grpc.CallOption) (*ListVmsResponse, error)
	GetVm(ctx context.Context, in *VmIdRequest, opts ...grpc.CallOption) (*VmInfo, error)
}

func NewClient(addr string) (*Client, error) {
	conn, err := grpc.Dial(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, fmt.Errorf("failed to connect to ignited: %w", err)
	}

	return &Client{
		conn:   conn,
		vmSvc:  NewVmServiceClient(conn),
	}, nil
}

func (c *Client) CreateVm(ctx context.Context, req *CreateVmRequest) (*CreateVmResponse, error) {
	return c.vmSvc.CreateVm(ctx, req)
}

func (c *Client) StartVm(ctx context.Context, req *VmIdRequest) (*VmStatusResponse, error) {
	return c.vmSvc.StartVm(ctx, req)
}

func (c *Client) StopVm(ctx context.Context, req *VmIdRequest) (*VmStatusResponse, error) {
	return c.vmSvc.StopVm(ctx, req)
}

func (c *Client) DeleteVm(ctx context.Context, req *VmIdRequest) (*Empty, error) {
	return c.vmSvc.DeleteVm(ctx, req)
}

func (c *Client) ListVms(ctx context.Context, req *ListVmsRequest) (*ListVmsResponse, error) {
	return c.vmSvc.ListVms(ctx, req)
}

func (c *Client) GetVm(ctx context.Context, req *VmIdRequest) (*VmInfo, error) {
	return c.vmSvc.GetVm(ctx, req)
}

func (c *Client) Close() error {
	return c.conn.Close()
}

type CreateVmRequest struct {
	Name      string
	Namespace string
	Vcpus     uint32
	MemoryMb  uint64
	Labels    map[string]string
	Ports     []*PortMapping
	Volumes   []*VolumeMapping
}

type CreateVmResponse struct {
	VmId string
}

type VmIdRequest struct {
	VmId string
}

type VmStatusResponse struct {
	VmId   string
	Status string
}

type Empty struct{}

type ListVmsRequest struct{}

type ListVmsResponse struct {
	Vms []*VmInfo
}

type VmInfo struct {
	Id        string
	Image     string
	Status    string
	Ip        string
	Vcpus     uint32
	MemoryMb  uint64
	Ports     []*PortMapping
	CreatedAt int64
}

type PortMapping struct {
	Host uint32
	Vm   uint32
}

type VolumeMapping struct {
	HostPath string
	VmPath   string
}
