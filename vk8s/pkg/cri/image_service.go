package cri

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"time"

	pb "k8s.io/cri-api/pkg/apis/runtime/v1"
)

const (
	vyomadHTTPAddr = "http://localhost:8080"
)

func (s *VyomaCriServer) PullImage(ctx context.Context, req *pb.PullImageRequest) (*pb.PullImageResponse, error) {
	imageRef := req.GetImage().GetImage()

	pullReq := map[string]string{
		"image": imageRef,
	}
	pullReqBytes, err := json.Marshal(pullReq)
	if err != nil {
		return nil, fmt.Errorf("failed to marshal pull request: %w", err)
	}

	httpReq, err := http.NewRequestWithContext(ctx, "POST", vyomadHTTPAddr+"/pull", nil)
	if err != nil {
		return nil, fmt.Errorf("failed to create HTTP request: %w", err)
	}
	httpReq.Header.Set("Content-Type", "application/json")
	httpReq.Body = http.NoBody
	httpReq.GetBody = nil

	client := &http.Client{Timeout: 10 * time.Minute}
	resp, err := client.Do(httpReq)
	if err != nil {
		return nil, fmt.Errorf("failed to pull image from vyomad: %w", err)
	}
	defer resp.Body.Close()

	if resp.StatusCode != http.StatusOK {
		return nil, fmt.Errorf("vyomad returned status: %d", resp.StatusCode)
	}

	var pullResp struct {
		Image string `json:"image"`
	}
	if err := json.NewDecoder(resp.Body).Decode(&pullResp); err != nil {
		return nil, fmt.Errorf("failed to decode pull response: %w", err)
	}

	s.mu.Lock()
	s.imageStore[imageRef] = &ImageInfo{
		ID:       imageRef,
		RepoTags: []string{imageRef},
		Size:     0,
		Created:  time.Now().Unix(),
	}
	s.mu.Unlock()

	return &pb.PullImageResponse{
		ImageRef: imageRef,
	}, nil
}

func (s *VyomaCriServer) ListImages(ctx context.Context, req *pb.ListImagesRequest) (*pb.ListImagesResponse, error) {
	s.mu.RLock()
	defer s.mu.RUnlock()

	filter := req.GetFilter()
	var images []*pb.Image

	for _, img := range s.imageStore {
		if filter != nil {
			if filter.Image != nil && filter.Image.Image != "" && img.ID != filter.Image.Image {
				continue
			}
			if len(filter.RepoTags) > 0 {
				hasMatch := false
				for _, rt := range filter.RepoTags {
					for _, imgRT := range img.RepoTags {
						if rt == imgRT {
							hasMatch = true
							break
						}
					}
				}
				if !hasMatch {
					continue
				}
			}
		}

		images = append(images, &pb.Image{
			Id:          img.ID,
			RepoTags:    img.RepoTags,
			Size_:       img.Size,
			CreatedAt:   img.Created,
		})
	}

	if images == nil {
		images = []*pb.Image{}
	}

	return &pb.ListImagesResponse{Images: images}, nil
}

func (s *VyomaCriServer) ImageStatus(ctx context.Context, req *pb.ImageStatusRequest) (*pb.ImageStatusResponse, error) {
	imageRef := req.GetImage().GetImage()

	s.mu.RLock()
	img, ok := s.imageStore[imageRef]
	s.mu.RUnlock()

	if !ok {
		return &pb.ImageStatusResponse{}, nil
	}

	return &pb.ImageStatusResponse{
		Image: &pb.Image{
			Id:          img.ID,
			RepoTags:    img.RepoTags,
			Size_:       img.Size,
			CreatedAt:   img.Created,
		},
	}, nil
}

func (s *VyomaCriServer) RemoveImage(ctx context.Context, req *pb.RemoveImageRequest) (*pb.RemoveImageResponse, error) {
	imageRef := req.GetImage().GetImage()

	s.mu.Lock()
	defer s.mu.Unlock()

	delete(s.imageStore, imageRef)

	return &pb.RemoveImageResponse{}, nil
}

func (s *VyomaCriServer) ImageFsInfo(ctx context.Context, req *pb.ImageFsInfoRequest) (*pb.ImageFsInfoResponse, error) {
	imageFilesystems := []*pb.FilesystemUsage{
		{
			Timestamp: time.Now().Unix(),
			FsId: &pb.FilesystemIdentifier{
				Mountpoint: "/var/lib/vyoma/images",
			},
			UsedBytes:  &pb.UInt64Value{Value: 0},
			InodesUsed: &pb.UInt64Value{Value: 0},
		},
	}

	return &pb.ImageFsInfoResponse{ImageFilesystems: imageFilesystems}, nil
}