module github.com/vyoma/vk8s

go 1.21

require (
	google.golang.org/grpc v1.64.0
	google.golang.org/protobuf v1.33.0
	k8s.io/cri-api v0.29.0
)

require (
	github.com/gogo/protobuf v1.3.2 // indirect
	golang.org/x/net v0.22.0 // indirect
	golang.org/x/sys v0.18.0 // indirect
	golang.org/x/text v0.14.0 // indirect
	google.golang.org/genproto/googleapis/rpc v0.0.0-20240318140521-94a12d6c2237 // indirect
)

replace k8s.io/cri-api => k8s.io/cri-api v0.29.0
