use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::info;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream};

const DEFAULT_PORT: u16 = 9000;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRequest {
    ProcessList,
    ExecCommand { cmd: Vec<String>, env: HashMap<String, String>, workdir: Option<String> },
    GetMetrics,
    FileRead { path: String },
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt::init();
    info!("Starting ignite-agent on port {}", DEFAULT_PORT);
    
    let listener = TcpListener::bind(format!("0.0.0.0:{}", DEFAULT_PORT)).await?;
    
    loop {
        let (stream, addr) = listener.accept().await?;
        info!("Client connected: {}", addr);
        
        let mut reader = BufReader::new(stream);
        let mut line = String::new();
        
        if reader.read_line(&mut line).await? == 0 {
            continue;
        }
        
        let request: AgentRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = serde_json::json!({"type": "Error", "message": e.to_string()});
                let mut writer = reader.into_inner();
                writer.write_all(resp.to_string().as_bytes()).await?;
                continue;
            }
        };
        
        let response = handle_request(request);
        let response_json = serde_json::to_string(&response)?;
        
        let mut writer = reader.into_inner();
        writer.write_all(response_json.as_bytes()).await?;
        writer.flush().await?;
    }
}

fn handle_request(request: AgentRequest) -> serde_json::Value {
    match request {
        AgentRequest::ProcessList => {
            serde_json::json!({"type": "ProcessList", "processes": []})
        }
        AgentRequest::GetMetrics => {
            serde_json::json!({
                "type": "Metrics",
                "cpu_usage_percent": 0.0,
                "mem_used_kb": 0,
                "mem_total_kb": 0,
                "process_count": 0
            })
        }
        AgentRequest::FileRead { path } => {
            serde_json::json!({"type": "FileContent", "content": ""})
        }
        AgentRequest::ExecCommand { .. } => {
            serde_json::json!({
                "type": "ExecOutput",
                "stdout": [],
                "stderr": [],
                "exit_code": 0
            })
        }
    }
}
