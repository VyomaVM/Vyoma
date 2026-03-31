# ADR-034: Prometheus Metrics Endpoint

**Status**: Accepted | Phase 3.5 (v1.6)

## Summary
Implement Prometheus metrics endpoint for Ignite to enable monitoring and observability.

## Context
As part of Phase 3, we need to add Prometheus metrics to Ignite. This enables:
- VM and container runtime monitoring
- Performance tracking and alerting
- Integration with Prometheus-compatible tools

## Decision
Implement metrics endpoint as per the technical spec:

```rust
pub struct IgniteMetrics {
    pub vms_running:      Gauge,
    pub vms_total:        Counter,
    pub vm_boot_duration: Histogram,
    pub vm_memory_usage:  GaugeVec,  // labels: vm_id
    pub vm_cpu_usage:     GaugeVec,  // labels: vm_id
    pub snapshot_count:   GaugeVec,  // labels: vm_id
}
```

## Implementation

### Location
- `crates/ignited/src/metrics.rs`

### Metrics Types
1. **vms_running**: Current number of running VMs (Gauge)
2. **vms_total**: Total VMs created since start (Counter)
3. **vm_boot_duration**: VM boot time in seconds (Histogram)
4. **vm_memory_usage**: Memory used by each VM in MB (GaugeVec)
5. **vm_cpu_usage**: CPU usage percentage per VM (GaugeVec)
6. **snapshot_count**: Number of snapshots per VM (GaugeVec)

### Endpoint
- Expose at `GET /metrics` in the Axum router

## Consequences
- Enables Prometheus monitoring
- Supports horizontal scaling with labeled metrics
- Ready for alerting and dashboards
