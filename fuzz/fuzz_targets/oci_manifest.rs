//! Fuzz target for OCI manifest parsing

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;

/// Fuzz the OCI manifest JSON parsing
///
/// This target fuzzes the JSON parsing of OCI manifests to uncover
/// hidden crashes or panics in the manifest parsing logic.
fuzz_target!(|data: &[u8]| {
    // Try to parse as JSON
    if let Ok(manifest) = serde_json::from_slice::<Value>(data) {
        // If it's valid JSON, try to extract common OCI manifest fields
        // This exercises the parsing logic without requiring a full OCI client

        // Check for schemaVersion
        let _ = manifest.get("schemaVersion")
            .and_then(|v| v.as_i64());

        // Check for mediaType
        let _ = manifest.get("mediaType")
            .and_then(|v| v.as_str());

        // Check for layers
        if let Some(layers) = manifest.get("layers").and_then(|v| v.as_array()) {
            for layer in layers {
                let _ = layer.get("mediaType")
                    .and_then(|v| v.as_str());
                let _ = layer.get("digest")
                    .and_then(|v| v.as_str());
                let _ = layer.get("size")
                    .and_then(|v| v.as_i64());
            }
        }

        // Check for config
        if let Some(config) = manifest.get("config") {
            let _ = config.get("mediaType")
                .and_then(|v| v.as_str());
            let _ = config.get("digest")
                .and_then(|v| v.as_str());
            let _ = config.get("size")
                .and_then(|v| v.as_i64());
        }
    }
});

/// Fuzz the OCI image config parsing
///
/// This target specifically fuzzes the OCI image config parsing
/// which is used when pulling images.
fuzz_target!(|data: &[u8]| {
    if let Ok(config) = serde_json::from_slice::<Value>(data) {
        // Check for common OCI config fields
        let _ = config.get("architecture")
            .and_then(|v| v.as_str());

        let _ = config.get("os")
            .and_then(|v| v.as_str());

        let _ = config.get("config")
            .and_then(|v| v.as_object());

        // Check for exposed ports
        if let Some(ports) = config.get("exposedPorts").and_then(|v| v.as_object()) {
            for (port, _) in ports {
                let _ = port.parse::<u16>();
            }
        }

        // Check for env variables
        if let Some(env) = config.get("env").and_then(|v| v.as_array()) {
            for e in env {
                let _ = e.as_str();
            }
        }

        // Check for cmd and entrypoint
        let _ = config.get("cmd").and_then(|v| v.as_array());
        let _ = config.get("entrypoint").and_then(|v| v.as_array());
    }
});

/// Fuzz index.json parsing (for multi-architecture images)
fuzz_target!(|data: &[u8]| {
    if let Ok(index) = serde_json::from_slice::<Value>(data) {
        // Check for manifest list structure
        let _ = index.get("manifests").and_then(|v| v.as_array());

        // Check for schema version
        let _ = index.get("schemaVersion")
            .and_then(|v| v.as_i64());
    }
});