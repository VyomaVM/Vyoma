use anyhow::Result;
use clap::Parser;
use vyoma_agent_vm::{collect_metrics, collect_process_list, read_file_content, execute_command, AgentRequest, AgentResponse};
use std::net::SocketAddr;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpListener;
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
        let (stream, _) = listener.accept().await?;
        let mut reader = BufReader::new(stream);
        
        let mut line = String::new();
        reader.read_line(&mut line).await?;
        
        if line.is_empty() {
            continue;
        }
        
        let request: AgentRequest = match serde_json::from_str(&line) {
            Ok(req) => req,
            Err(e) => {
                let resp = AgentResponse::Error { message: format!("Invalid request: {}", e) };
                let response_json = serde_json::to_string(&resp)?;
                let mut writer = reader.into_inner();
                writer.write_all(response_json.as_bytes()).await?;
                continue;
            }
        };
        
        let response = handle_request(request).await;
        
        let response_json = serde_json::to_string(&response)?;
        
        let mut writer = reader.into_inner();
        writer.write_all(response_json.as_bytes()).await?;
        writer.flush().await?;
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
