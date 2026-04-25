use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::process::Stdio;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::process::Command;
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use tokio_vsock::{VsockAddr, VsockListener, VMADDR_CID_ANY};
use tracing::{error, info};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};

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
    ProcessList { processes: Vec<ProcessInfo> },
    ExecOutput { stdout: Vec<u8>, stderr: Vec<u8>, exit_code: i32 },
    Metrics { cpu_user_ms: u64, cpu_system_ms: u64, mem_used_kb: u64, mem_total_kb: u64, process_count: usize },
    FileContent { content: Vec<u8> },
    Error { message: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting ignite-agent on vsock port {}", VSOCK_PORT);
    
    let listener = match VsockListener::bind(VsockAddr::new(VMADDR_CID_ANY, VSOCK_PORT)) {
        Ok(l) => l,
        Err(e) => {
            error!("Failed to bind vsock: {}", e);
            std::process::exit(1);
        }
    };
    
    loop {
        match listener.accept().await {
            Ok((stream, addr)) => {
                info!("Client connected from vsock {:?}", addr);
                tokio::spawn(async move {
                    if let Err(e) = handle_connection(stream).await {
                        error!("Connection error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Accept failed: {}", e);
            }
        }
    }
}

async fn handle_connection(stream: tokio_vsock::VsockStream) -> Result<()> {
    let mut framed = Framed::new(stream, LengthDelimitedCodec::new());
    
    while let Some(frame_result) = framed.next().await {
        let frame = frame_result?;
        let request: AgentRequest = match serde_json::from_slice(&frame) {
            Ok(req) => req,
            Err(e) => {
                error!("Invalid request: {}", e);
                let resp = AgentResponse::Error { message: e.to_string() };
                framed.send(Bytes::from(serde_json::to_vec(&resp)?)).await?;
                continue;
            }
        };
        
        let response = handle_request(request).await;
        framed.send(Bytes::from(serde_json::to_vec(&response)?)).await?;
    }
    
    Ok(())
}

async fn handle_request(request: AgentRequest) -> AgentResponse {
    match request {
        AgentRequest::ProcessList => {
            AgentResponse::ProcessList { processes: collect_process_list() }
        }
        AgentRequest::GetMetrics => {
            let metrics = collect_metrics();
            AgentResponse::Metrics {
                cpu_user_ms: metrics.cpu_user_ms,
                cpu_system_ms: metrics.cpu_system_ms,
                mem_used_kb: metrics.mem_used_kb,
                mem_total_kb: metrics.mem_total_kb,
                process_count: metrics.process_count
            }
        }
        AgentRequest::FileRead { path } => {
            match tokio::fs::read(&path).await {
                Ok(content) => AgentResponse::FileContent { content },
                Err(e) => AgentResponse::Error { message: e.to_string() }
            }
        }
        AgentRequest::ExecCommand { cmd, env, workdir } => {
            if cmd.is_empty() {
                return AgentResponse::Error { message: "Empty command".to_string() };
            }
            
            let mut command = Command::new(&cmd[0]);
            command.args(&cmd[1..]);
            command.envs(env);
            
            if let Some(dir) = workdir {
                command.current_dir(dir);
            }
            
            command.stdout(Stdio::piped()).stderr(Stdio::piped());
            
            match command.output().await {
                Ok(output) => {
                    AgentResponse::ExecOutput {
                        stdout: output.stdout,
                        stderr: output.stderr,
                        exit_code: output.status.code().unwrap_or(-1),
                    }
                }
                Err(e) => AgentResponse::Error { message: e.to_string() }
            }
        }
    }
}

struct SysMetrics {
    cpu_user_ms: u64,
    cpu_system_ms: u64,
    mem_used_kb: u64,
    mem_total_kb: u64,
    process_count: usize,
}

fn collect_metrics() -> SysMetrics {
    let mut metrics = SysMetrics {
        cpu_user_ms: 0,
        cpu_system_ms: 0,
        mem_used_kb: 0,
        mem_total_kb: 0,
        process_count: 0,
    };

    if let Ok(stat) = std::fs::read_to_string("/proc/stat") {
        if let Some(cpu_line) = stat.lines().find(|l| l.starts_with("cpu ")) {
            let parts: Vec<&str> = cpu_line.split_whitespace().collect();
            if parts.len() > 3 {
                metrics.cpu_user_ms = parts[1].parse().unwrap_or(0);
                metrics.cpu_system_ms = parts[3].parse().unwrap_or(0);
            }
        }
    }

    if let Ok(meminfo) = std::fs::read_to_string("/proc/meminfo") {
        let mut mem_total = 0;
        let mut mem_free = 0;
        let mut mem_buffers = 0;
        let mut mem_cached = 0;

        for line in meminfo.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 { continue; }
            let val: u64 = parts[1].parse().unwrap_or(0);
            
            if line.starts_with("MemTotal:") { mem_total = val; }
            else if line.starts_with("MemFree:") { mem_free = val; }
            else if line.starts_with("Buffers:") { mem_buffers = val; }
            else if line.starts_with("Cached:") { mem_cached = val; }
        }

        metrics.mem_total_kb = mem_total;
        metrics.mem_used_kb = mem_total.saturating_sub(mem_free + mem_buffers + mem_cached);
    }

    metrics.process_count = std::fs::read_dir("/proc")
        .map(|entries| {
            entries.filter_map(Result::ok)
                .filter(|e| e.file_name().to_string_lossy().parse::<u32>().is_ok())
                .count()
        }).unwrap_or(0);

    metrics
}

fn collect_process_list() -> Vec<ProcessInfo> {
    let mut processes = Vec::new();
    
    if let Ok(entries) = std::fs::read_dir("/proc") {
        for entry in entries.filter_map(Result::ok) {
            let file_name = entry.file_name();
            let pid_str = file_name.to_string_lossy();
            if let Ok(pid) = pid_str.parse::<u32>() {
                if let Ok(stat) = std::fs::read_to_string(format!("/proc/{}/stat", pid)) {
                    let parts: Vec<&str> = stat.split_whitespace().collect();
                    if parts.len() >= 4 {
                        let name = parts[1].trim_matches('(').trim_matches(')').to_string();
                        let state = parts[2].to_string();
                        let ppid = parts[3].parse().unwrap_or(0);
                        
                        processes.push(ProcessInfo {
                            pid,
                            ppid,
                            name,
                            state,
                        });
                    }
                }
            }
        }
    }
    processes
}
