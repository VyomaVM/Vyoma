use std::sync::Arc;
use tokio::sync::RwLock;
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

pub struct AutoSnapshotTask {
    vm_id: String,
    interval: Duration,
    retain_count: usize,
    label: Option<String>,
    running: Arc<RwLock<bool>>,
}

impl AutoSnapshotTask {
    pub fn new(config: AutoSnapshotConfig) -> Self {
        Self {
            vm_id: config.vm_id,
            interval: config.interval,
            retain_count: config.retain_count,
            label: config.label,
            running: Arc::new(RwLock::new(false)),
        }
    }

    pub async fn start(&self, timemachine: Arc<RwLock<TimeMachine>>) {
        let mut is_running = self.running.write().await;
        if *is_running {
            warn!("Auto-snapshot task already running for VM {}", self.vm_id);
            return;
        }
        *is_running = true;
        drop(is_running);

        info!(
            "Starting auto-snapshot task for VM {} with interval {:?}, retain {}",
            self.vm_id, self.interval, self.retain_count
        );

        let mut ticker = interval(self.interval);
        ticker.tick().await;

        loop {
            ticker.tick().await;

            let should_stop = *self.running.read().await;
            if !should_stop {
                break;
            }

            let label = self.label.clone().unwrap_or_else(|| {
                format!("auto-{}", chrono::Utc::now().format("%Y%m%d-%H%M%S"))
            });

            let mut tm = timemachine.write().await;
            let _snapshot = tm.create_snapshot(self.vm_id.clone(), Some(label));

            let count = tm.get_snapshot_count(&self.vm_id);
            if count > self.retain_count {
                let history = tm.get_snapshot_history(&self.vm_id).unwrap();
                if let Some(oldest) = history.first() {
                    let _ = tm.delete_snapshot(&self.vm_id, &oldest.id);
                    info!(
                        "Pruned old snapshot {} for VM {}, {} remaining",
                        oldest.id, self.vm_id, count - 1
                    );
                }
            }

            info!("Auto-snapshot completed for VM {}", self.vm_id);
        }

        info!("Auto-snapshot task stopped for VM {}", self.vm_id);
    }

    pub async fn stop(&self) {
        info!("Stopping auto-snapshot task for VM {}", self.vm_id);
        let mut is_running = self.running.write().await;
        *is_running = false;
    }

    pub fn vm_id(&self) -> &str {
        &self.vm_id
    }
}

pub struct AutoSnapshotManager {
    tasks: Arc<RwLock<BTreeMap<String, AutoSnapshotTask>>>,
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

        let task = AutoSnapshotTask::new(config);
        let task_vm_id = task.vm_id().to_string();
        
        tasks.insert(vm_id.clone(), task);

        let tasks = Arc::clone(&self.tasks);
        tokio::spawn(async move {
            let task = tasks.write().await.remove(&task_vm_id);
            if let Some(t) = task {
                t.start(timemachine).await;
            }
        });

        info!("Started auto-snapshot manager for VM {}", vm_id);
        
        Ok(())
    }

    pub async fn stop_task(&self, vm_id: &str) -> Result<(), String> {
        let mut tasks = self.tasks.write().await;
        
        if let Some(task) = tasks.remove(vm_id) {
            task.stop().await;
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

use std::collections::BTreeMap;

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
    async fn test_task_creation() {
        let config = AutoSnapshotConfig {
            vm_id: "vm-1".to_string(),
            interval: Duration::from_secs(60),
            retain_count: 5,
            label: None,
        };

        let task = AutoSnapshotTask::new(config);
        assert_eq!(task.vm_id(), "vm-1");
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
            interval: Duration::from_secs(60),
            retain_count: 3,
            label: Some("test".to_string()),
        };

        manager.start_task(config, timemachine).await.unwrap();
        
        assert!(manager.is_running("vm-1").await);
        
        manager.stop_task("vm-1").await.unwrap();
        
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
}
