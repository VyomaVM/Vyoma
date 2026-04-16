
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
    pub state_machine: Arc<RwLock<BTreeMap<String, String>>>,
    pub current_snapshot: Arc<RwLock<Option<u64>>>,
}

impl SwarmStore {
    pub fn new(wal: Arc<Wal>) -> Self {
        Self {
            wal,
            state_machine: Arc::new(RwLock::new(BTreeMap::new())),
            current_snapshot: Arc::new(RwLock::new(None)),
        }
    }
}

impl RaftLogReader<SwarmConfig> for SwarmStore {
    async fn try_get_log_entries<RB: RangeBounds<u64> + Clone + Debug + OptionalSend>(
        &mut self,
        _range: RB,
    ) -> Result<Vec<Entry<SwarmConfig>>, StorageError<NodeId>> {
        Ok(vec![])
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

    async fn save_vote(&mut self, _vote: &Vote<NodeId>) -> Result<(), StorageError<NodeId>> {
        Ok(())
    }

    async fn read_vote(&mut self) -> Result<Option<Vote<NodeId>>, StorageError<NodeId>> {
        Ok(None)
    }

    async fn get_log_reader(&mut self) -> Self::LogReader {
        self.clone()
    }

    async fn get_log_state(&mut self) -> Result<LogState<SwarmConfig>, StorageError<NodeId>> {
        Ok(LogState {
            last_purged_log_id: None,
            last_log_id: None,
        })
    }

    async fn append_to_log<I>(&mut self, _entries: I) -> Result<(), StorageError<NodeId>>
    where
        I: IntoIterator<Item = Entry<SwarmConfig>> + OptionalSend,
    {
        Ok(())
    }

    async fn delete_conflict_logs_since(
        &mut self,
        _log_id: LogId<NodeId>,
    ) -> Result<(), StorageError<NodeId>> {
        Ok(())
    }

    async fn purge_logs_upto(&mut self, _log_id: LogId<NodeId>) -> Result<(), StorageError<NodeId>> {
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
        Ok((None, StoredMembership::default()))
    }

    async fn apply_to_state_machine(
        &mut self,
        _entries: &[Entry<SwarmConfig>],
    ) -> Result<Vec<SwarmResponse>, StorageError<NodeId>> {
        Ok(vec![])
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
