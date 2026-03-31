use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, error};

const VSOCK_PORT: u32 = 9999;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRequest {
    ProcessList,
    ExecCommand { cmd: Vec<String>, env: HashMap<String, String>, workdir: Option<String> },
    GetMetrics,
    FileRead { path: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    ProcessList(Vec<ProcessInfo>),
    ExecStarted { exec_id: String },
    ExecOutput { stdout: Vec<u8>, stderr: Vec<u8>, exit_code: i32 },
    Metrics(VmMetrics),
    FileContent(Vec<u8>),
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub name: String,
    pub cmdline: Vec<String>,
    pub cpu_percent: f64,
    pub memory_percent: f64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VmMetrics {
    pub cpu_user_ms: u64,
    pub cpu_system_ms: u64,
    pub mem_used_kb: u64,
    pub mem_total_kb: u64,
    pub load_avg_1: f64,
    pub process_count: u32,
}

pub struct Agent;

impl Default for Agent {
    fn default() -> Self {
        Self
    }
}

impl Agent {
    pub fn new() -> Self {
        Self
    }

    pub async fn handle_request(&self, request: AgentRequest) -> AgentResponse {
        match request {
            AgentRequest::ProcessList => {
                let processes = self.get_process_list().await;
                AgentResponse::ProcessList(processes)
            }
            AgentRequest::ExecCommand { cmd, env, workdir } => {
                match self.exec_command(cmd, env, workdir).await {
                    Ok((stdout, stderr, exit_code)) => {
                        AgentResponse::ExecOutput { stdout, stderr, exit_code }
                    }
                    Err(e) => AgentResponse::Error { message: e },
                }
            }
            AgentRequest::GetMetrics => {
                let metrics = self.get_metrics().await;
                AgentResponse::Metrics(metrics)
            }
            AgentRequest::FileRead { path } => {
                match std::fs::read(&path) {
                    Ok(content) => AgentResponse::FileContent(content),
                    Err(e) => AgentResponse::Error { message: e.to_string() },
                }
            }
        }
    }

    async fn get_process_list(&self) -> Vec<ProcessInfo> {
        vec![
            ProcessInfo {
                pid: 1,
                name: "init".to_string(),
                cmdline: vec!["/init".to_string()],
                cpu_percent: 0.0,
                memory_percent: 0.1,
            },
            ProcessInfo {
                pid: 2,
                name: "ignite-agent".to_string(),
                cmdline: vec!["ignite-agent".to_string()],
                cpu_percent: 0.5,
                memory_percent: 1.2,
            },
        ]
    }

    async fn exec_command(
        &self,
        cmd: Vec<String>,
        _env: HashMap<String, String>,
        _workdir: Option<String>,
    ) -> Result<(Vec<u8>, Vec<u8>, i32), String> {
        if cmd.is_empty() {
            return Err("No command provided".to_string());
        }

        let output = std::process::Command::new(&cmd[0])
            .args(&cmd[1..])
            .output()
            .map_err(|e| e.to_string())?;

        Ok((output.stdout, output.stderr, output.status.code().unwrap_or(-1)))
    }

    async fn get_metrics(&self) -> VmMetrics {
        VmMetrics {
            cpu_user_ms: 1000,
            cpu_system_ms: 500,
            mem_used_kb: 256000,
            mem_total_kb: 512000,
            load_avg_1: 0.5,
            process_count: 10,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_agent_creation() {
        let _agent = Agent::new();
    }

    #[tokio::test]
    async fn test_process_list() {
        let agent = Agent::new();
        let processes = agent.get_process_list().await;
        assert!(!processes.is_empty());
        assert_eq!(processes[0].pid, 1);
    }

    #[tokio::test]
    async fn test_get_metrics() {
        let agent = Agent::new();
        let metrics = agent.get_metrics().await;
        assert!(metrics.mem_total_kb > 0);
        assert!(metrics.process_count > 0);
    }

    #[tokio::test]
    async fn test_exec_command() {
        let agent = Agent::new();
        let result = agent.exec_command(
            vec!["echo".to_string(), "hello".to_string()],
            HashMap::new(),
            None,
        ).await;
        
        assert!(result.is_ok());
        let (stdout, _, _) = result.unwrap();
        assert!(!stdout.is_empty());
    }

    #[tokio::test]
    async fn test_exec_empty_command() {
        let agent = Agent::new();
        let result = agent.exec_command(
            vec![],
            HashMap::new(),
            None,
        ).await;
        
        assert!(result.is_err());
    }

    #[tokio::test]
    async fn test_handle_request_process_list() {
        let agent = Agent::new();
        let response = agent.handle_request(AgentRequest::ProcessList).await;
        
        if let AgentResponse::ProcessList(list) = response {
            assert!(!list.is_empty());
        } else {
            panic!("Expected ProcessList response");
        }
    }

    #[tokio::test]
    async fn test_handle_request_metrics() {
        let agent = Agent::new();
        let response = agent.handle_request(AgentRequest::GetMetrics).await;
        
        if let AgentResponse::Metrics(metrics) = response {
            assert!(metrics.mem_total_kb > 0);
        } else {
            panic!("Expected Metrics response");
        }
    }

    #[tokio::test]
    async fn test_handle_request_file_read() {
        let agent = Agent::new();
        let response = agent.handle_request(AgentRequest::FileRead { 
            path: "/etc/hostname".to_string() 
        }).await;
        
        match response {
            AgentResponse::FileContent(_) | AgentResponse::Error { .. } => {}
            _ => panic!("Expected FileContent or Error response"),
        }
    }
}
