use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::codec::{Framed, LengthDelimitedCodec};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentRequest {
    ProcessList,
    ExecCommand {
        cmd: Vec<String>,
        env: HashMap<String, String>,
        workdir: Option<String>,
    },
    GetMetrics,
    FileRead {
        path: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AgentResponse {
    ProcessList {
        processes: Vec<ProcessInfo>,
    },
    ExecOutput {
        stdout: Vec<u8>,
        stderr: Vec<u8>,
        exit_code: i32,
    },
    Metrics {
        cpu_user_ms: u64,
        cpu_system_ms: u64,
        mem_used_kb: u64,
        mem_total_kb: u64,
        process_count: usize,
    },
    FileContent {
        content: Vec<u8>,
    },
    Error {
        message: String,
    },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ProcessInfo {
    pub pid: u32,
    pub ppid: u32,
    pub name: String,
    pub state: String,
}

pub struct AgentClient {
    socket_path: String,
}

impl AgentClient {
    pub fn new(socket_path: impl Into<String>) -> Self {
        Self {
            socket_path: socket_path.into(),
        }
    }

    pub async fn connect(&self) -> Result<Framed<UnixStream, LengthDelimitedCodec>> {
        let mut stream = UnixStream::connect(&self.socket_path)
            .await
            .context(format!("Failed to connect to vsock at {}", self.socket_path))?;

        // Cloud Hypervisor vhost-user vsock / Unix socket requires "CONNECT <port>\n"
        stream.write_all(b"CONNECT 9999\n").await?;
        
        let mut response = [0u8; 32];
        let mut line = String::new();
        // Read until newline
        loop {
            let mut buf = [0u8; 1];
            stream.read_exact(&mut buf).await?;
            if buf[0] == b'\n' {
                break;
            }
            line.push(buf[0] as char);
        }

        if !line.starts_with("OK") {
            anyhow::bail!("Failed to connect to guest agent: {}", line);
        }

        let framed = Framed::new(stream, LengthDelimitedCodec::new());
        Ok(framed)
    }

    pub async fn send_request(&self, request: AgentRequest) -> Result<AgentResponse> {
        let mut framed = self.connect().await?;
        
        let request_bytes = serde_json::to_vec(&request)?;
        framed.send(Bytes::from(request_bytes)).await?;

        if let Some(frame_res) = framed.next().await {
            let frame = frame_res?;
            let response: AgentResponse = serde_json::from_slice(&frame)?;
            Ok(response)
        } else {
            anyhow::bail!("Agent closed connection without response")
        }
    }

    pub async fn get_metrics(&self) -> Result<AgentResponse> {
        self.send_request(AgentRequest::GetMetrics).await
    }

    pub async fn process_list(&self) -> Result<AgentResponse> {
        self.send_request(AgentRequest::ProcessList).await
    }

    pub async fn exec(&self, cmd: Vec<String>, env: HashMap<String, String>, workdir: Option<String>) -> Result<AgentResponse> {
        self.send_request(AgentRequest::ExecCommand { cmd, env, workdir }).await
    }
}
