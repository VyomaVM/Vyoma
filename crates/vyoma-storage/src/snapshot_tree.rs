use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, error};
use serde::{Deserialize, Serialize};
use sled::Tree;

use crate::error::{StorageError, Result};

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_secs()
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotNode {
    pub id: String,
    pub vm_id: String,
    pub parent_id: Option<String>,
    pub created_at: u64,
    pub label: Option<String>,
    pub tag: Option<String>,
    pub memory_path: PathBuf,
    pub snapshot_path: PathBuf,
    pub cow_delta_path: PathBuf,
    pub cow_delta_size: u64,
    pub memory_size: u64,
}

impl SnapshotNode {
    pub fn new(vm_id: &str, parent_id: Option<String>) -> Self {
        Self {
            id: uuid::Uuid::new_v4().to_string(),
            vm_id: vm_id.to_string(),
            parent_id,
            created_at: now(),
            label: None,
            tag: None,
            memory_path: PathBuf::new(),
            snapshot_path: PathBuf::new(),
            cow_delta_path: PathBuf::new(),
            cow_delta_size: 0,
            memory_size: 0,
        }
    }
    
    pub fn with_label(mut self, label: &str) -> Self {
        self.label = Some(label.to_string());
        self
    }
    
    pub fn with_tag(mut self, tag: &str) -> Self {
        self.tag = Some(tag.to_string());
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnapshotDiff {
    pub snap_a_id: String,
    pub snap_b_id: String,
    pub changes: Vec<DiffEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DiffEntry {
    pub path: String,
    pub change_type: String, // "added", "modified", "deleted"
}

pub struct SnapshotTree {
    snapshots: Tree,
    tags: Tree,
    base_path: PathBuf,
}

impl SnapshotTree {
    pub fn new(base_path: &Path) -> Result<Self> {
        std::fs::create_dir_all(base_path)?;
        
        let db = sled::Config::new()
            .path(base_path.join("snapshots.db"))
            .mode(sled::Mode::HighThroughput)
            .open()?;
        
        let snapshots = db.open_tree("snapshots")?;
        let tags = db.open_tree("tags")?;
        
        Ok(Self {
            snapshots,
            tags,
            base_path: base_path.to_path_buf(),
        })
    }
    
    pub fn create(&self, node: &SnapshotNode) -> Result<()> {
        info!("Creating snapshot {} for VM {}", node.id, node.vm_id);
        
        let key = node.id.as_bytes();
        let value = serde_json::to_vec(node)?;
        self.snapshots.insert(key, value)?;
        self.snapshots.flush()?;
        
        // Handle tag if present
        if let Some(ref tag) = node.tag {
            let tag_key = format!("{}:{}", node.vm_id, tag);
            self.tags.insert(tag_key.as_bytes(), key)?;
            self.tags.flush()?;
        }
        
        Ok(())
    }
    
    pub fn update(&self, node: &SnapshotNode) -> Result<()> {
        info!("Updating snapshot metadata for {}", node.id);
        let key = node.id.as_bytes();
        let value = serde_json::to_vec(node).map_err(|e| StorageError::Json(e))?;
        self.snapshots.insert(key, value)?;
        self.snapshots.flush()?;
        Ok(())
    }
    
    pub fn get(&self, id: &str) -> Result<SnapshotNode> {
        let key = id.as_bytes();
        let value = self.snapshots
            .get(key)?
            .ok_or_else(|| StorageError::NotFound(format!("Snapshot {} not found", id)))?;
        
        let node: SnapshotNode = serde_json::from_slice(&value)
            .map_err(|e| StorageError::Json(e))?;
        
        Ok(node)
    }
    
    pub fn history(&self, vm_id: &str) -> Result<Vec<SnapshotNode>> {
        info!("Getting history for VM {}", vm_id);
        
        let mut nodes = Vec::new();
        
        for item in self.snapshots.iter() {
            let (_, value) = item.map_err(|e| StorageError::Io(std::io::Error::new(std::io::ErrorKind::Other, e)))?;
            let node: SnapshotNode = serde_json::from_slice(&value)
                .map_err(|e| StorageError::Json(e))?;
            
            if node.vm_id == vm_id {
                nodes.push(node);
            }
        }
        
        nodes.sort_by_key(|n| n.created_at);
        Ok(nodes)
    }
    
    pub fn branch(&self, snap_id: &str, new_vm_id: &str) -> Result<SnapshotNode> {
        info!("Branching from snapshot {} to new VM {}", snap_id, new_vm_id);
        
        let parent = self.get(snap_id)?;
        
        let new_node = SnapshotNode::new(new_vm_id, Some(snap_id.to_string()))
            .with_label(&format!("branched-from-{}", snap_id));
        
        self.create(&new_node)?;
        
        Ok(new_node)
    }
    
    pub fn diff(&self, snap_a_id: &str, snap_b_id: &str) -> Result<SnapshotDiff> {
        info!("Computing diff between {} and {}", snap_a_id, snap_b_id);
        
        let _snap_a = self.get(snap_a_id)?;
        let _snap_b = self.get(snap_b_id)?;
        
        // Placeholder: In production, mount COW layers and compute diff
        Ok(SnapshotDiff {
            snap_a_id: snap_a_id.to_string(),
            snap_b_id: snap_b_id.to_string(),
            changes: vec![],
        })
    }
    
    pub fn tag_snapshot(&self, snap_id: &str, vm_id: &str, tag: &str) -> Result<()> {
        info!("Tagging snapshot {} as {}", snap_id, tag);
        
        let mut node = self.get(snap_id)?;
        node.tag = Some(tag.to_string());
        
        let key = node.id.as_bytes();
        let value = serde_json::to_vec(&node)?;
        self.snapshots.insert(key, value)?;
        self.snapshots.flush()?;
        
        // Update tag index
        let tag_key = format!("{}:{}", vm_id, tag);
        self.tags.insert(tag_key.as_bytes(), snap_id.as_bytes())?;
        self.tags.flush()?;
        
        Ok(())
    }
    
    pub fn get_by_tag(&self, vm_id: &str, tag: &str) -> Result<Option<SnapshotNode>> {
        let tag_key = format!("{}:{}", vm_id, tag);
        
        if let Some(snap_id_bytes) = self.tags.get(tag_key.as_bytes())? {
            let snap_id = String::from_utf8_lossy(&snap_id_bytes).to_string();
            Ok(Some(self.get(&snap_id)?))
        } else {
            Ok(None)
        }
    }
    
    pub fn delete(&self, id: &str) -> Result<()> {
        info!("Deleting snapshot {}", id);
        
        let node = self.get(id)?;
        
        // Remove tag reference if tagged
        if let Some(ref tag) = node.tag {
            let tag_key = format!("{}:{}", node.vm_id, tag);
            let _ = self.tags.remove(tag_key.as_bytes());
        }
        
        self.snapshots.remove(id.as_bytes())?;
        self.snapshots.flush()?;
        
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;
    
    #[test]
    fn test_snapshot_creation() {
        let temp_dir = TempDir::new().unwrap();
        let tree = SnapshotTree::new(temp_dir.path()).unwrap();
        
        let node = SnapshotNode::new("vm-123", None)
            .with_label("test-snapshot");
        
        tree.create(&node).unwrap();
        
        let retrieved = tree.get(&node.id).unwrap();
        assert_eq!(retrieved.vm_id, "vm-123");
        assert_eq!(retrieved.label, Some("test-snapshot".to_string()));
    }
    
    #[test]
    fn test_history() {
        let temp_dir = TempDir::new().unwrap();
        let tree = SnapshotTree::new(temp_dir.path()).unwrap();
        
        for i in 0..3 {
            let node = SnapshotNode::new("vm-123", None)
                .with_label(&format!("snap-{}", i));
            tree.create(&node).unwrap();
        }
        
        let history = tree.history("vm-123").unwrap();
        assert_eq!(history.len(), 3);
    }
    
    #[test]
    fn test_tag() {
        let temp_dir = TempDir::new().unwrap();
        let tree = SnapshotTree::new(temp_dir.path()).unwrap();
        
        let node = SnapshotNode::new("vm-123", None);
        tree.create(&node).unwrap();
        
        tree.tag_snapshot(&node.id, "vm-123", "v1.0").unwrap();
        
        let found = tree.get_by_tag("vm-123", "v1.0").unwrap();
        assert!(found.is_some());
        assert_eq!(found.unwrap().id, node.id);
    }
}
