
use openraft::{
    storage::RaftLogReader, AnyError, Entry, EntryPayload, LogId, LogState, OptionalSend,
    RaftSnapshotBuilder, RaftStorage, RaftTypeConfig, Snapshot, SnapshotMeta, StorageError,
    StorageIOError, StoredMembership, Vote,
};
use std::collections::BTreeMap;
use std::fmt::Debug;
use std::ops::RangeBounds;
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::state::wal::Wal;
use crate::swarm::raft_types::{NodeId, SwarmConfig, SwarmResponse};

#[derive(Clone)]
pub struct SwarmStore {
    pub wal: Arc<Wal>,
    pub db: sled::Db,
    pub log_tree: sled::Tree,
    pub sm_tree: sled::Tree,
    pub vote_tree: sled::Tree,
}

impl SwarmStore {
    pub fn new(wal: Arc<Wal>, db: sled::Db) -> Self {
        let log_tree = db.open_tree("logs").unwrap();
        let sm_tree = db.open_tree("state_machine").unwrap();
        let vote_tree = db.open_tree("vote").unwrap();
        Self {
            wal,
            db,
            log_tree,
            sm_tree,
            vote_tree,
        }
    }
}

impl RaftLogReader<SwarmConfig> for SwarmStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        range: RB,
    ) -> Result<Vec<Entry<SwarmConfig>>, StorageError<NodeId>> {
        let mut entries = vec![];
        
        let start = match range.start_bound() {
            std::ops::Bound::Included(&s) => s,
            std::ops::Bound::Excluded(&s) => s + 1,
            std::ops::Bound::Unbounded => 0,
        };
        
        let end = match range.end_bound() {
            std::ops::Bound::Included(&e) => e,
            std::ops::Bound::Excluded(&e) => e - 1,
            std::ops::Bound::Unbounded => u64::MAX,
        };

        for id in start..=end {
            if let Ok(Some(data)) = self.log_tree.get(id.to_be_bytes()) {
                if let Ok(entry) = serde_json::from_slice::<Entry<SwarmConfig>>(&data) {
                    entries.push(entry);
                }
            } else {
                break;
            }
        }
        
        Ok(entries)
    }
}

impl RaftSnapshotBuilder<SwarmConfig> for SwarmStore {
    async fn build_snapshot(&mut self) -> Result<Snapshot<SwarmConfig>, StorageError<NodeId>> {
        Err(StorageError::IO {
            source: StorageIOError::read_state_machine(&AnyError::error("not implemented")),
        })
    }
}

impl RaftStorage<SwarmConfig> for SwarmStore {
    type LogReader = Self;
    type SnapshotBuilder = Self;

    async fn save_vote(&mut self, vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        let data = serde_json::to_vec(vote).unwrap();
        self.vote_tree.insert(b"vote", data).unwrap();
        self.vote_tree.flush().unwrap();
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        if let Ok(Some(data)) = self.vote_tree.get(b"vote") {
            if let Ok(vote) = serde_json::from_slice(&data) {
                return Ok(Some(vote));
            }
        }
        Ok(None)
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn get_log_state(&mut self) -> Result<LogState<SwarmConfig>, StorageError<NodeId>> {
        let last_log_id = if let Ok(Some((_, data))) = self.log_tree.last() {
            if let Ok(entry) = serde_json::from_slice::<Entry<SwarmConfig>>(&data) {
                Some(entry.log_id)
            } else {
                None
            }
        } else {
            None
        };

        let last_purged_log_id = if let Ok(Some(data)) = self.vote_tree.get(b"last_purged") {
            serde_json::from_slice(&data).unwrap_or(None)
        } else {
            None
        };

        Ok(LogState {
            last_purged_log_id,
            last_log_id,
        })
    }

    async fn append_to_log<I>(&mut self, entries: I) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<SwarmConfig>> + OptionalSend,
    {
        for entry in entries {
            let data = serde_json::to_vec(&entry).unwrap();
            self.log_tree.insert(entry.log_id.index.to_be_bytes(), data).unwrap();
        }
        self.log_tree.flush().unwrap();
        Ok(())
    }

    async fn delete_conflict_logs_since(
        &mut self,
        log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        let start = log_id.index;
        let mut keys_to_remove = vec![];
        for res in self.log_tree.range(start.to_be_bytes()..) {
            if let Ok((k, _)) = res {
                keys_to_remove.push(k);
            }
        }
        for k in keys_to_remove {
            self.log_tree.remove(k).unwrap();
        }
        self.log_tree.flush().unwrap();
        Ok(())
    }

    async fn purge_logs_upto(&mut self, log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
        let mut keys_to_remove = vec![];
        for res in self.log_tree.range(..=log_id.index.to_be_bytes()) {
            if let Ok((k, _)) = res {
                keys_to_remove.push(k);
            }
        }
        for k in keys_to_remove {
            self.log_tree.remove(k).unwrap();
        }
        let data = serde_json::to_vec(&Some(log_id)).unwrap();
        self.vote_tree.insert(b"last_purged", data).unwrap();
        self.log_tree.flush().unwrap();
        self.vote_tree.flush().unwrap();
        Ok(())
    }

    async fn last_applied_state(
        &mut self,
    ) -> Result<
        (
            Option<LogId<NodeId>>,
            StoredMembership<NodeId, <SwarmConfig as RaftTypeConfig>::Node>,
        ),
        StorageError<NodeId>,
    > {
        let last_applied = if let Ok(Some(data)) = self.sm_tree.get(b"last_applied") {
            serde_json::from_slice(&data).unwrap_or(None)
        } else {
            None
        };

        let membership = if let Ok(Some(data)) = self.sm_tree.get(b"membership") {
            serde_json::from_slice(&data).unwrap_or_default()
        } else {
            StoredMembership::default()
        };

        Ok((last_applied, membership))
    }

    async fn apply_to_state_machine(
        &mut self,
        entries: &[Entry<SwarmConfig>],
    ) -> Result<Vec<SwarmResponse>, StorageError<NodeId>> {
        let mut responses = vec![];

        for entry in entries {
            match entry.payload {
                EntryPayload::Blank => {
                    responses.push(SwarmResponse { success: true });
                }
                EntryPayload::Normal(ref req) => {
                    // Update state_machine tree based on SwarmRequest
                    // We also save the request itself so we know what happened
                    let key = format!("req_{}", entry.log_id.index);
                    let data = serde_json::to_vec(req).unwrap();
                    self.sm_tree.insert(key.as_bytes(), data).unwrap();
                    
                    responses.push(SwarmResponse { success: true });
                }
                EntryPayload::Membership(ref mem) => {
                    let mem_data = serde_json::to_vec(&StoredMembership::new(Some(entry.log_id), mem.clone())).unwrap();
                    self.sm_tree.insert(b"membership", mem_data).unwrap();
                    responses.push(SwarmResponse { success: true });
                }
            }

            let applied_data = serde_json::to_vec(&Some(entry.log_id)).unwrap();
            self.sm_tree.insert(b"last_applied", applied_data).unwrap();
        }

        self.sm_tree.flush().unwrap();
        Ok(responses)
    }

    async fn get_snapshot_builder(&mut self) -> Self::SnapshotBuilder {
        self.clone()
    }

    async fn begin_receiving_snapshot(
        &mut self,
    ) -> Result<Box<<SwarmConfig as RaftTypeConfig>::SnapshotData>, StorageError<NodeId>> {
        Ok(Box::new(std::io::Cursor::new(Vec::new())))
    }

    async fn install_snapshot(
        &mut self,
        _meta: &SnapshotMeta<NodeId, <SwarmConfig as RaftTypeConfig>::Node>,
        _snapshot: Box<<SwarmConfig as RaftTypeConfig>::SnapshotData>,
    ) -> Result<(), StorageError<NodeId>> {
        Ok(())
    }

    async fn get_current_snapshot(
        &mut self,
    ) -> Result<Option<Snapshot<SwarmConfig>>, StorageError<NodeId>> {
        Ok(None)
    }
}
