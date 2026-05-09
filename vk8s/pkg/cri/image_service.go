package cri

import (
	"bytes"
	"context"
	"encoding/json"
	"fmt"
	"io"
	"net/http"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
)

func (s *VyomaCriServer) PullImage(ctx context.Context, req *pb.PullImageRequest) (*pb.PullImageResponse, error) {
	image := req.GetImage().GetImage()
	s.logger.Printf("PullImage: %s", image)

	body, err := json.Marshal(map[string]string{"image": image})
	if err != nil {
		return nil, errorf(codes.Internal, "marshal request: %v", err)
	}

	data, err := s.httpRequest(ctx, "POST", "/pull", bytes.NewReader(body))
	if err != nil {
		s.logError(ctx, "PullImage", err)
		return nil, errorf(codes.Internal, "pull image: %v", err)
	}

	var resp struct {
		Status string `json:"status"`
		Path   string `json:"path"`
	}
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, errorf(codes.Internal, "decode response: %v", err)
	}

	s.logger.Printf("Image pulled: %s -> %s", image, resp.Path)
	return &pb.PullImageResponse{ImageRef: image}, nil
}

func (s *VyomaCriServer) ListImages(ctx context.Context, req *pb.ListImagesRequest) (*pb.ListImagesResponse, error) {
	data, err := s.httpRequest(ctx, "GET", "/images", nil)
	if err != nil {
		s.logError(ctx, "ListImages", err)
		return &pb.ListImagesResponse{Images: []*pb.Image{}}, nil
	}

	var resp struct {
		Images []struct {
			Name string `json:"name"`
			Size int64  `json:"size"`
		} `json:"images"`
	}
	if err := json.Unmarshal(data, &resp); err != nil {
		return nil, errorf(codes.Internal, "decode response: %v", err)
	}

	images := make([]*pb.Image, 0, len(resp.Images))
	for _, img := range resp.Images {
		images = append(images, &pb.Image{
			Id:       img.Name,
			RepoTags: []string{img.Name},
			Size_:    uint64(img.Size),
		})
	}

	return &pb.ListImagesResponse{Images: images}, nil
}

func (s *VyomaCriServer) ImageStatus(ctx context.Context, req *pb.ImageStatusRequest) (*pb.ImageStatusResponse, error) {
	image := req.GetImage().GetImage()

	data, err := s.httpRequest(ctx, "GET", "/images/"+image, nil)
	if err != nil {
		return &pb.ImageStatusResponse{}, nil
	}

	var img struct {
		Name  string `json:"name"`
		Size  int64  `json:"size"`
	}
	if err := json.Unmarshal(data, &img); err != nil {
		return nil, errorf(codes.Internal, "decode response: %v", err)
	}

	return &pb.ImageStatusResponse{
		Image: &pb.Image{
			Id:       img.Name,
			RepoTags: []string{img.Name},
			Size_:    uint64(img.Size),
		},
	}, nil
}

func (s *VyomaCriServer) RemoveImage(ctx context.Context, req *pb.RemoveImageRequest) (*pb.RemoveImageResponse, error) {
	image := req.GetImage().GetImage()
	s.logger.Printf("RemoveImage: %s", image)

	_, err := s.httpRequest(ctx, "DELETE", "/images/"+image, nil)
	if err != nil {
		s.logError(ctx, "RemoveImage", err)
	}

	return &pb.RemoveImageResponse{}, nil
}

func (s *VyomaCriServer) ImageFsInfo(ctx context.Context, req *pb.ImageFsInfoRequest) (*pb.ImageFsInfoResponse, error) {
	return &pb.ImageFsInfoResponse{
		ImageFilesystems: []*pb.FilesystemUsage{
			{
				Timestamp:  time.Now().Unix(),
				FsId:       &pb.FilesystemIdentifier{Mountpoint: "/var/lib/vyoma/images"},
				UsedBytes:  &pb.UInt64Value{Value: 0},
				InodesUsed: &pb.UInt64Value{Value: 0},
			},
		},
	}, nil
}