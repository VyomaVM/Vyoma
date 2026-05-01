use std::path::{Path, PathBuf};
use tracing::{info, debug};
use std::fs;
use crate::error::{StorageError, Result};

use crate::dm::DmManager;
use crate::cow::LoopManager;
use crate::ext4::Ext4Manager;
use crate::snapshot_tree::{SnapshotTree, SnapshotNode};

pub struct StorageManager {
    base_path: PathBuf,
    pub dm: DmManager,
    pub cow: LoopManager,
    pub tree: SnapshotTree,
}

impl StorageManager {
    /// Initialize the holistic StorageManager by injecting its persistent base volume directory dynamically.
    pub fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();
        info!("Initializing StorageManager at {:?}", base_path);
        
        fs::create_dir_all(&base_path).map_err(StorageError::Io)?;
        
        let dm = DmManager::new()?;
        let cow = LoopManager::new()?;
        let tree = SnapshotTree::new(&base_path.join("metadata"))?;
        
        Ok(Self {
            base_path,
            dm,
            cow,
            tree,
        })
    }
    
    /// Create a pure standalone volume mapping without COW inheritance.
    /// Provisions ext4 natively then attaches it to a Loop device context.
    pub fn create_vm_volume(&self, vm_id: &str, capacity_mb: u64) -> Result<SnapshotNode> {
        info!("Provisioning base volume for VM {} ({}MB)", vm_id, capacity_mb);
        
        let vol_dir = self.base_path.join(vm_id);
        fs::create_dir_all(&vol_dir).map_err(StorageError::Io)?;
        
        // 1. Allocate backing sparse file
        let base_file = vol_dir.join("root.ext4");
        LoopManager::create_cow_file(&base_file, capacity_mb)?;
        
        // 2. Format ext4
        Ext4Manager::format(&base_file)?;
        
        // 3. Document in metadata Sled Tree
        let mut node = SnapshotNode::new(vm_id, None);
        node.snapshot_path = base_file.clone();
        
        self.tree.create(&node)?;
        
        Ok(node)
    }

    /// Branches an existing snapshot into a fresh COW overlay map physically injected into Devicemapper.
    pub fn branch_snapshot(&self, snap_id: &str, new_vm_id: &str, cow_capacity_mb: u64) -> Result<SnapshotNode> {
        info!("Branching snapshot {} into new VM {}", snap_id, new_vm_id);
        
        let parent = self.tree.get(snap_id)?;
        
        let new_vol_dir = self.base_path.join(new_vm_id);
        fs::create_dir_all(&new_vol_dir).map_err(StorageError::Io)?;
        
        // 1. Create sparse COW delta layer
        let cow_file = new_vol_dir.join("delta.cow");
        LoopManager::create_cow_file(&cow_file, cow_capacity_mb)?;
        
        // 2. We attach the parent root and the new cow_file to kernel loop devices
        let parent_loop = self.cow.attach(&parent.snapshot_path)?;
        let cow_loop = self.cow.attach(&cow_file)?;
        
        // 3. Assemble Snapshot natively mapping overlay Slices in kernel
        let dm_device = self.dm.create_snapshot(
            new_vm_id, 
            parent_loop.path(), 
            cow_loop.path()
        )?;
        
        debug!("Snapshot successfully mounted in mapper at {:?}", dm_device.path());
        
        // 4. Trace the inheritance in DB
        let mut child = self.tree.branch(snap_id, new_vm_id)?;
        child.snapshot_path = dm_device.path().to_path_buf();
        child.cow_delta_path = cow_file;
        child.cow_delta_size = cow_capacity_mb * 1024 * 1024;
        
        // DB branching triggers self.create internally, so we re-save the exact mutation
        self.tree.update(&child)?;
        
        Ok(child)
    }

    /// Commits an active snapshot (block device) into a fresh independent base image.
    /// This performs native block I/O rather than shelling out to dd.
    pub fn commit_snapshot(&self, snap_id: &str, new_base_name: &str) -> Result<SnapshotNode> {
        info!("Committing snapshot {} to new base image {}", snap_id, new_base_name);
        
        let node = self.tree.get(snap_id)?;
        let src_device = &node.snapshot_path;
        
        if !src_device.exists() {
            return Err(StorageError::Path(format!("Source device does not exist: {:?}", src_device)));
        }

        let new_vol_dir = self.base_path.join(new_base_name);
        fs::create_dir_all(&new_vol_dir).map_err(StorageError::Io)?;
        
        let new_base_file = new_vol_dir.join("root.ext4");
        
        // Native block I/O copy
        let mut src_file = fs::File::open(src_device).map_err(StorageError::Io)?;
        let mut dst_file = fs::File::create(&new_base_file).map_err(StorageError::Io)?;
        std::io::copy(&mut src_file, &mut dst_file).map_err(StorageError::Io)?;
        
        let mut new_node = SnapshotNode::new(new_base_name, None);
        new_node.snapshot_path = new_base_file.clone();
        
        self.tree.create(&new_node)?;
        
        Ok(new_node)
    }
}
