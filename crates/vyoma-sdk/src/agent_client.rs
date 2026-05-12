use anyhow::{Context, Result};
use bytes::Bytes;
use futures::{SinkExt, StreamExt};
use std::collections::HashMap;
use tokio::net::UnixStream;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio_util::codec::{Framed, LengthDelimitedCodec};
use vyoma_agent_protocol::{AgentRequest, AgentResponse};

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

        stream.write_all(b"CONNECT 9999\n").await?;
        
        let mut line = String::new();
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