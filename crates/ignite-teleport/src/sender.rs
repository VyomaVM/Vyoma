use std::net::SocketAddr;
use serde::{Deserialize, Serialize};
use tracing::info;
use bytes::Bytes;
use bitvec::prelude::BitVec;
use tokio::io::{AsyncWriteExt, AsyncReadExt};

const MIGRATION_THRESHOLD_PAGES: u64 = 1000;
const PAGE_SIZE: u64 = 4096;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationStats {
    pub rounds: u32,
    pub total_pages: u64,
    pub bytes_transferred: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum MigrationSignal {
    Start,
    Resume,
    Pause,
    Complete,
    Failed(String),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MigrationHeader {
    pub vm_id: String,
    pub memory_bytes: u64,
    pub snapshot_size: u64,
}

pub struct MigrationSender {
    vm_id: String,
    memory_pages: u64,
}

impl MigrationSender {
    pub fn new(vm_id: String, memory_mb: u64) -> Self {
        let memory_pages = (memory_mb * 1024 * 1024) / PAGE_SIZE;
        info!("Creating MigrationSender for VM {} with {} pages", vm_id, memory_pages);
        
        Self {
            vm_id,
            memory_pages,
        }
    }

    pub async fn migrate(
        &self,
        dest_addr: SocketAddr,
    ) -> Result<MigrationStats, String> {
        info!("Starting migration of VM {} to {}", self.vm_id, dest_addr);
        
        let mut stream = tokio::net::TcpStream::connect(dest_addr)
            .await
            .map_err(|e| format!("Failed to connect to destination: {}", e))?;
        
        let mut total_bytes: u64 = 0;
        let mut rounds: u32 = 0;

        let header = MigrationHeader {
            vm_id: self.vm_id.clone(),
            memory_bytes: self.memory_pages * PAGE_SIZE,
            snapshot_size: 0,
        };
        
        let header_bytes = serde_json::to_vec(&header)
            .map_err(|e| format!("Failed to serialize header: {}", e))?;
        
        stream.write_all(&(header_bytes.len() as u32).to_be_bytes()).await
            .map_err(|e| format!("Failed to send header length: {}", e))?;
        stream.write_all(&header_bytes).await
            .map_err(|e| format!("Failed to send header: {}", e))?;
        
        info!("Phase 1: Sending initial bulk transfer");
        let initial_pages = self.simulate_get_all_pages()?;
        let page_data = self.simulate_copy_pages(initial_pages);
        total_bytes += page_data.len() as u64;
        
        stream.write_all(&(page_data.len() as u64).to_be_bytes()).await
            .map_err(|e| format!("Failed to send page count: {}", e))?;
        stream.write_all(&page_data).await
            .map_err(|e| format!("Failed to send pages: {}", e))?;
        
        info!("Phase 2: Iterative dirty page transfer");
        let mut dirty_rate = MIGRATION_THRESHOLD_PAGES + 1;
        
        while dirty_rate > MIGRATION_THRESHOLD_PAGES {
            rounds += 1;
            let dirty_pages = self.simulate_get_dirty_pages()?;
            dirty_rate = dirty_pages.count_ones() as u64;
            
            info!("Migration round {}: {} dirty pages", rounds, dirty_rate);
            
            if dirty_rate == 0 {
                break;
            }
            
            let dirty_data = self.simulate_copy_dirty_pages(dirty_pages);
            total_bytes += dirty_data.len() as u64;
            
            stream.write_all(&(dirty_data.len() as u64).to_be_bytes()).await
                .map_err(|e| format!("Failed to send dirty pages: {}", e))?;
            stream.write_all(&dirty_data).await
                .map_err(|e| format!("Failed to send dirty data: {}", e))?;
        }
        
        info!("Phase 3: Final pause and transfer");
        let final_signal = MigrationSignal::Pause;
        let signal_bytes = serde_json::to_vec(&final_signal)
            .map_err(|e| format!("Failed to serialize signal: {}", e))?;
        stream.write_all(signal_bytes.as_slice()).await
            .map_err(|e| format!("Failed to send pause signal: {}", e))?;
        
        let final_pages = self.simulate_get_dirty_pages()?;
        let final_data = self.simulate_copy_dirty_pages(final_pages);
        total_bytes += final_data.len() as u64;
        
        stream.write_all(&(final_data.len() as u64).to_be_bytes()).await
            .map_err(|e| format!("Failed to send final pages: {}", e))?;
        stream.write_all(&final_data).await
            .map_err(|e| format!("Failed to send final data: {}", e))?;
        
        let snapshot_data = self.simulate_create_snapshot();
        total_bytes += snapshot_data.len() as u64;
        
        stream.write_all(&(snapshot_data.len() as u64).to_be_bytes()).await
            .map_err(|e| format!("Failed to send snapshot size: {}", e))?;
        stream.write_all(&snapshot_data).await
            .map_err(|e| format!("Failed to send snapshot: {}", e))?;
        
        let resume_signal = MigrationSignal::Resume;
        let resume_bytes = serde_json::to_vec(&resume_signal)
            .map_err(|e| format!("Failed to serialize resume: {}", e))?;
        stream.write_all(resume_bytes.as_slice()).await
            .map_err(|e| format!("Failed to send resume signal: {}", e))?;
        
        info!("Migration complete: {} rounds, {} bytes", rounds, total_bytes);
        
        Ok(MigrationStats {
            rounds: rounds + 1,
            total_pages: self.memory_pages,
            bytes_transferred: total_bytes,
        })
    }

    fn simulate_get_all_pages(&self) -> Result<Vec<u64>, String> {
        Ok((0..self.memory_pages).collect())
    }

    fn simulate_get_dirty_pages(&self) -> Result<BitVec, String> {
        let mut dirty = bitvec::bitvec![0; self.memory_pages as usize];
        let dirty_count = (self.memory_pages / 10).min(100) as usize;
        for i in 0..dirty_count {
            let idx = (i * 137) % self.memory_pages as usize;
            dirty.set(idx, true);
        }
        Ok(dirty)
    }

    fn simulate_copy_pages(&self, _pages: Vec<u64>) -> Bytes {
        let size = (self.memory_pages * PAGE_SIZE) as usize;
        Bytes::from(vec![0u8; size])
    }

    fn simulate_copy_dirty_pages(&self, _dirty: BitVec) -> Bytes {
        let size = (self.memory_pages * PAGE_SIZE / 10).min(40960) as usize;
        Bytes::from(vec![0u8; size])
    }

    fn simulate_create_snapshot(&self) -> Bytes {
        let size = 1024 * 1024;
        Bytes::from(vec![0u8; size])
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::net::TcpListener;
    use tokio::spawn;
    use std::time::Duration;

    #[tokio::test]
    async fn test_migration_sender_creation() {
        let sender = MigrationSender::new("test-vm".to_string(), 256);
        assert_eq!(sender.memory_pages, 65536);
    }

    #[tokio::test]
    async fn test_migration_stats() {
        let stats = MigrationStats {
            rounds: 5,
            total_pages: 65536,
            bytes_transferred: 268435456,
        };
        assert_eq!(stats.rounds, 5);
        assert_eq!(stats.total_pages, 65536);
    }

    #[tokio::test]
    async fn test_migration_signal_serialization() {
        let signal = MigrationSignal::Resume;
        let bytes = serde_json::to_vec(&signal).unwrap();
        let decoded: MigrationSignal = serde_json::from_slice(&bytes).unwrap();
        assert!(matches!(decoded, MigrationSignal::Resume));
    }

    #[tokio::test]
    async fn test_dirty_page_tracking() {
        let sender = MigrationSender::new("test-vm".to_string(), 256);
        let dirty = sender.simulate_get_dirty_pages().unwrap();
        assert!(dirty.count_ones() > 0);
    }
}
