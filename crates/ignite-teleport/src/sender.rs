use std::path::PathBuf;
use tokio::fs::File;
use tokio::io::{AsyncReadExt, BufReader};
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tonic::transport::Endpoint;
use tracing::{error, info};
use uuid::Uuid;

use async_compression::tokio::bufread::ZstdEncoder;

use ignite_proto::teleport::v1::teleport_chunk::Content;
use ignite_proto::teleport::v1::teleport_service_client::TeleportServiceClient;
use ignite_proto::teleport::v1::{TeleportChunk, VmMetadata};

pub struct Teleporter {
    vm_id: String,
    target_addr: String,
    memory_size_bytes: u64,
}

impl Teleporter {
    pub fn new(vm_id: String, target_addr: String, memory_size_bytes: u64) -> Self {
        info!("Initializing Teleporter for VM {}", vm_id);
        Self {
            vm_id,
            target_addr,
            memory_size_bytes,
        }
    }

    pub async fn teleport_vm(&self, memory_file: PathBuf, state_file: PathBuf) -> Result<(), String> {
        info!("Starting robust ZSTD gRPC Teleportation to {}", self.target_addr);
        
        let endpoint = Endpoint::from_shared(self.target_addr.clone())
            .map_err(|e| format!("Invalid target address URI: {}", e))?;
        
        let mut client = TeleportServiceClient::connect(endpoint)
            .await
            .map_err(|e| format!("Failed to connect to teleport target: {}", e))?;

        let session_id = Uuid::new_v4().to_string();
        
        // Setup mpsc stream for gRPC transmission
        let (tx, rx) = mpsc::channel::<TeleportChunk>(32);
        
        // Spawn the reader task
        let memory_size = self.memory_size_bytes;
        let session = session_id.clone();
        let vm = self.vm_id.clone();
        
        tokio::spawn(async move {
            let mut seq = 0;
            
            // 1. Send Metadata
            let meta_chunk = TeleportChunk {
                session_id: session.clone(),
                chunk_sequence: seq,
                content: Some(Content::Metadata(VmMetadata {
                    id: vm.clone(),
                    config_json: "{}".to_string(), // In reality we'd parse the Vm object
                    memory_size_bytes: memory_size,
                })),
            };
            
            if tx.send(meta_chunk).await.is_err() {
                error!("Failed to send Teleport Metadata");
                return;
            }
            seq += 1;
            
            // 2. Stream State File (uncompressed context)
            if let Ok(mut state_f) = File::open(&state_file).await {
                let mut buf = vec![0u8; 1024 * 64]; // 64kb chunks
                while let Ok(n) = state_f.read(&mut buf).await {
                    if n == 0 { break; }
                    let chunk = TeleportChunk {
                        session_id: session.clone(),
                        chunk_sequence: seq,
                        content: Some(Content::StateChunk(buf[..n].to_vec())),
                    };
                    if tx.send(chunk).await.is_err() {
                        return;
                    }
                    seq += 1;
                }
            } else {
                error!("Could not open state file for Teleportation");
                return;
            }

            // 3. Stream Memory File WITH ZSTD Compression
            if let Ok(mem_f) = File::open(&memory_file).await {
                let reader = BufReader::new(mem_f);
                let mut encoder = ZstdEncoder::new(reader);
                
                let mut buf = vec![0u8; 1024 * 256]; // 256kb compressed chunks
                while let Ok(n) = encoder.read(&mut buf).await {
                    if n == 0 { break; }
                    let chunk = TeleportChunk {
                        session_id: session.clone(),
                        chunk_sequence: seq,
                        content: Some(Content::MemoryChunk(buf[..n].to_vec())),
                    };
                    if tx.send(chunk).await.is_err() {
                        return;
                    }
                    seq += 1;
                }
            } else {
                error!("Could not open memory file for Teleportation");
            }
        });

        // 4. Begin gRPC bidirectional stream
        let request_stream = ReceiverStream::new(rx);
        let response = client.teleport_vm(request_stream)
            .await
            .map_err(|e| format!("Teleportation stream dropped: {}", e))?;
            
        let mut response_stream = response.into_inner();
        
        while let Ok(Some(ack)) = response_stream.message().await {
            if !ack.received {
                return Err(format!("Target node rejected chunk {}: {}", ack.processed_sequence, ack.error_msg));
            }
            // In a real system, we'd log progress percentages here based on total bytes
        }

        info!("Teleportation session {} completed successfully!", session_id);
        Ok(())
    }
}
