use openraft::error::{InstallSnapshotError, NetworkError, RPCError, RaftError};
use openraft::network::{RaftNetwork, RaftNetworkFactory};
use openraft::raft::{
    AppendEntriesRequest, AppendEntriesResponse, InstallSnapshotRequest, InstallSnapshotResponse,
    VoteRequest, VoteResponse,
};
use reqwest::Client;
use std::future::Future;

use crate::swarm::raft_types::{NodeId, SwarmConfig, SwarmNode};

pub struct SwarmNetwork {
    client: Client,
}

impl SwarmNetwork {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
        }
    }
}

pub struct SwarmNetworkConnection {
    client: Client,
    target: NodeId,
    target_node: SwarmNode,
}

impl RaftNetworkFactory<SwarmConfig> for SwarmNetwork {
    type Network = SwarmNetworkConnection;

    async fn new_client(&mut self, target: NodeId, node: &SwarmNode) -> Self::Network {
        SwarmNetworkConnection {
            client: self.client.clone(),
            target,
            target_node: node.clone(),
        }
    }
}

impl RaftNetwork<SwarmConfig> for SwarmNetworkConnection {
    async fn append_entries(
        &mut self,
        req: AppendEntriesRequest<SwarmConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        AppendEntriesResponse<NodeId>,
        RPCError<NodeId, SwarmNode, RaftError<NodeId>>,
    > {
        let url = format!("{}/raft/append", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        let res: AppendEntriesResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        Ok(res)
    }

    async fn install_snapshot(
        &mut self,
        req: InstallSnapshotRequest<SwarmConfig>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        InstallSnapshotResponse<NodeId>,
        RPCError<NodeId, SwarmNode, RaftError<NodeId, InstallSnapshotError>>,
    > {
        let url = format!("{}/raft/snapshot", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        let res: InstallSnapshotResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        Ok(res)
    }

    async fn vote(
        &mut self,
        req: VoteRequest<NodeId>,
        _option: openraft::network::RPCOption,
    ) -> Result<
        VoteResponse<NodeId>,
        RPCError<NodeId, SwarmNode, RaftError<NodeId>>,
    > {
        let url = format!("{}/raft/vote", self.target_node.addr);
        let resp = self
            .client
            .post(url)
            .json(&req)
            .send()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        let res: VoteResponse<NodeId> = resp
            .json()
            .await
            .map_err(|e| RPCError::Network(NetworkError::new(&e)))?;

        Ok(res)
    }
}
