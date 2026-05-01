use std::sync::Arc;
use tonic::{Request, Response, Status};
use tracing::{info, error};

use vyoma_proto::v1::vm_service_server::VmService;
use vyoma_proto::v1::{
    CreateVmRequest, CreateVmResponse, ExecOutput, ExecRequest, ListVmsRequest, ListVmsResponse,
    LogLine, LogRequest, MigrateRequest, MigrationProgress, PortMapping as ProtoPortMapping,
    RestoreRequest, SnapshotInfo, SnapshotRequest, VmIdRequest, VmInfo as ProtoVmInfo,
    VmStatusResponse, VolumeMapping as ProtoVolumeMapping,
};

use crate::state::AppState;
use crate::api::handlers;

pub struct GrpcVmService {
    state: AppState,
}

impl GrpcVmService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl VmService for GrpcVmService {
    async fn create_vm(
        &self,
        request: Request<CreateVmRequest>,
    ) -> Result<Response<CreateVmResponse>, Status> {
        // Just stub for now, logic will wrap around existing `api::handlers` soon!
        Err(Status::unimplemented("Not implemented"))
    }

    async fn start_vm(
        &self,
        request: Request<VmIdRequest>,
    ) -> Result<Response<VmStatusResponse>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn stop_vm(
        &self,
        request: Request<VmIdRequest>,
    ) -> Result<Response<VmStatusResponse>, Status> {
        let req = request.into_inner();
        let id = req.vm_id;
        info!("gRPC: Request to stop VM: {}", id);

        let vm_arc = {
            let mut vms = self.state.vms.lock().unwrap();
            vms.remove(&id)
        };

        if let Some(vm_mutex) = vm_arc {
            let mut vm = vm_mutex.lock().await;
            vm.cleanup(&self.state.cni_manager).await;

            if let Err(e) = self.state.wal.append(&crate::state::wal::WalEntry::vm_stop(id.clone())) {
                error!("Failed to write WAL entry: {}", e);
            }
            
            let _ = self.state.events_tx.send(serde_json::json!({
                "type": "vm_stop",
                "id": id
            }).to_string());
            
            Ok(Response::new(VmStatusResponse {
                vm_id: id,
                status: "stopped".to_string()
            }))
        } else {
            Err(Status::not_found("VM not found"))
        }
    }

    async fn delete_vm(
        &self,
        request: Request<VmIdRequest>,
    ) -> Result<Response<()>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn list_vms(
        &self,
        _request: Request<ListVmsRequest>,
    ) -> Result<Response<ListVmsResponse>, Status> {
        let instances: Vec<std::sync::Arc<tokio::sync::Mutex<crate::state::VmInstance>>> = {
            let vms_map = self.state.vms.lock().unwrap();
            vms_map.values().cloned().collect()
        };

        let mut vms = Vec::new();
        for arc_inst in instances {
            let inst = arc_inst.lock().await;
            vms.push(ProtoVmInfo {
                id: inst.id.clone(),
                image: inst.base_image_path.clone(),
                status: "running".to_string(),
                ip: inst.ip_address.clone(),
                vcpus: inst.vcpu,
                memory_mb: inst.mem_size_mib as u64,
                ports: vec![],
                created_at: 0,
            });
        }

        Ok(Response::new(ListVmsResponse { vms }))
    }

    async fn get_vm(
        &self,
        request: Request<VmIdRequest>,
    ) -> Result<Response<ProtoVmInfo>, Status> {
        let req = request.into_inner();
        let vm_arc = {
            let vms = self.state.vms.lock().unwrap();
            vms.get(&req.vm_id).cloned()
        };

        if let Some(vm_mutex) = vm_arc {
            let inst = vm_mutex.lock().await;
            let info = ProtoVmInfo {
                id: inst.id.clone(),
                image: inst.base_image_path.clone(),
                status: "running".to_string(),
                ip: inst.ip_address.clone(),
                vcpus: inst.vcpu,
                memory_mb: inst.mem_size_mib as u64,
                ports: vec![],
                created_at: 0,
            };
            Ok(Response::new(info))
        } else {
            Err(Status::not_found("VM not found"))
        }
    }

    type ExecCommandStream = tonic::codegen::BoxStream<ExecOutput>;

    async fn exec_command(
        &self,
        request: Request<ExecRequest>,
    ) -> Result<Response<Self::ExecCommandStream>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    type StreamLogsStream = tonic::codegen::BoxStream<LogLine>;

    async fn stream_logs(
        &self,
        request: Request<LogRequest>,
    ) -> Result<Response<Self::StreamLogsStream>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn create_snapshot(
        &self,
        request: Request<SnapshotRequest>,
    ) -> Result<Response<SnapshotInfo>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    async fn restore_snapshot(
        &self,
        request: Request<RestoreRequest>,
    ) -> Result<Response<ProtoVmInfo>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }

    type MigrateVmStream = tonic::codegen::BoxStream<MigrationProgress>;

    async fn migrate_vm(
        &self,
        request: Request<MigrateRequest>,
    ) -> Result<Response<Self::MigrateVmStream>, Status> {
        Err(Status::unimplemented("Not implemented"))
    }
}

use vyoma_proto::teleport::v1::teleport_service_server::TeleportService;
use vyoma_proto::teleport::v1::{TeleportChunk, TeleportAck};
use vyoma_teleport::TeleportReceiver;
use std::path::PathBuf;

pub struct GrpcTeleportService {
    state: AppState,
}

impl GrpcTeleportService {
    pub fn new(state: AppState) -> Self {
        Self { state }
    }
}

#[tonic::async_trait]
impl TeleportService for GrpcTeleportService {
    type TeleportVmStream = tokio_stream::wrappers::ReceiverStream<Result<TeleportAck, Status>>;

    async fn teleport_vm(
        &self,
        request: Request<tonic::Streaming<TeleportChunk>>,
    ) -> Result<Response<Self::TeleportVmStream>, Status> {
        info!("Receiving incoming Teleportation Request...");
        
        // Prepare temporary staging paths for the decompression
        let temp_dir = std::env::temp_dir().join("vyoma-teleport");
        tokio::fs::create_dir_all(&temp_dir).await
            .map_err(|e| Status::internal(format!("Failed to create temp dir: {}", e)))?;
            
        let mem_file = temp_dir.join("teleport_mem.zstd");
        let state_file = temp_dir.join("teleport_state.bin");
        
        let mut receiver = TeleportReceiver::new(mem_file, state_file);
        
        // Let the receiver process the stream
        receiver.process_stream(request).await
    }
}
