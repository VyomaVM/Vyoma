use prometheus::{
    Counter, Encoder, Gauge, GaugeVec, Histogram, HistogramOpts, Opts, Registry, TextEncoder,
};
use std::sync::Arc;
use tokio::sync::RwLock;
use tracing::info;

pub struct IgniteMetrics {
    registry: Registry,
    pub vms_running: Gauge,
    pub vms_total: Counter,
    pub vm_boot_duration: Histogram,
    pub vm_memory_usage: GaugeVec,
    pub vm_cpu_usage: GaugeVec,
    pub snapshot_count: GaugeVec,
}

impl IgniteMetrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let registry = Registry::new();

        let vms_running = Gauge::with_opts(Opts::new(
            "vyoma_vms_running",
            "Number of currently running VMs",
        ))?;

        let vms_total =
            Counter::with_opts(Opts::new("vyoma_vms_total", "Total number of VMs created"))?;

        let vm_boot_duration = Histogram::with_opts(HistogramOpts::new(
            "vyoma_vm_boot_duration_seconds",
            "VM boot duration in seconds",
        ))?;

        let vm_memory_usage = GaugeVec::new(
            Opts::new(
                "vyoma_vm_memory_usage_bytes",
                "Memory usage per VM in bytes",
            ),
            &["vm_id"],
        )?;

        let vm_cpu_usage = GaugeVec::new(
            Opts::new("vyoma_vm_cpu_usage_percent", "CPU usage percentage per VM"),
            &["vm_id"],
        )?;

        let snapshot_count = GaugeVec::new(
            Opts::new("vyoma_snapshot_count", "Number of snapshots per VM"),
            &["vm_id"],
        )?;

        registry.register(Box::new(vms_running.clone()))?;
        registry.register(Box::new(vms_total.clone()))?;
        registry.register(Box::new(vm_boot_duration.clone()))?;
        registry.register(Box::new(vm_memory_usage.clone()))?;
        registry.register(Box::new(vm_cpu_usage.clone()))?;
        registry.register(Box::new(snapshot_count.clone()))?;

        Ok(Self {
            registry,
            vms_running,
            vms_total,
            vm_boot_duration,
            vm_memory_usage,
            vm_cpu_usage,
            snapshot_count,
        })
    }

    pub fn register_vm(&self, vm_id: &str) {
        self.vms_running.inc();
        self.vms_total.inc();
        self.vm_memory_usage.with_label_values(&[vm_id]).set(0.0);
        self.vm_cpu_usage.with_label_values(&[vm_id]).set(0.0);
        self.snapshot_count.with_label_values(&[vm_id]).set(0.0);
        info!("Registered VM {} in metrics", vm_id);
    }

    pub fn unregister_vm(&self, vm_id: &str) {
        self.vms_running.dec();
        let _ = self.vm_memory_usage.remove_label_values(&[vm_id]);
        let _ = self.vm_cpu_usage.remove_label_values(&[vm_id]);
        let _ = self.snapshot_count.remove_label_values(&[vm_id]);
        info!("Unregistered VM {} from metrics", vm_id);
    }

    pub fn set_memory_usage(&self, vm_id: &str, bytes: u64) {
        self.vm_memory_usage
            .with_label_values(&[vm_id])
            .set(bytes as f64);
    }

    pub fn set_cpu_usage(&self, vm_id: &str, percent: f64) {
        self.vm_cpu_usage.with_label_values(&[vm_id]).set(percent);
    }

    pub fn record_boot_duration(&self, seconds: f64) {
        self.vm_boot_duration.observe(seconds);
    }

    pub fn increment_snapshot_count(&self, vm_id: &str) {
        self.snapshot_count.with_label_values(&[vm_id]).inc();
    }

    pub fn gather(&self) -> Vec<u8> {
        let encoder = TextEncoder::new();
        let metric_families = self.registry.gather();
        let mut buffer = Vec::new();
        encoder.encode(&metric_families, &mut buffer).unwrap();
        buffer
    }

    pub fn registry(&self) -> &Registry {
        &self.registry
    }
}

impl Default for IgniteMetrics {
    fn default() -> Self {
        Self::new().expect("Failed to create IgniteMetrics")
    }
}

pub type SharedMetrics = Arc<RwLock<IgniteMetrics>>;

pub fn create_metrics() -> Result<SharedMetrics, prometheus::Error> {
    let metrics = IgniteMetrics::new()?;
    Ok(Arc::new(RwLock::new(metrics)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_creation() {
        let metrics = IgniteMetrics::new().unwrap();
        assert_eq!(metrics.vms_running.get(), 0.0);
        assert_eq!(metrics.vms_total.get(), 0.0);
    }

    #[test]
    fn test_register_vm() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        assert_eq!(metrics.vms_running.get(), 1.0);
        assert_eq!(metrics.vms_total.get(), 1.0);
    }

    #[test]
    fn test_unregister_vm() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        metrics.unregister_vm("test-vm-1");
        assert_eq!(metrics.vms_running.get(), 0.0);
    }

    #[test]
    fn test_set_memory_usage() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        metrics.set_memory_usage("test-vm-1", 2048);
        let memory = metrics
            .vm_memory_usage
            .with_label_values(&["test-vm-1"])
            .get();
        assert_eq!(memory, 2048.0);
    }

    #[test]
    fn test_set_cpu_usage() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        metrics.set_cpu_usage("test-vm-1", 50.0);
        let cpu = metrics.vm_cpu_usage.with_label_values(&["test-vm-1"]).get();
        assert_eq!(cpu, 50.0);
    }

    #[test]
    fn test_record_boot_duration() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.record_boot_duration(1.5);
        metrics.record_boot_duration(2.0);
    }

    #[test]
    fn test_increment_snapshot_count() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        metrics.increment_snapshot_count("test-vm-1");
        metrics.increment_snapshot_count("test-vm-1");
        let count = metrics
            .snapshot_count
            .with_label_values(&["test-vm-1"])
            .get();
        assert_eq!(count, 2.0);
    }

    #[test]
    fn test_gather_metrics() {
        let metrics = IgniteMetrics::new().unwrap();
        metrics.register_vm("test-vm-1");
        let output = metrics.gather();
        let text = String::from_utf8_lossy(&output);
        assert!(text.contains("vyoma_vms_running"));
        assert!(text.contains("vyoma_vms_total"));
    }

    #[test]
    fn test_create_shared_metrics() {
        let shared = create_metrics().unwrap();
        let metrics = shared.blocking_read();
        assert_eq!(metrics.vms_running.get(), 0.0);
    }
}
