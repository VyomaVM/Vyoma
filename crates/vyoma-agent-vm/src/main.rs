use anyhow::{Context, Result};
use clap::Parser;
use vyoma_agent_vm::{collect_metrics, collect_process_list, read_file_content, execute_command};
use vyoma_agent_protocol::{AgentRequest, AgentResponse};
use std::net::SocketAddr;
use std::time::Duration;
use tokio::net::{TcpListener, TcpStream};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio_vsock::{VsockAddr, VsockListener, VsockStream, VMADDR_CID_ANY};
use tracing::{info, warn, error, debug};
use tracing_subscriber::FmtSubscriber;

const DEFAULT_PORT: u16 = 9999;
const DEFAULT_PROXY_PORT: u16 = 10000;

#[derive(Parser, Debug)]
struct Opts {
    #[clap(long, default_value = "vsock")]
    mode: String,
    #[clap(long, default_value_t = DEFAULT_PORT)]
    port: u16,
    #[clap(long, default_value_t = DEFAULT_PROXY_PORT)]
    proxy_port: u16,
}

#[tokio::main]
async fn main() -> Result<()> {
    FmtSubscriber::builder()
        .with_max_level(tracing::Level::INFO)
        .init();

    let opts = Opts::parse();

    match opts.mode.as_str() {
        "vsock" => run_vsock(opts.port, opts.proxy_port).await,
        "tcp" => run_tcp(opts.port).await,
        _ => Err(anyhow::anyhow!("Unknown mode: {}. Use 'vsock' or 'tcp'", opts.mode)),
    }
}

async fn run_vsock(agent_port: u16, proxy_port: u16) -> Result<()> {
    info!("Starting vyoma-agent-vm in VSOCK mode (agent: {}, proxy: {})", agent_port, proxy_port);
    
    // 1. Spawn L4 Port Forwarding Proxy on vsock:proxy_port
    let proxy_handle = tokio::spawn(async move {
        if let Err(e) = run_vsock_proxy(proxy_port).await {
            error!("Vsock proxy service encountered fatal error: {}", e);
        }
    });

    // 2. Run In-VM Agent Management / Introspection service on vsock:agent_port
    let agent_handle = tokio::spawn(async move {
        if let Err(e) = run_vsock_agent(agent_port).await {
            error!("Vsock agent management service encountered fatal error: {}", e);
        }
    });

    tokio::select! {
        res = proxy_handle => {
            warn!("Vsock proxy listener exited: {:?}", res);
        }
        res = agent_handle => {
            warn!("Vsock agent management listener exited: {:?}", res);
        }
    }

    Ok(())
}

async fn run_vsock_proxy(port: u16) -> Result<()> {
    let addr = VsockAddr::new(VMADDR_CID_ANY, port as u32);
    let listener = VsockListener::bind(addr)
        .context(format!("Failed to bind vsock proxy listener on port {}", port))?;
    
    info!("vyoma-agent-vm L4 port-forwarding proxy listening on vsock:{}", port);

    loop {
        match listener.accept().await {
            Ok((mut vsock_stream, peer_addr)) => {
                debug!("Accepted vsock forwarding connection from {:?}", peer_addr);
                tokio::spawn(async move {
                    if let Err(e) = handle_proxy_conn(&mut vsock_stream).await {
                        debug!("Proxy stream ended/error: {}", e);
                    }
                });
            }
            Err(e) => {
                error!("Vsock proxy accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn handle_proxy_conn(vsock_stream: &mut VsockStream) -> Result<()> {
    // 1. Read target container port line (e.g. "80\n")
    let mut reader = BufReader::new(vsock_stream);
    let mut port_str = String::new();
    reader.read_line(&mut port_str).await
        .context("Failed to read target port line from host vsock proxy")?;

    let target_port: u16 = port_str.trim().parse()
        .map_err(|e| anyhow::anyhow!("Invalid target port '{}': {}", port_str.trim(), e))?;

    // 2. Connect to local container process at 127.0.0.1:<target_port>
    // Retry up to 30 times (with 50ms interval = 1.5s total) in case the application is initializing
    let mut tcp_stream = None;
    for _ in 0..30 {
        match TcpStream::connect(("127.0.0.1", target_port)).await {
            Ok(stream) => {
                tcp_stream = Some(stream);
                break;
            }
            Err(_) => {
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }

    let vsock_raw = reader.into_inner();
    let mut local_tcp = match tcp_stream {
        Some(s) => {
            vsock_raw.write_all(b"OK\n").await
                .context("Failed to send OK acknowledgement to host vsock proxy")?;
            s
        }
        None => {
            let _ = vsock_raw.write_all(b"ERR connection refused\n").await;
            anyhow::bail!("Failed to connect to container on 127.0.0.1:{}", target_port);
        }
    };

    // 3. Bidirectional splice between vsock stream and local container TCP stream
    tokio::io::copy_bidirectional(vsock_raw, &mut local_tcp).await?;
    Ok(())
}

async fn run_vsock_agent(port: u16) -> Result<()> {
    let addr = VsockAddr::new(VMADDR_CID_ANY, port as u32);
    let listener = VsockListener::bind(addr)
        .context(format!("Failed to bind vsock agent listener on port {}", port))?;
    
    info!("vyoma-agent-vm management protocol listening on vsock:{}", port);

    loop {
        match listener.accept().await {
            Ok((mut stream, _)) => {
                tokio::spawn(async move {
                    let _ = process_agent_messages(&mut stream).await;
                });
            }
            Err(e) => {
                error!("Vsock agent accept error: {}", e);
                tokio::time::sleep(Duration::from_millis(50)).await;
            }
        }
    }
}

async fn run_tcp(port: u16) -> Result<()> {
    let addr = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = TcpListener::bind(addr).await?;
    info!("vyoma-agent-vm listening on tcp:{}", port);

    loop {
        let (mut stream, _) = listener.accept().await?;
        tokio::spawn(async move {
            let _ = process_agent_messages(&mut stream).await;
        });
    }
}

async fn process_agent_messages<S: AsyncReadExt + AsyncWriteExt + Unpin>(stream: &mut S) -> Result<()> {
    loop {
        let mut len_buf = [0u8; 4];
        if stream.read_exact(&mut len_buf).await.is_err() {
            break;
        }
        let len = u32::from_be_bytes(len_buf) as usize;

        if len > 1024 * 1024 {
            let resp = AgentResponse::Error { message: "Message too large".to_string() };
            let response_json = serde_json::to_string(&resp)?;
            let response_len = response_json.len() as u32;
            stream.write_all(&response_len.to_be_bytes()).await?;
            stream.write_all(response_json.as_bytes()).await?;
            break;
        }

        let mut data = vec![0u8; len];
        if stream.read_exact(&mut data).await.is_err() {
            break;
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
    Ok(())
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
        assert_eq!(opts.mode, "vsock");
        assert_eq!(opts.port, DEFAULT_PORT);
        assert_eq!(opts.proxy_port, DEFAULT_PROXY_PORT);
    }

    #[test]
    fn test_cli_custom_port() {
        let opts = Opts::parse_from(&["vyoma-agent-vm", "--mode", "tcp", "--port", "8080"]);
        assert_eq!(opts.mode, "tcp");
        assert_eq!(opts.port, 8080);
    }
}