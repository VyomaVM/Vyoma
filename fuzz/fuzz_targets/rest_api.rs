//! Fuzz target for REST API request handlers

#![no_main]

use libfuzzer_sys::fuzz_target;
use serde_json::Value;
use std::collections::HashMap;

/// Fuzz the VM run request parsing
///
/// This target fuzzes the JSON parsing for VM creation requests
/// to find crashes in the request deserialization.
fuzz_target!(|data: &[u8]| {
    if let Ok(request) = serde_json::from_slice::<VmRunRequest>(data) {
        // If parsing succeeds, verify we can access fields
        let _ = request.image;
        let _ = request.vcpu;
        let _ = request.mem_size_mib;
        let _ = request.networks;
        let _ = request.labels;
    }
});

/// Fuzz generic JSON request parsing
///
/// This target fuzzes generic JSON structures that might be received
/// by the REST API, testing the underlying JSON parsing infrastructure.
fuzz_target!(|data: &[u8]| {
    if let Ok(value) = serde_json::from_slice::<Value>(data) {
        // Recursively process the JSON value to exercise all parsing paths
        process_value(&value);
    }
});

/// Fuzz port mapping parsing
///
/// This target specifically fuzzes port mapping structures used
/// in the VM run requests.
fuzz_target!(|data: &[u8]| {
    if let Ok(mapping) = serde_json::from_slice::<PortMapping>(data) {
        let _ = mapping.container_port;
        let _ = mapping.host_port;
        let _ = mapping.protocol;
    }
});

/// Fuzz volume mount parsing
///
/// This target fuzzes volume mount structures used in VM requests.
fuzz_target!(|data: &[u8]| {
    if let Ok(mount) = serde_json::from_slice::<VolumeMount>(data) {
        let _ = mount.host_path;
        let _ = mount.container_path;
        let _ = mount.read_only;
    }
});

/// Helper function to recursively process JSON values
fn process_value(value: &Value) {
    match value {
        Value::Null => {},
        Value::Bool(_) => {},
        Value::Number(_) => {},
        Value::String(s) => { let _ = s.len(); },
        Value::Array(arr) => { for item in arr { process_value(item); } },
        Value::Object(obj) => { for (_, v) in obj { process_value(v); } },
    }
}

#[derive(Debug, serde::Deserialize)]
struct VmRunRequest {
    image: String,
    #[serde(default)]
    vcpu: u32,
    #[serde(default)]
    mem_size_mib: u32,
    #[serde(default)]
    ports: Vec<PortMapping>,
    #[serde(default)]
    volumes: Vec<VolumeMount>,
    #[serde(default)]
    hostname: Option<String>,
    #[serde(default)]
    networks: Vec<String>,
    #[serde(default)]
    labels: HashMap<String, String>,
    #[serde(default)]
    base_image_path: String,
}

#[derive(Debug, serde::Deserialize)]
struct PortMapping {
    #[serde(default)]
    container_port: u16,
    #[serde(default)]
    host_port: Option<u16>,
    #[serde(default)]
    protocol: String,
}

#[derive(Debug, serde::Deserialize)]
struct VolumeMount {
    #[serde(default)]
    host_path: String,
    #[serde(default)]
    container_path: String,
    #[serde(default)]
    read_only: bool,
}