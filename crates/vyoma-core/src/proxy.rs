use tokio::net::{TcpListener, TcpStream};
use tokio::task::JoinHandle;
use tracing::{info, error, debug};
use std::net::SocketAddr;

pub struct ProxyManager;

impl ProxyManager {
    /// Starts a TCP proxy that forwards traffic from host_port to vm_ip:vm_port.
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
                                    // info!("Proxy: Connected {} -> {}", addr, vm_addr);
                                    if let Err(e) = tokio::io::copy_bidirectional(&mut inbound, &mut outbound).await {
                                        // Connection resets are common
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
