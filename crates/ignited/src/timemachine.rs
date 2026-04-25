use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use tracing::{error, info};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct SnapshotEntry {
    pub id: String,
    pub vm_id: String,
    pub created_at: DateTime<Utc>,
    pub cow_delta_size: u64,
    pub label: Option<String>,
    pub parent_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotHistory {
    pub vm_id: String,
    pub snapshots: Vec<SnapshotEntry>,
}

impl SnapshotEntry {
    pub fn new(vm_id: String, label: Option<String>, parent_id: Option<String>) -> Self {
        Self {
            id: Uuid::new_v4().to_string(),
            vm_id,
            created_at: Utc::now(),
            cow_delta_size: 0,
            label,
            parent_id,
        }
    }

    pub fn with_size(mut self, size: u64) -> Self {
        self.cow_delta_size = size;
        self
    }
}

pub struct TimeMachine {
    tree: sled::Tree,
}

impl TimeMachine {
    pub fn new(db: &sled::Db) -> Self {
        let tree = db.open_tree("timemachine_tree").expect("Failed to open timemachine tree");
        Self { tree }
    }

    fn get_snapshots(&self, vm_id: &str) -> Vec<SnapshotEntry> {
        if let Ok(Some(bytes)) = self.tree.get(vm_id) {
            serde_json::from_slice(&bytes).unwrap_or_default()
        } else {
            Vec::new()
        }
    }

    fn save_snapshots(&self, vm_id: &str, snapshots: &Vec<SnapshotEntry>) {
        if let Ok(bytes) = serde_json::to_vec(snapshots) {
            let _ = self.tree.insert(vm_id, bytes);
            let _ = self.tree.flush();
        }
    }

    pub fn create_snapshot(&self, vm_id: String, label: Option<String>) -> SnapshotEntry {
        let mut snapshots = self.get_snapshots(&vm_id);

        let parent_id = snapshots.last().map(|s| s.id.clone());
        let entry = SnapshotEntry::new(vm_id.clone(), label, parent_id);

        info!("Created snapshot {} for VM {}", entry.id, entry.vm_id);

        snapshots.push(entry.clone());
        self.save_snapshots(&vm_id, &snapshots);

        entry
    }

    pub fn get_snapshot_history(&self, vm_id: &str) -> Option<Vec<SnapshotEntry>> {
        let snaps = self.get_snapshots(vm_id);
        if snaps.is_empty() { None } else { Some(snaps) }
    }

    pub fn get_snapshot(&self, vm_id: &str, snapshot_id: &str) -> Option<SnapshotEntry> {
        self.get_snapshots(vm_id)
            .into_iter()
            .find(|s| s.id == snapshot_id)
    }

    pub fn get_latest_snapshot(&self, vm_id: &str) -> Option<SnapshotEntry> {
        self.get_snapshots(vm_id).into_iter().last()
    }

    pub fn delete_snapshot(&self, vm_id: &str, snapshot_id: &str) -> Result<(), String> {
        let mut snapshots = self.get_snapshots(vm_id);
        if snapshots.is_empty() {
            return Err("VM not found".to_string());
        }

        let index = snapshots
            .iter()
            .position(|s| s.id == snapshot_id)
            .ok_or("Snapshot not found")?;

        snapshots.remove(index);

        if let Some(next) = snapshots.get(index).cloned() {
            let next_next = snapshots.get(index + 1).cloned();
            if let Some(mut np) = next_next {
                np.parent_id = Some(next.id);
                snapshots[index] = np;
            }
        }

        self.save_snapshots(vm_id, &snapshots);
        info!("Deleted snapshot {} for VM {}", snapshot_id, vm_id);

        Ok(())
    }

    pub fn list_all_vms(&self) -> Vec<String> {
        self.tree.iter().filter_map(|res| {
            res.ok().map(|(k, _)| String::from_utf8_lossy(&k).to_string())
        }).collect()
    }

    pub fn get_snapshot_count(&self, vm_id: &str) -> usize {
        self.get_snapshots(vm_id).len()
    }
}

pub fn parse_snapshot_ref(reference: &str) -> Result<usize, String> {
    reference
        .strip_prefix("snap:")
        .ok_or_else(|| "Invalid snapshot ref. Use snap:N".to_string())?
        .parse()
        .map_err(|_| "Invalid snapshot index".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    fn test_tm() -> TimeMachine {
        let dir = tempdir().unwrap();
        let db = sled::open(dir.path()).unwrap();
        TimeMachine::new(&db)
    }

    #[test]
    fn test_create_snapshot() {
        let tm = test_tm();
        let entry = tm.create_snapshot("vm-1".to_string(), Some("initial".to_string()));

        assert_eq!(entry.vm_id, "vm-1");
        assert_eq!(entry.label, Some("initial".to_string()));
        assert!(entry.parent_id.is_none());
    }

    #[test]
    fn test_snapshot_chain() {
        let tm = test_tm();

        let snap1 = tm.create_snapshot("vm-1".to_string(), Some("snap1".to_string()));
        let snap2 = tm.create_snapshot("vm-1".to_string(), Some("snap2".to_string()));
        let snap3 = tm.create_snapshot("vm-1".to_string(), Some("snap3".to_string()));

        assert_eq!(snap2.parent_id, Some(snap1.id));
        assert_eq!(snap3.parent_id, Some(snap2.id));
    }

    #[test]
    fn test_get_snapshot_history() {
        let tm = test_tm();
        tm.create_snapshot("vm-1".to_string(), Some("first".to_string()));
        tm.create_snapshot("vm-1".to_string(), Some("second".to_string()));

        let history = tm.get_snapshot_history("vm-1").unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_get_latest_snapshot() {
        let tm = test_tm();
        tm.create_snapshot("vm-1".to_string(), Some("first".to_string()));
        tm.create_snapshot("vm-1".to_string(), Some("second".to_string()));

        let latest = tm.get_latest_snapshot("vm-1").unwrap();
        assert_eq!(latest.label, Some("second".to_string()));
    }

    #[test]
    fn test_delete_snapshot() {
        let tm = test_tm();
        let snap1 = tm.create_snapshot("vm-1".to_string(), Some("first".to_string()));
        tm.create_snapshot("vm-1".to_string(), Some("second".to_string()));

        tm.delete_snapshot("vm-1", &snap1.id).unwrap();

        let history = tm.get_snapshot_history("vm-1").unwrap();
        assert_eq!(history.len(), 1);
    }

    #[test]
    fn test_parse_snapshot_ref() {
        assert_eq!(parse_snapshot_ref("snap:0").unwrap(), 0);
        assert_eq!(parse_snapshot_ref("snap:5").unwrap(), 5);
    }

    #[test]
    fn test_parse_invalid_ref() {
        assert!(parse_snapshot_ref("invalid").is_err());
        assert!(parse_snapshot_ref("snap:abc").is_err());
    }

    #[test]
    fn test_snapshot_with_size() {
        let entry = SnapshotEntry::new("vm-1".to_string(), None, None).with_size(1024000);

        assert_eq!(entry.cow_delta_size, 1024000);
    }

    #[test]
    fn test_list_all_vms() {
        let tm = test_tm();
        tm.create_snapshot("vm-1".to_string(), None);
        tm.create_snapshot("vm-2".to_string(), None);

        let vms = tm.list_all_vms();
        assert_eq!(vms.len(), 2);
    }

    #[test]
    fn test_get_snapshot_count() {
        let tm = test_tm();
        assert_eq!(tm.get_snapshot_count("vm-1"), 0);

        tm.create_snapshot("vm-1".to_string(), None);
        tm.create_snapshot("vm-1".to_string(), None);

        assert_eq!(tm.get_snapshot_count("vm-1"), 2);
    }
}
