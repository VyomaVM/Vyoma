use anyhow::Result;
use clap::Parser;
use vyoma_agent_vm::{collect_metrics, collect_process_list, read_file_content, execute_command, AgentRequest, AgentResponse};
use std::net::SocketAddr;
use tokio::net::TcpListener;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tracing::info;
use tracing_subscriber::FmtSubscriber;

const DEFAULT_TCP_PORT: u16 = 9999;

#[derive(Parser, Debug)]
struct Opts {
    #[clap(long, default_value = "tcp")]
    mode: String,
    #[clap(long, default_value_t = DEFAULT_TCP_PORT)]
    port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .init();

    let opts = Opts::parse();

    match opts.mode.as_str() {
        "tcp" => run_tcp(opts.port).await,
        _ => Err(anyhow::anyhow!("Unknown mode: {}. Use 'tcp' or 'vsock'", opts.mode)),
    }
}

async fn run_tcp(port: u16) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    
    info!("vyoma-agent-vm listening on tcp:{}", port);

    loop {
        let (mut stream, _) = listener.accept().await?;
        
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            continue;
        }
        let len = u32::from_be_bytes(len_buf) as usize;
        
        if len > 1024 * 1024 {
            let resp = AgentResponse::Error { message: "Message too large".to_string() };
            let response_json = serde_json::to_string(&resp)?;
            let response_len = response_json.len() as u32;
            stream.write_all(&response_len.to_be_bytes()).await?;
            stream.write_all(response_json.as_bytes()).await?;
            continue;
        }
        
        let mut data = vec![0u8; len];
        if stream.read_exact(&mut data).await.is_err() {
            continue;
        }
        
        let line = match String::from_utf8(data) {
            Ok(l) => l,
            Err(e) => {
                let resp = AgentResponse::Error { message: format!("Invalid UTF-8: {}", e) };
                let response_json = serde_json::to_string(&resp)?;
                let response_len = response_json.len() as u32;
                stream.write_all(&response_len.to_be_bytes()).await?;
                stream.write_all(response_json.as_bytes()).await?;
                continue;
            }
        };
        
        if line.is_empty() {
            continue;
        }
        
        let request: AgentRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = AgentResponse::Error { message: format!("Invalid request: {}", e) };
                let response_json = serde_json::to_string(&resp)?;
                let response_len = response_json.len() as u32;
                stream.write_all(&response_len.to_be_bytes()).await?;
                stream.write_all(response_json.as_bytes()).await?;
                continue;
            }
        };
        
        let response = handle_request(request).await;
        
        let response_json = serde_json::to_string(&response)?;
        let response_len = response_json.len() as u32;
        stream.write_all(&response_len.to_be_bytes()).await?;
        stream.write_all(response_json.as_bytes()).await?;
    }
}

async fn handle_request(request: AgentRequest) -> AgentResponse {
    match request {
        AgentRequest::ProcessList => {
            let processes = collect_process_list();
            AgentResponse::ProcessList(processes)
        }
        AgentRequest::GetMetrics => {
            match collect_metrics().await {
                Ok(metrics) => AgentResponse::Metrics(metrics),
                Err(e) => AgentResponse::Error { message: e.to_string() },
            }
        }
        AgentRequest::FileRead { path } => {
            match read_file_content(&path).await {
                Ok(content) => AgentResponse::FileContent(content),
                Err(e) => AgentResponse::Error { message: e.to_string() },
            }
        }
        AgentRequest::ExecCommand { cmd, env: _, workdir: _ } => {
            match execute_command(&cmd).await {
                Ok((stdout, stderr, exit_code)) => {
                    AgentResponse::ExecOutput { stdout, stderr, exit_code }
                }
                Err(e) => AgentResponse::Error { message: e.to_string() },
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cli_defaults() {
        let opts = Opts::parse_from(&["vyoma-agent-vm"]);
        assert_eq!(opts.mode, "tcp");
        assert_eq!(opts.port, DEFAULT_TCP_PORT);
    }

    #[test]
    fn test_cli_custom_port() {
        let opts = Opts::parse_from(&["vyoma-agent-vm", "--port", "8080"]);
        assert_eq!(opts.port, 8080);
    }
}