use anyhow::{Context, Result};
use std::net::SocketAddr;
use std::path::{Path, PathBuf};
use std::time::Duration;
use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
use tokio::net::{TcpListener, TcpStream, UnixStream};
use tokio::task::JoinHandle;
use tracing::{debug, error, info};

pub struct ProxyManager;

impl ProxyManager {
    /// Starts a rootless Vsock L4 stream proxy.
    /// Inbound TCP traffic arriving on host_port is tunneled through Cloud-Hypervisor's
    /// Unix domain vsock socket to the in-VM guest agent, which connects to 127.0.0.1:vm_port.
    /// This requires ZERO root privileges, ZERO TAP devices, and ZERO iptables/nftables manipulation.
    pub fn start_vsock_proxy(
        host_port: u16,
        vsock_path: PathBuf,
        vm_port: u16,
    ) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!(
                "Starting Rootless Vsock L4 Proxy: 0.0.0.0:{} -> vsock (guest 127.0.0.1:{})",
                host_port, vm_port
            );

            let addr = SocketAddr::from(([0, 0, 0, 0], host_port));
            let listener = match TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Vsock proxy failed to bind host port {}: {}", host_port, e);
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((mut inbound, client_addr)) => {
                        debug!("Vsock proxy accepted connection from {}", client_addr);
                        let vsock_path = vsock_path.clone();
                        tokio::spawn(async move {
                            if let Err(e) = forward_vsock(&mut inbound, &vsock_path, vm_port).await {
                                debug!("Vsock proxy connection error (client {}): {}", client_addr, e);
                            }
                        });
                    }
                    Err(e) => {
                        error!("Vsock proxy accept error on port {}: {}", host_port, e);
                    }
                }
            }
        })
    }

    /// Legacy / IP-based TCP proxy that forwards traffic from host_port to vm_ip:vm_port.
    /// Returns a JoinHandle that can be aborted to stop the proxy.
    pub fn start_proxy(host_port: u16, vm_ip: String, vm_port: u16) -> JoinHandle<()> {
        tokio::spawn(async move {
            info!("Starting TCP Proxy: 0.0.0.0:{} -> {}:{}", host_port, vm_ip, vm_port);
            
            let addr = SocketAddr::from(([0, 0, 0, 0], host_port));
            let listener = match TcpListener::bind(addr).await {
                Ok(l) => l,
                Err(e) => {
                    error!("Proxy bind error on port {}: {}", host_port, e);
                    return;
                }
            };

            loop {
                match listener.accept().await {
                    Ok((mut inbound, addr)) => {
                        debug!("Proxy: Pending connection from {}", addr);
                        let vm_addr = format!("{}:{}", vm_ip, vm_port);
                        
                        tokio::spawn(async move {
                            match TcpStream::connect(&vm_addr).await {
                                Ok(mut outbound) => {
                                    if let Err(e) = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await {
                                        debug!("Proxy transfer ended/error: {}", e);
                                    }
                                }
                                Err(e) => {
                                    error!("Proxy failed to connect to VM {}: {}", vm_addr, e);
                                }
                            }
                        });
                    }
                    Err(e) => {
                        error!("Proxy accept error: {}", e);
                    }
                }
            }
        })
    }
}

async fn forward_vsock(
    inbound: &mut TcpStream,
    vsock_path: &Path,
    vm_port: u16,
) -> Result<()> {
    // Retry connecting to vsock.sock for up to 3 seconds if the VM is still initializing
    let mut outbound = None;
    for _ in 0..30 {
        if vsock_path.exists() {
            if let Ok(stream) = UnixStream::connect(vsock_path).await {
                outbound = Some(stream);
                break;
            }
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }

    let mut outbound = outbound.ok_or_else(|| {
        anyhow::anyhow!("MicroVM vsock socket not accessible at {:?}", vsock_path)
    })?;

    // 1. Cloud-Hypervisor handshake: Send "CONNECT 10000\n" to connect to guest agent multiplexer
    outbound.write_all(b"CONNECT 10000\n").await
        .context("Failed to send CONNECT command to Cloud-Hypervisor vsock backend")?;

    // 2. Read Cloud-Hypervisor response: "OK <assigned_port>\n"
    let mut reader = BufReader::new(outbound);
    let mut ch_response = String::new();
    reader.read_line(&mut ch_response).await
        .context("Failed to read handshake response from Cloud-Hypervisor vsock backend")?;

    if !ch_response.starts_with("OK") {
        anyhow::bail!(
            "Cloud-Hypervisor rejected vsock connection to guest port 10000: {}",
            ch_response.trim()
        );
    }

    // 3. Send target container port to vyoma-agent-vm: "<vm_port>\n"
    let mut outbound = reader.into_inner();
    outbound.write_all(format!("{}\n", vm_port).as_bytes()).await
        .context("Failed to send target container port to vyoma-agent-vm")?;

    // 4. Read vyoma-agent-vm confirmation: "OK\n"
    let mut reader = BufReader::new(outbound);
    let mut agent_response = String::new();
    reader.read_line(&mut agent_response).await
        .context("Failed to read confirmation from vyoma-agent-vm")?;

    if !agent_response.starts_with("OK") {
        anyhow::bail!(
            "vyoma-agent-vm rejected port forward for port {}: {}",
            vm_port,
            agent_response.trim()
        );
    }

    // 5. Splice TCP stream and vsock stream bidirectionally
    let mut outbound = reader.into_inner();
    tokio::io::copy_bidirectional(inbound, &mut outbound).await
        .context("Vsock stream transfer terminated")?;

    Ok(())
}
