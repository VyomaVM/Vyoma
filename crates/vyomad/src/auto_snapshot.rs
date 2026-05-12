use std::sync::Arc;
use std::collections::BTreeMap;
use tokio::sync::{RwLock, watch};
use tokio::time::{interval, Duration};
use tracing::{info, warn};

use crate::timemachine::{SnapshotEntry, TimeMachine};

#[derive(Clone)]
pub struct AutoSnapshotConfig {
    pub vm_id: String,
    pub interval: Duration,
    pub retain_count: usize,
    pub label: Option<String>,
}

pub struct AutoSnapshotManager {
    tasks: Arc<RwLock<BTreeMap<String, watch::Sender<bool>>>>,
}

impl AutoSnapshotManager {
    pub fn new() -> Self {
        Self {
            tasks: Arc::new(RwLock::new(BTreeMap::new())),
        }
    }

    pub async fn start_task(
        &self,
        config: AutoSnapshotConfig,
        timemachine: Arc<RwLock<TimeMachine>>,
    ) -> Result<(), String> {
        let vm_id = config.vm_id.clone();
        
        let mut tasks = self.tasks.write().await;
        
        if tasks.contains_key(&vm_id) {
            return Err(format!("Auto-snapshot task already running for VM {}", vm_id));
        }

        let (stop_tx, stop_rx) = watch::channel(false);
        
        tasks.insert(vm_id.clone(), stop_tx);

        let tasks_map = Arc::clone(&self.tasks);
        let vm_id_clone = vm_id.clone();
        let vm_id_for_spawn = vm_id.clone();
        let interval = config.interval;
        let retain_count = config.retain_count;
        let label = config.label;
        
        tokio::spawn(async move {
            auto_snapshot_loop(
                vm_id_clone,
                interval,
                retain_count,
                label,
                stop_rx,
                timemachine,
                tasks_map,
            ).await;
            
            info!("Auto-snapshot task completed for VM {}", vm_id_for_spawn);
        });

        info!("Started auto-snapshot task for VM {}", vm_id);
        
        Ok(())
    }

    pub async fn stop_task(&self, vm_id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(sender) = tasks.remove(vm_id) {
            drop(sender);
            info!("Stopped auto-snapshot task for VM {}", vm_id);
            Ok(())
        } else {
            Err(format!("No auto-snapshot task found for VM {}", vm_id))
        }
    }

    pub async fn is_running(&self, vm_id: &str) -> bool {
        let tasks = self.tasks.read().await;
        tasks.contains_key(vm_id)
    }

    pub async fn list_running(&self) -> Vec<String> {
        let tasks = self.tasks.read().await;
        tasks.keys().cloned().collect()
    }
}

impl Default for AutoSnapshotManager {
    fn default() -> Self {
        Self::new()
    }
}

async fn auto_snapshot_loop(
    vm_id: String,
    interval_duration: Duration,
    retain_count: usize,
    label: Option<String>,
    mut stop_rx: watch::Receiver<bool>,
    timemachine: Arc<RwLock<TimeMachine>>,
    manager_tasks: Arc<RwLock<BTreeMap<String, watch::Sender<bool>>>>,
) {
    let mut ticker = interval(interval_duration);
    let vm_id_for_log = vm_id.clone();
    
    loop {
        tokio::select! {
            _ = ticker.tick() => {
                let snapshot_label = label.clone().unwrap_or_else(|| {
                    format!("auto-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"))
                });

                let mut tm = timemachine.write().await;
                let _snapshot = tm.create_snapshot(vm_id.clone(), Some(snapshot_label));

                let count = tm.get_snapshot_count(&vm_id);
                if count > retain_count {
                    let history = tm.get_snapshot_history(&vm_id).unwrap();
                    if let Some(oldest) = history.first() {
                        let _ = tm.delete_snapshot(&vm_id, &oldest.id);
                        info!(
                            "Pruned old snapshot {} for VM {}, {} remaining",
                            oldest.id, vm_id, count - 1
                        );
                    }
                }

                info!("Auto-snapshot completed for VM {}", vm_id_for_log);
            }
            result = stop_rx.changed() => {
                if result.is_err() {
                    break;
                }
                if *stop_rx.borrow() {
                    break;
                }
            }
        }
    }

    info!("Auto-snapshot task stopped for VM {}", vm_id);
    
    let mut tasks = manager_tasks.write().await;
    tasks.remove(&vm_id);
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_auto_snapshot_config() {
        let config = AutoSnapshotConfig {
            vm_id: "vm-1".to_string(),
            interval: Duration::from_secs(3600),
            retain_count: 10,
            label: Some("hourly".to_string()),
        };

        assert_eq!(config.vm_id, "vm-1");
        assert_eq!(config.interval.as_secs(), 3600);
        assert_eq!(config.retain_count, 10);
    }

    #[tokio::test]
    async fn test_manager_creation() {
        let manager = AutoSnapshotManager::new();
        let running = manager.list_running().await;
        assert!(running.is_empty());
    }

    #[tokio::test]
    async fn test_manager_task_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let timemachine = Arc::new(RwLock::new(TimeMachine::new(&db)));
        let manager = AutoSnapshotManager::new();

        let config = AutoSnapshotConfig {
            vm_id: "vm-1".to_string(),
            interval: Duration::from_millis(50),
            retain_count: 3,
            label: Some("test".to_string()),
        };

        manager.start_task(config, timemachine).await.unwrap();
        
        tokio::time::sleep(Duration::from_millis(100)).await;
        
        assert!(manager.is_running("vm-1").await);
        
        manager.stop_task("vm-1").await.unwrap();
        
        tokio::time::sleep(Duration::from_millis(50)).await;
        
        assert!(!manager.is_running("vm-1").await);
    }

    #[tokio::test]
    async fn test_duplicate_task_prevention() {
        let dir = tempfile::tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let timemachine = Arc::new(RwLock::new(TimeMachine::new(&db)));
        let manager = AutoSnapshotManager::new();

        let config = AutoSnapshotConfig {
            vm_id: "vm-1".to_string(),
            interval: Duration::from_secs(60),
            retain_count: 3,
            label: None,
        };

        manager.start_task(config.clone(), Arc::clone(&timemachine)).await.unwrap();
        let result = manager.start_task(config, timemachine).await;
        
        assert!(result.is_err());
        
        manager.stop_task("vm-1").await.unwrap();
    }

    #[tokio::test]
    async fn test_rapid_start_stop_start() {
        let dir = tempfile::tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        let timemachine = Arc::new(RwLock::new(TimeMachine::new(&db)));
        let manager = AutoSnapshotManager::new();

        let config = AutoSnapshotConfig {
            vm_id: "vm-1".to_string(),
            interval: Duration::from_millis(50),
            retain_count: 3,
            label: Some("rapid".to_string()),
        };

        manager.start_task(config.clone(), Arc::clone(&timemachine)).await.unwrap();
        
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        manager.stop_task("vm-1").await.unwrap();
        
        tokio::time::sleep(Duration::from_millis(20)).await;
        
        let result = manager.start_task(config, timemachine).await;
        assert!(result.is_ok());
        
        assert!(manager.is_running("vm-1").await);
        
        manager.stop_task("vm-1").await.unwrap();
    }
}