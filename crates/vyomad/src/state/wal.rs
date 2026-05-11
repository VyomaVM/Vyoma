use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use sled::{Db, Tree};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};
use tracing::{info, error, warn};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum WalEntry {
    VmCreate {
        id: String,
        timestamp: u64,
    },
    VmStart {
        id: String,
        timestamp: u64,
    },
    VmStop {
        id: String,
        timestamp: u64,
    },
    VmDestroy {
        id: String,
        timestamp: u64,
    },
    VmCheckpoint {
        id: String,
        snapshot_path: String,
        timestamp: u64,
    },
}

impl WalEntry {
    pub fn vm_create(id: String) -> Self {
        Self::VmCreate {
            id,
            timestamp: now(),
        }
    }

    pub fn vm_start(id: String) -> Self {
        Self::VmStart {
            id,
            timestamp: now(),
        }
    }

    pub fn vm_stop(id: String) -> Self {
        Self::VmStop {
            id,
            timestamp: now(),
        }
    }

    pub fn vm_destroy(id: String) -> Self {
        Self::VmDestroy {
            id,
            timestamp: now(),
        }
    }

    pub fn vm_checkpoint(id: String, snapshot_path: String) -> Self {
        Self::VmCheckpoint {
            id,
            snapshot_path,
            timestamp: now(),
        }
    }

    pub fn vm_id(&self) -> Option<&str> {
        match self {
            Self::VmCreate { id, .. } => Some(id),
            Self::VmStart { id, .. } => Some(id),
            Self::VmStop { id, .. } => Some(id),
            Self::VmDestroy { id, .. } => Some(id),
            Self::VmCheckpoint { id, .. } => Some(id),
        }
    }
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos() as u64
}

pub struct Wal {
    tree: Tree,
    vm_state: Tree,
}

impl Wal {
    pub fn new(db: &Db) -> Result<Self> {
        let tree = db.open_tree("wal")?;
        let vm_state = db.open_tree("vm_state")?;
        Ok(Self { tree, vm_state })
    }

    pub fn new_test() -> Self {
        let db = sled::Config::new()
            .temporary(true)
            .open()
            .expect("Failed to create test DB");
        Self::new(&db).expect("Failed to create test Wal")
    }

    pub fn open_or_create(path: &Path) -> Result<(Db, Self)> {
        std::fs::create_dir_all(path)?;
        
        let db = sled::Config::new()
            .path(path.join("vyoma.db"))
            .mode(sled::Mode::HighThroughput)
            .open()?;
        
        let wal = Self::new(&db)?;
        Ok((db, wal))
    }

    pub fn append(&self, entry: &WalEntry) -> Result<()> {
        let key = format!("{}:{}", now(), entry.vm_id().unwrap_or("unknown"));
        let value = serde_json::to_vec(entry)?;
        
        self.tree.insert(key.as_bytes(), value)?;
        self.tree.flush()?;
        
        info!("WAL append: {:?}", entry);
        Ok(())
    }

    pub fn save_vm_state(&self, id: &str, state: &[u8]) -> Result<()> {
        self.vm_state.insert(id.as_bytes(), state.to_vec())?;
        self.vm_state.flush()?;
        Ok(())
    }

    pub fn get_vm_state(&self, id: &str) -> Result<Option<Vec<u8>>> {
        Ok(self.vm_state.get(id.as_bytes())?.map(|v| v.to_vec()))
    }

    pub fn remove_vm_state(&self, id: &str) -> Result<()> {
        self.vm_state.remove(id.as_bytes())?;
        self.vm_state.flush()?;
        Ok(())
    }

    pub fn iterate_wal(&self) -> impl Iterator<Item = (String, WalEntry)> {
        self.tree.iter()
            .flat_map(|r| r.ok())
            .map(|(k, v)| {
                let key = String::from_utf8_lossy(&k).to_string();
                let entry: WalEntry = serde_json::from_slice(&v).unwrap_or_else(|_| {
                    error!("Failed to parse WAL entry: {:?}", v);
                    WalEntry::VmCreate { id: "parse_error".to_string(), timestamp: 0 }
                });
                (key, entry)
            })
    }

    pub fn get_vm_entries(&self, vm_id: &str) -> Vec<WalEntry> {
        self.iterate_wal()
            .filter_map(|(key, entry)| {
                if entry.vm_id() == Some(vm_id) {
                    Some(entry)
                } else {
                    None
                }
            })
            .collect()
    }
}
