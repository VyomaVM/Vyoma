# ADR-024: Docker Compose Schema v3 Compatibility

## Status
Accepted | Phase 1.5 (v1.2)

## Context
Currently (v1.1), the vyoma-compose parser only supports `version: "1.0"` which is a custom format. Users cannot `vyoma up` from existing `docker-compose.yml` files that use the standard Docker Compose v3 format. This prevents adoption as users must manually convert their compose files.

Additionally, the `networks:` top-level key is not supported - all services share one default bridge network.

## Decision
We will extend the compose parser to support Docker Compose v3 schema while maintaining backward compatibility with the existing "1.0" format.

### Schema Changes

#### Version Support
- Accept versions: "1.0" (legacy), "3.x", "3.0", "3.1", "3.2", "3.3", "3.4", "3.5", "3.6", "3.7", "3.8", "3.9"

#### New Top-Level Keys

**networks:**
```yaml
networks:
  frontend:
    driver: bridge
    ipam:
      config:
        - subnet: 172.17.0.0/16
  backend:
    driver: bridge
    external: true
```
Maps to Linux bridges created via CNI.

**volumes:**
```yaml
volumes:
  db-data:
    driver: local
  cache:
    external: true
```

#### Extended Service Fields
- `networks:` - List of networks to attach (replaces single network)
- `deploy:` - Deployment constraints (future)
- `secrets:` - Secret files (future)

### Implementation

1. **Parser Update**: Modify `VyomaCompose` struct to include optional `networks` and `volumes` fields
2. **Network Creation**: On `vyoma up`, iterate networks and create Linux bridge per network
3. **Service Network Assignment**: Update VM network config to attach to specified networks

## Consequences
**Positive:**
- Full Docker Compose v3 compatibility
- Multi-network support per compose file
- Volume definitions for persistent storage
- Backward compatible with existing "1.0" format

**Negative:**
- Parser complexity increases
- Network creation adds latency to `vyoma up`

## Implementation Notes
- Use serde flatten for backward compatibility
- Network names in compose map to bridge names: `vyoma-<network-name>`
- Default network (when no networks specified) remains "vyoma-net"
