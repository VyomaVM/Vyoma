# Swarm Migration Guide

## Overview

This guide helps migrate from the legacy seed-based cluster management to the new Raft-based Swarm system.

## Changes in 0.2.0

### Before (Legacy)
- Single seed node as central point of failure
- `ClusterManager` handling all registrations
- VXLAN routes created on seed only
- WireGuard keys exchanged ad-hoc

### After (Raft-based)
- All nodes equal, Raft consensus as single source of truth
- `SwarmRaft` state machine drives cluster membership
- Deterministic subnet allocation (no central mutex)
- Network operations triggered by Raft state changes
- WireGuard integration via NetworkIntegration

## Migration Steps

### For New Deployments
No migration needed. Just use the new Raft-based endpoints.

### For Existing Swarms

#### Option 1: Fresh Start (Recommended)
1. Stop all daemon instances
2. Clear Raft state: `rm -rf .vyoma/.vyoma/state/raft_db`
3. Restart with new configuration

#### Option 2: Rolling Upgrade
1. Deploy new version to seed node
2. Restart seed - it will initialize fresh Raft cluster
3. Rolling restart other nodes
4. Other nodes will join via `/swarm/join`

### API Changes

| Old Endpoint | New Endpoint | Notes |
|--------------|--------------|-------|
| `POST /swarm/init` | `POST /swarm/init` | Now uses Raft |
| `POST /swarm/join` | `POST /swarm/join` | Now uses Raft |
| `POST /swarm/register` | `POST /swarm/join` | Deprecated |
| `GET /swarm/nodes` | `GET /swarm/nodes` | Now reads from SwarmRaft |

### New Request Format for `/swarm/join`

```json
{
  "node_id": 2,
  "addr": "192.168.1.102:7946",
  "public_key": "node2_public_key",
  "wireguard_key": "optional_wireguard_key",
  "wireguard_port": 51820
}
```

### New Response Format for `/swarm/init`

```json
{
  "node_id": 1,
  "subnet_id": 2,
  "wireguard_port": null,
  "wireguard_key": null
}
```

### Subnet Allocation Changes

Old: Sequential allocation starting from 1
- Node 1: 10.42.1.0/24
- Node 2: 10.42.2.0/24

New: Deterministic based on node_id
- Node 1: 10.42.2.0/24 (1 % 254 + 1 = 2)
- Node 2: 10.42.3.0/24 (2 % 254 + 1 = 3)
- Node 100: 10.42.101.0/24 (100 % 254 + 1 = 101)

## Verification

After migration, verify cluster health:

```bash
# Check node list
curl http://localhost:3000/swarm/nodes

# Check Raft status (if endpoint exists)
curl http://localhost:3000/raft/status

# Test connectivity between nodes
ping 10.42.<subnet_id>.1
```

## Rollback

If issues occur, rollback to previous version:

1. Stop daemon
2. Restore `cluster.rs` from backup
3. Restart daemon
4. Existing nodes will re-register via old flow

## Troubleshooting

### Nodes not joining
- Check network connectivity on port 7946
- Verify Raft initialization completed: `curl localhost:3000/swarm/nodes`

### Route issues
- Check VXLAN device exists: `ip link show ign-vxlan`
- Check WireGuard status: `wg show`

### Leader election problems
- Ensure odd number of nodes for Raft
- Check heartbeat connectivity between nodes

## Deprecation Notes

- `ClusterManager` is deprecated, use `SwarmRaft` instead
- `/swarm/register` redirects to Raft-based flow
- Legacy methods in `cluster.rs` still work but will be removed in 0.3.0