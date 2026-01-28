// Jeeves Kernel Server
//
// Standalone gRPC server for the Jeeves kernel and engine services.
// This binary can be run as a sidecar process or remote service.
//
// Usage:
//
//	go run ./cmd/main.go                    # Default :50051
//	go run ./cmd/main.go -addr :8080        # Custom port
//	go build -o jeeves-kernel ./cmd && ./jeeves-kernel
package main

import (
	"flag"
	"fmt"
	"log"
	"os"
	"os/signal"
	"syscall"

	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/grpc"
	"github.com/jeeves-cluster-organization/codeanalysis/coreengine/kernel"
)

// stdLogger implements grpc.Logger using standard library log.
type stdLogger struct{}

func (l *stdLogger) Debug(msg string, keysAndValues ...any) {
	log.Printf("[DEBUG] %s %v", msg, keysAndValues)
}

func (l *stdLogger) Info(msg string, keysAndValues ...any) {
	log.Printf("[INFO] %s %v", msg, keysAndValues)
}

func (l *stdLogger) Warn(msg string, keysAndValues ...any) {
	log.Printf("[WARN] %s %v", msg, keysAndValues)
}

func (l *stdLogger) Error(msg string, keysAndValues ...any) {
	log.Printf("[ERROR] %s %v", msg, keysAndValues)
}

func main() {
	// Parse command-line flags
	addr := flag.String("addr", ":50051", "gRPC server address")
	flag.Parse()

	logger := &stdLogger{}
	logger.Info("jeeves_kernel_starting", "version", "1.0.0", "address", *addr)

	// Create kernel with all subsystems (nil config uses defaults)
	k := kernel.NewKernel(logger, nil)
	logger.Info("kernel_created")

	// Create gRPC server with kernel
	server := grpc.NewEngineServer(logger)
	server.SetKernel(k)
	logger.Info("grpc_server_configured", "services", []string{"EngineService", "KernelService"})

	// Handle graceful shutdown
	sigCh := make(chan os.Signal, 1)
	signal.Notify(sigCh, syscall.SIGINT, syscall.SIGTERM)

	// Start server in background
	grpcServer, err := grpc.StartBackground(*addr, server)
	if err != nil {
		log.Fatalf("Failed to start server: %v", err)
	}

	logger.Info("jeeves_kernel_ready", "address", *addr)
	fmt.Printf("\nJeeves Kernel Server running on %s\n", *addr)
	fmt.Println("Press Ctrl+C to stop")

	// Wait for shutdown signal
	sig := <-sigCh
	logger.Info("shutdown_signal_received", "signal", sig.String())

	// Graceful shutdown
	grpcServer.GracefulStop()
	logger.Info("jeeves_kernel_stopped")
}
