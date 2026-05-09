//! Chaos mode support for crash injection testing
//!
//! This module provides crash injection points that can be enabled via
//! marker files in the data directory when the `chaos` feature is enabled.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use tokio::sync::RwLock;

pub static CHAOS_ENABLED: AtomicBool = AtomicBool::new(false);

#[derive(Debug, Clone, Default)]
pub struct ChaosState {
    crash_points: Arc<RwLock<std::collections::HashSet<String>>>,
}

impl ChaosState {
    pub fn new() -> Self {
        Self {
            crash_points: Arc::new(RwLock::new(std::collections::HashSet::new())),
        }
    }

    pub async fn scan_crash_points(&self, data_dir: &Path) {
        let mut points = self.crash_points.write().await;
        points.clear();

        if let Ok(entries) = std::fs::read_dir(data_dir) {
            for entry in entries.flatten() {
                let name = entry.file_name();
                let name_str = name.to_string_lossy().to_string();
                if name_str.starts_with("enable_crash_") {
                    let point = name_str.strip_prefix("enable_crash_").unwrap().to_string();
                    points.insert(point);
                }
            }
        }

        if !points.is_empty() {
            CHAOS_ENABLED.store(true, Ordering::SeqCst);
            tracing::info!("Chaos mode enabled with crash points: {:?}", points);
        }
    }

    pub async fn check_crash_point(&self, point: &str) -> bool {
        let points = self.crash_points.read().await;
        points.contains(point)
    }

    pub async fn should_crash(&self, point: &str) -> bool {
        if !CHAOS_ENABLED.load(Ordering::SeqCst) {
            return false;
        }
        self.check_crash_point(point).await
    }
}

#[macro_export]
macro_rules! chaos_crash {
    ($state:expr, $point:literal) => {
        #[cfg(feature = "chaos")]
        {
            use std::path::Path;
            use std::fs;

            let marker = Path::new($state.data_dir())
                .join(format!("enable_crash_{}", $point));

            if marker.exists() {
                tracing::error!("CHAOS: Triggering crash at point: {}", $point);
                std::process::exit(1);
            }
        }
    };
}

pub fn enable_chaos_on_startup(data_dir: &Path) {
    if let Ok(entries) = std::fs::read_dir(data_dir) {
        for entry in entries.flatten() {
            let name = entry.file_name();
            let name_str = name.to_string_lossy().to_string();
            if name_str.starts_with("enable_crash_") {
                CHAOS_ENABLED.store(true, Ordering::SeqCst);
                tracing::warn!("Chaos mode detected on startup: {}", name_str);
            }
        }
    }
}

pub fn is_chaos_enabled() -> bool {
    CHAOS_ENABLED.load(Ordering::SeqCst)
}