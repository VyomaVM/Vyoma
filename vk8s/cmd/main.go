package main

import (
	"context"
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/vyoma/vk8s/pkg/cri"
)

var (
	vyomaGRPCAddr = flag.String("vyoma-grpc", "localhost:7071", "Address of the vyomad gRPC server")
	vyomaHTTPAddr = flag.String("vyoma-http", "http://localhost:8080", "Base URL of the vyomad HTTP server")
)

func main() {
	flag.Parse()

	log.Printf("Starting vyoma-k8s CRI server")
	log.Printf("  gRPC endpoint: %s", *vyomaGRPCAddr)
	log.Printf("  HTTP endpoint: %s", *vyomaHTTPAddr)
	log.Printf("  CRI socket: %s", cri.SocketPath)

	server, err := cri.NewVyomaCriServer(*vyomaGRPCAddr, *vyomaHTTPAddr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to create CRI server: %v\n", err)
		os.Exit(1)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	if err := server.StartStreamingServer(); err != nil {
		log.Printf("Warning: failed to start streaming server: %v", err)
	}

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigCh
		log.Println("Shutting down...")
		cancel()
	}()

	if err := server.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "Server error: %v\n", err)
		os.Exit(1)
	}
}