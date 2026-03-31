use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use tracing::info;
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
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
    snapshots: BTreeMap<String, Vec<SnapshotEntry>>,
}

impl TimeMachine {
    pub fn new() -> Self {
        Self {
            snapshots: BTreeMap::new(),
        }
    }

    pub fn create_snapshot(&mut self, vm_id: String, label: Option<String>) -> SnapshotEntry {
        let snapshots = self.snapshots.entry(vm_id.clone()).or_insert_with(Vec::new);

        let parent_id = snapshots.last().map(|s| s.id.clone());

        let entry = SnapshotEntry::new(vm_id, label, parent_id);

        info!("Created snapshot {} for VM {}", entry.id, entry.vm_id);

        snapshots.push(entry.clone());

        entry
    }

    pub fn get_snapshot_history(&self, vm_id: &str) -> Option<Vec<SnapshotEntry>> {
        self.snapshots.get(vm_id).cloned()
    }

    pub fn get_snapshot(&self, vm_id: &str, snapshot_id: &str) -> Option<SnapshotEntry> {
        self.snapshots
            .get(vm_id)
            .and_then(|snaps| snaps.iter().find(|s| s.id == snapshot_id))
            .cloned()
    }

    pub fn get_latest_snapshot(&self, vm_id: &str) -> Option<SnapshotEntry> {
        self.snapshots
            .get(vm_id)
            .and_then(|snaps| snaps.last())
            .cloned()
    }

    pub fn delete_snapshot(&mut self, vm_id: &str, snapshot_id: &str) -> Result<(), String> {
        let snapshots = self.snapshots.get_mut(vm_id).ok_or("VM not found")?;

        let index = snapshots
            .iter()
            .position(|s| s.id == snapshot_id)
            .ok_or("Snapshot not found")?;

        snapshots.remove(index);

        if let Some(next) = snapshots.get(index) {
            let next = next.clone();
            let next_next = snapshots.get(index + 1).cloned();
            if let Some(mut np) = next_next {
                np.parent_id = Some(next.id);
                snapshots[index] = np;
            }
        }

        info!("Deleted snapshot {} for VM {}", snapshot_id, vm_id);

        Ok(())
    }

    pub fn list_all_vms(&self) -> Vec<String> {
        self.snapshots.keys().cloned().collect()
    }

    pub fn get_snapshot_count(&self, vm_id: &str) -> usize {
        self.snapshots.get(vm_id).map(|s| s.len()).unwrap_or(0)
    }
}

impl Default for TimeMachine {
    fn default() -> Self {
        Self::new()
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

    #[test]
    fn test_create_snapshot() {
        let mut tm = TimeMachine::new();
        let entry = tm.create_snapshot("vm-1".to_string(), Some("initial".to_string()));

        assert_eq!(entry.vm_id, "vm-1");
        assert_eq!(entry.label, Some("initial".to_string()));
        assert!(entry.parent_id.is_none());
    }

    #[test]
    fn test_snapshot_chain() {
        let mut tm = TimeMachine::new();

        let snap1 = tm.create_snapshot("vm-1".to_string(), Some("snap1".to_string()));
        let snap2 = tm.create_snapshot("vm-1".to_string(), Some("snap2".to_string()));
        let snap3 = tm.create_snapshot("vm-1".to_string(), Some("snap3".to_string()));

        assert_eq!(snap2.parent_id, Some(snap1.id));
        assert_eq!(snap3.parent_id, Some(snap2.id));
    }

    #[test]
    fn test_get_snapshot_history() {
        let mut tm = TimeMachine::new();
        tm.create_snapshot("vm-1".to_string(), Some("first".to_string()));
        tm.create_snapshot("vm-1".to_string(), Some("second".to_string()));

        let history = tm.get_snapshot_history("vm-1").unwrap();
        assert_eq!(history.len(), 2);
    }

    #[test]
    fn test_get_latest_snapshot() {
        let mut tm = TimeMachine::new();
        tm.create_snapshot("vm-1".to_string(), Some("first".to_string()));
        tm.create_snapshot("vm-1".to_string(), Some("second".to_string()));

        let latest = tm.get_latest_snapshot("vm-1").unwrap();
        assert_eq!(latest.label, Some("second".to_string()));
    }

    #[test]
    fn test_delete_snapshot() {
        let mut tm = TimeMachine::new();
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
        let mut tm = TimeMachine::new();
        tm.create_snapshot("vm-1".to_string(), None);
        tm.create_snapshot("vm-2".to_string(), None);

        let vms = tm.list_all_vms();
        assert_eq!(vms.len(), 2);
    }

    #[test]
    fn test_get_snapshot_count() {
        let mut tm = TimeMachine::new();
        assert_eq!(tm.get_snapshot_count("vm-1"), 0);

        tm.create_snapshot("vm-1".to_string(), None);
        tm.create_snapshot("vm-1".to_string(), None);

        assert_eq!(tm.get_snapshot_count("vm-1"), 2);
    }
}
