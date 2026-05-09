package client

import (
	"context"
	"fmt"

	"google.golang.org/grpc"
	"google.golang.org/grpc/credentials/insecure"

	vyomav1 "github.com/vyoma/vk8s/pkg/vyoma/proto"
)

const (
	DefaultVyomadAddr = "localhost:7071"
)

type Client struct {
	conn  *grpc.ClientConn
	vmSvc vyomav1.VmServiceClient
}

type VmServiceClient interface {
	CreateVm(ctx context.Context, in *vyomav1.CreateVmRequest, opts ...grpc.CallOption) (*vyomav1.CreateVmResponse, error)
	StartVm(ctx context.Context, in *vyomav1.VmIdRequest, opts ...grpc.CallOption) (*vyomav1.VmStatusResponse, error)
	StopVm(ctx context.Context, in *vyomav1.VmIdRequest, opts ...grpc.CallOption) (*vyomav1.VmStatusResponse, error)
	DeleteVm(ctx context.Context, in *vyomav1.VmIdRequest, opts ...grpc.CallOption) (*vyomav1.Empty, error)
	ListVms(ctx context.Context, in *vyomav1.ListVmsRequest, opts ...grpc.CallOption) (*vyomav1.ListVmsResponse, error)
	GetVm(ctx context.Context, in *vyomav1.VmIdRequest, opts ...grpc.CallOption) (*vyomav1.VmInfo, error)
}

func NewClient(addr string) (*Client, error) {
	if addr == "" {
		addr = DefaultVyomadAddr
	}
	conn, err := grpc.Dial(addr, grpc.WithTransportCredentials(insecure.NewCredentials()))
	if err != nil {
		return nil, fmt.Errorf("failed to connect to vyomad: %w", err)
	}

	return &Client{
		conn:  conn,
		vmSvc: vyomav1.NewVmServiceClient(conn),
	}, nil
}

func (c *Client) CreateVm(ctx context.Context, req *vyomav1.CreateVmRequest) (*vyomav1.CreateVmResponse, error) {
	return c.vmSvc.CreateVm(ctx, req)
}

func (c *Client) StartVm(ctx context.Context, req *vyomav1.VmIdRequest) (*vyomav1.VmStatusResponse, error) {
	return c.vmSvc.StartVm(ctx, req)
}

func (c *Client) StopVm(ctx context.Context, req *vyomav1.VmIdRequest) (*vyomav1.VmStatusResponse, error) {
	return c.vmSvc.StopVm(ctx, req)
}

func (c *Client) DeleteVm(ctx context.Context, req *vyomav1.VmIdRequest) (*vyomav1.Empty, error) {
	return c.vmSvc.DeleteVm(ctx, req)
}

func (c *Client) ListVms(ctx context.Context, req *vyomav1.ListVmsRequest) (*vyomav1.ListVmsResponse, error) {
	return c.vmSvc.ListVms(ctx, req)
}

func (c *Client) GetVm(ctx context.Context, req *vyomav1.VmIdRequest) (*vyomav1.VmInfo, error) {
	return c.vmSvc.GetVm(ctx, req)
}

func (c *Client) Close() error {
	return c.conn.Close()
}