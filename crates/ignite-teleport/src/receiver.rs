use std::net::SocketAddr;
use tracing::info;
use bytes::Bytes;
use tokio::io::AsyncReadExt;

use crate::sender::{MigrationHeader, MigrationSignal, MigrationStats};

pub struct MigrationReceiver {
    vm_id: String,
    memory_buffer: Bytes,
    snapshot_buffer: Bytes,
}

impl MigrationReceiver {
    pub fn new(vm_id: String) -> Self {
        info!("Creating MigrationReceiver for VM {}", vm_id);
        
        Self {
            vm_id,
            memory_buffer: Bytes::new(),
            snapshot_buffer: Bytes::new(),
        }
    }

    pub async fn receive(&mut self, listen_addr: SocketAddr) -> Result<MigrationStats, String> {
        info!("Starting migration receiver on {}", listen_addr);
        
        let listener = tokio::net::TcpListener::bind(listen_addr)
            .await
            .map_err(|e| format!("Failed to bind: {}", e))?;
        
        let (mut stream, addr) = listener.accept()
            .await
            .map_err(|e| format!("Failed to accept connection: {}", e))?;
        
        info!("Accepted migration connection from {}", addr);
        
        let mut total_bytes: u64 = 0;
        
        let mut header_len_buf = [0u8; 4];
        stream.read_exact(&mut header_len_buf).await
            .map_err(|e| format!("Failed to read header length: {}", e))?;
        let header_len = u32::from_be_bytes(header_len_buf) as usize;
        
        let mut header_buf = vec![0u8; header_len];
        stream.read_exact(&mut header_buf).await
            .map_err(|e| format!("Failed to read header: {}", e))?;
        
        let header: MigrationHeader = serde_json::from_slice(&header_buf)
            .map_err(|e| format!("Failed to parse header: {}", e))?;
        
        info!("Receiving VM {} ({} bytes)", header.vm_id, header.memory_bytes);
        
        let mut page_count_buf = [0u8; 8];
        stream.read_exact(&mut page_count_buf).await
            .map_err(|e| format!("Failed to read page count: {}", e))?;
        let page_count = u64::from_be_bytes(page_count_buf) as usize;
        
        let mut page_data = vec![0u8; page_count];
        stream.read_exact(&mut page_data).await
            .map_err(|e| format!("Failed to read pages: {}", e))?;
        
        self.memory_buffer = Bytes::from(page_data.clone());
        total_bytes += page_count as u64;
        
        info!("Phase 1: Received initial {} pages", page_count);
        
        let mut rounds: u32 = 0;
        
        loop {
            let mut size_buf = [0u8; 8];
            match stream.read_exact(&mut size_buf).await {
                Ok(_) => {}
                Err(e) if e.kind() == std::io::ErrorKind::UnexpectedEof => {
                    break;
                }
                Err(e) => return Err(format!("Failed to read: {}", e)),
            }
            
            let size = u64::from_be_bytes(size_buf) as usize;
            let mut data = vec![0u8; size];
            stream.read_exact(&mut data).await
                .map_err(|e| format!("Failed to read data: {}", e))?;
            
            total_bytes += size as u64;
            rounds += 1;
            
            info!("Phase 2: Received dirty data round {}", rounds);
        }
        
        let pause_signal = self.receive_signal(&mut stream).await?;
        if !matches!(pause_signal, MigrationSignal::Pause) {
            return Err("Expected pause signal".to_string());
        }
        
        info!("Phase 3: Received pause signal, finalizing migration");
        
        let mut snapshot_size_buf = [0u8; 8];
        stream.read_exact(&mut snapshot_size_buf).await
            .map_err(|e| format!("Failed to read snapshot size: {}", e))?;
        let snapshot_size = u64::from_be_bytes(snapshot_size_buf) as usize;
        
        let mut snapshot_data = vec![0u8; snapshot_size];
        stream.read_exact(&mut snapshot_data).await
            .map_err(|e| format!("Failed to read snapshot: {}", e))?;
        
        self.snapshot_buffer = Bytes::from(snapshot_data);
        total_bytes += snapshot_size as u64;
        
        let resume_signal = self.receive_signal(&mut stream).await?;
        if !matches!(resume_signal, MigrationSignal::Resume) {
            return Err("Expected resume signal".to_string());
        }
        
        info!("Phase 3: Received resume signal, VM ready on destination");
        
        Ok(MigrationStats {
            rounds,
            total_pages: header.memory_bytes / 4096,
            bytes_transferred: total_bytes,
        })
    }

    async fn receive_signal(
        &self,
        stream: &mut tokio::net::TcpStream,
    ) -> Result<MigrationSignal, String> {
        let mut len_buf = [0u8; 4];
        stream.read_exact(&mut len_buf).await
            .map_err(|e| format!("Failed to read signal length: {}", e))?;
        let len = u32::from_be_bytes(len_buf) as usize;
        
        let mut buf = vec![0u8; len];
        stream.read_exact(&mut buf).await
            .map_err(|e| format!("Failed to read signal: {}", e))?;
        
        serde_json::from_slice(&buf)
            .map_err(|e| format!("Failed to parse signal: {}", e))
    }

    pub fn get_memory_buffer(&self) -> &Bytes {
        &self.memory_buffer
    }

    pub fn get_snapshot_buffer(&self) -> &Bytes {
        &self.snapshot_buffer
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_receiver_creation() {
        let receiver = MigrationReceiver::new("test-vm".to_string());
        assert!(receiver.memory_buffer.is_empty());
    }

    #[test]
    fn test_buffers_empty_initially() {
        let receiver = MigrationReceiver::new("test-vm".to_string());
        assert_eq!(receiver.get_memory_buffer().len(), 0);
        assert_eq!(receiver.get_snapshot_buffer().len(), 0);
    }
}
