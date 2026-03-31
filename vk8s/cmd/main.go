package main

import (
	"context"
	"flag"
	"fmt"
	"os"
	"os/signal"
	"syscall"

	"github.com/ignite/vk8s/pkg/cri"
)

var (
	igniteAddr = flag.String("ignite-addr", "localhost:50051", "Address of the ignited gRPC server")
)

func main() {
	flag.Parse()

	server, err := cri.NewIgniteCriServer(*igniteAddr)
	if err != nil {
		fmt.Fprintf(os.Stderr, "Failed to create CRI server: %v\n", err)
		os.Exit(1)
	}

	ctx, cancel := context.WithCancel(context.Background())
	defer cancel()

	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	go func() {
		<-sigCh
		fmt.Println("Shutting down...")
		cancel()
	}()

	fmt.Printf("Starting vk8s CRI server on %s\n", cri.SocketPath)
	if err := server.Run(ctx); err != nil {
		fmt.Fprintf(os.Stderr, "Server error: %v\n", err)
		os.Exit(1)
	}
}
