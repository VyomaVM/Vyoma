use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use std::path::PathBuf;
use sysinfo::System;
use tokio::fs;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRequest {
    ProcessList,
    GetMetrics,
    FileRead { path: String },
    ExecCommand { cmd: Vec<String>, env: std::collections::HashMap<String, String>, workdir: Option<String> },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    ProcessList(Vec<ProcessInfo>),
    Metrics(VmMetrics),
    FileContent(Vec<u8>),
    ExecOutput { stdout: Vec<u8>, stderr: Vec<u8>, exit_code: i32 },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cpu_usage: f32,
    pub memory_mb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetrics {
    pub cpu_usage_percent: f32,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
    pub process_count: usize,
}

pub async fn collect_metrics() -> Result<VmMetrics> {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    let cpu_usage_percent = sys.global_cpu_info().cpu_usage();
    let mem_used_kb = sys.used_memory() / 1024;
    let mem_total_kb = sys.total_memory() / 1024;
    let process_count = sys.processes().len();
    
    Ok(VmMetrics {
        cpu_usage_percent,
        mem_used_kb,
        mem_total_kb,
        process_count,
    })
}

pub fn collect_process_list() -> Vec<ProcessInfo> {
    let mut sys = System::new_all();
    sys.refresh_all();
    
    sys.processes()
        .iter()
        .map(|(pid, process)| ProcessInfo {
            pid: pid.as_u32(),
            name: process.name().to_string(),
            cpu_usage: process.cpu_usage(),
            memory_mb: process.memory() / 1024 / 1024,
        })
        .collect()
}

pub async fn read_file_content(path: &str) -> Result<Vec<u8>> {
    let path = PathBuf::from(path);
    fs::read(&path)
        .await
        .context(format!("Failed to read file: {}", path.display()))
}

pub async fn execute_command(cmd: &[String]) -> Result<(Vec<u8>, Vec<u8>, i32)> {
    if cmd.is_empty() {
        return Ok((Vec::new(), b"Empty command".to_vec(), 1));
    }
    
    let child = tokio::process::Command::new(&cmd[0])
        .args(&cmd[1..])
        .stdout(std::process::Stdio::piped())
        .stderr(std::process::Stdio::piped())
        .spawn()
        .context("Failed to spawn command")?;
    
    let output = child.wait_with_output().await?;
    
    let exit_code = output.status.code().unwrap_or(-1);
    Ok((output.stdout, output.stderr, exit_code))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_metrics_collection() {
        let sys = System::new_all();
        assert!(sys.total_memory() > 0);
    }

    #[test]
    fn test_process_list() {
        let processes = collect_process_list();
        assert!(!processes.is_empty());
    }

    #[test]
    fn test_process_info_fields() {
        let processes = collect_process_list();
        if let Some(p) = processes.first() {
            assert!(p.pid > 0);
            assert!(!p.name.is_empty());
        }
    }

    #[test]
    fn test_agent_request_serialization() {
        let req = AgentRequest::ProcessList;
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ProcessList"));
    }

    #[test]
    fn test_agent_response_serialization() {
        let resp = AgentResponse::Metrics(VmMetrics {
            cpu_usage_percent: 50.0,
            mem_used_kb: 512000,
            mem_total_kb: 1024000,
            process_count: 42,
        });
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Metrics"));
    }

    #[test]
    fn test_file_read_request() {
        let req = AgentRequest::FileRead {
            path: "/etc/hostname".to_string(),
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("FileRead"));
    }

    #[test]
    fn test_exec_command_request() {
        let req = AgentRequest::ExecCommand {
            cmd: vec!["ls".to_string(), "-la".to_string()],
            env: std::collections::HashMap::new(),
            workdir: None,
        };
        let json = serde_json::to_string(&req).unwrap();
        assert!(json.contains("ExecCommand"));
    }

    #[test]
    fn test_response_error_serialization() {
        let resp = AgentResponse::Error {
            message: "Test error".to_string(),
        };
        let json = serde_json::to_string(&resp).unwrap();
        assert!(json.contains("Error"));
        assert!(json.contains("Test error"));
    }
}
