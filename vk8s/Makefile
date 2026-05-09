# vk8s Makefile - CRI v1 Implementation
# Generates Go code from protobuf definitions

PROTO_DIR := proto
PROTO_FILE := vm.proto
PROTO_OUT := pkg/vyoma/proto

PROTOC := protoc
GOPATH := $(shell go env GOPATH 2>/dev/null || echo "$(HOME)/go")

.PHONY: all generate clean proto-check deps

all: deps generate

deps:
	@echo "Checking dependencies..."
	@which protoc > /dev/null || (echo "protoc not found. Install with: apt install protobuf-compiler" && exit 1)
	@if [ ! -f "$(GOPATH)/bin/protoc-gen-go" ]; then \
		docker run --rm -v $(GOPATH):/output golang:1.23 sh -c "GOPATH=/output go install google.golang.org/protobuf/cmd/protoc-gen-go@v1.32.0 && cp /output/bin/* /output/"; \
	fi

proto-check:
	@echo "Checking protoc installation..."
	@$(PROTOC) --version || (echo "protoc not installed" && exit 1)

generate: proto-check
	@echo "Generating Go code from proto files..."
	@mkdir -p $(PROTO_OUT)
	@PATH="$(GOPATH)/bin:$$PATH" $(PROTOC) \
		--go_out=. --go_opt=paths=source_relative \
		--go-grpc_out=. --go-grpc_opt=paths=source_relative \
		-I$(PROTO_DIR) \
		$(PROTO_DIR)/$(PROTO_FILE)
	@mv $(PROTO_OUT)/vm_grpc.pb.go $(PROTO_OUT)/ 2>/dev/null || true
	@mv $(PROTO_OUT)/vm.pb.go $(PROTO_OUT)/ 2>/dev/null || true
	@mv vm_grpc.pb.go $(PROTO_OUT)/ 2>/dev/null || true
	@mv vm.pb.go $(PROTO_OUT)/ 2>/dev/null || true
	@echo "Proto generation complete."

clean:
	@echo "Cleaning generated files..."
	@rm -f $(PROTO_OUT)/*.pb.go

proto-deps:
	go get google.golang.org/grpc@v1.60.0
	go get google.golang.org/protobuf@v1.32.0
	go get k8s.io/cri-api@v0.29.0

update-cri:
	go get k8s.io/cri-api@latest
	go mod tidy

test:
	go build ./...
	go vet ./...

.PHONY: all generate clean proto-check deps test update-cri