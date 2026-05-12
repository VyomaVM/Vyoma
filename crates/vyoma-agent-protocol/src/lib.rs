use serde::{Deserialize, Serialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRequest {
    ProcessList,
    GetMetrics,
    FileRead { path: String },
    ExecCommand {
        cmd: Vec<String>,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    ProcessList(Vec<ProcessInfo>),
    Metrics(VmMetrics),
    FileContent(Vec<u8>),
    ExecOutput {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_code: i32,
    },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: Option<u32>,
    pub name: String,
    pub state: Option<String>,
    pub cpu_usage: Option<f32>,
    pub memory_mb: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetrics {
    pub cpu_usage_percent: f32,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
    pub process_count: usize,
}

impl VmMetrics {
    pub fn new(
        cpu_usage_percent: f32,
        mem_used_kb: u64,
        mem_total_kb: u64,
        process_count: usize,
    ) -> Self {
        Self {
            cpu_usage_percent,
            mem_used_kb,
            mem_total_kb,
            process_count,
        }
    }
}

impl ProcessInfo {
    pub fn new(pid: u32, name: String) -> Self {
        Self {
            pid,
            ppid: None,
            name,
            state: None,
            cpu_usage: None,
            memory_mb: None,
        }
    }
}