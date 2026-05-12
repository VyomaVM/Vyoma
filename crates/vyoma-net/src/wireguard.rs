use std::net::{Ipv4Addr, IpAddr, SocketAddr};
use std::path::PathBuf;
use std::os::unix::net::UnixStream;
use std::io::{BufRead, BufReader, Write};
use tracing::info;
use serde::{Deserialize, Serialize};

use boringtun::device::{DeviceConfig, DeviceHandle};
use x25519_dalek::{StaticSecret, PublicKey};
use ipnetwork::IpNetwork;
use rand::rngs::OsRng;
use base64::{Engine as _, engine::general_purpose};
use futures::TryStreamExt;
use netlink_packet_route::link::LinkAttribute;
use rtnetlink::{new_connection, Handle};

use crate::error::{NetworkError, Result};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WireGuardConfig {
    pub listen_port: u16,
    pub interface_name: String,
    pub mtu: Option<u16>,
    pub node_ip: Option<Ipv4Addr>,
}

impl Default for WireGuardConfig {
    fn default() -> Self {
        Self {
            listen_port: 51820,
            interface_name: "vyoma-wg0".to_string(),
            mtu: Some(1420),
            node_ip: None,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PeerConfig {
    pub public_key: String,
    pub endpoint: SocketAddr,
    pub allowed_ips: Vec<String>,
    pub keepalive: Option<u16>,
}

impl PeerConfig {
    pub fn new(public_key: String, endpoint: SocketAddr) -> Self {
        Self {
            public_key,
            endpoint,
            allowed_ips: vec!["0.0.0.0/0".to_string()],
            keepalive: Some(25),
        }
    }

    pub fn with_allowed_ips(mut self, ips: Vec<String>) -> Self {
        self.allowed_ips = ips;
        self
    }
}

pub struct WireGuardNode {
    pub config: WireGuardConfig,
    secret_key: StaticSecret,
    public_key: PublicKey,
    handle: Option<DeviceHandle>,
    rt_handle: Option<Handle>,
    peers: Vec<PeerConfig>,
    running: bool,
    interface_index: Option<u32>,
}

impl WireGuardNode {
    pub fn new(config: WireGuardConfig) -> Result<Self> {
        info!("Creating WireGuard node on port {}", config.listen_port);
        let secret_key = StaticSecret::random_from_rng(OsRng);
        let public_key = PublicKey::from(&secret_key);

        Ok(Self {
            config,
            secret_key,
            public_key,
            handle: None,
            rt_handle: None,
            peers: Vec::new(),
            running: false,
            interface_index: None,
        })
    }

    pub fn from_key(key_path: PathBuf, config: WireGuardConfig) -> Result<Self> {
        info!("Loading/Saving WireGuard key from/to {:?}", key_path);

        let secret_key = if key_path.exists() {
            let key_str = std::fs::read_to_string(&key_path)
                .map_err(|e| NetworkError::Io(e))?
                .trim()
                .to_string();

            let bytes = general_purpose::STANDARD.decode(&key_str)
                .map_err(|_| NetworkError::Netlink("Invalid base64 key".to_string()))?;
            let mut key_bytes = [0u8; 32];
            key_bytes.copy_from_slice(&bytes[..32]);
            StaticSecret::from(key_bytes)
        } else {
            let sk = StaticSecret::random_from_rng(OsRng);
            let encoded = general_purpose::STANDARD.encode(sk.to_bytes());
            std::fs::write(&key_path, encoded)
                .map_err(|e| NetworkError::Io(e))?;
            sk
        };

        let public_key = PublicKey::from(&secret_key);

        Ok(Self {
            config,
            secret_key,
            public_key,
            handle: None,
            rt_handle: None,
            peers: Vec::new(),
            running: false,
            interface_index: None,
        })
    }

    pub fn public_key_base64(&self) -> String {
        general_purpose::STANDARD.encode(self.public_key.as_bytes())
    }

    pub fn get_rt_handle(&self) -> Option<&Handle> {
        self.rt_handle.as_ref()
    }

    pub fn start(&mut self) -> Result<()> {
        info!("Starting WireGuard node natively via boringtun...");

        let (conn, handle, _) = new_connection()
            .map_err(|e| NetworkError::Netlink(format!("Failed to create rtnetlink connection: {}", e)))?;
        std::thread::spawn(move || {
            let rt = tokio::runtime::Builder::new_current_thread()
                .enable_all()
                .build()
                .unwrap();
            rt.block_on(conn);
        });
        self.rt_handle = Some(handle.clone());

        let dev_config = DeviceConfig {
            n_threads: 2,
            ..Default::default()
        };

        let handle = DeviceHandle::new(&self.config.interface_name, dev_config)
            .map_err(|e| NetworkError::Netlink(format!("Boringtun Device creation failed: {}", e)))?;

        self.handle = Some(handle);

        let rt_handle = self.rt_handle.as_ref()
            .ok_or_else(|| NetworkError::Netlink("rtnetlink handle not initialized".to_string()))?;

        let if_index = rtnetlink_get_interface_index(rt_handle, &self.config.interface_name)
            .map_err(|e| NetworkError::Netlink(format!("Failed to get interface index: {}", e)))?;
        self.interface_index = Some(if_index);
        info!("WireGuard interface {} has index {}", self.config.interface_name, if_index);

        if let Some(node_ip) = self.config.node_ip {
            let ip_cidr = IpNetwork::new(IpAddr::V4(ip_octets_to_ip(node_ip, 0)), 24)
                .unwrap_or_else(|_| IpNetwork::new(IpAddr::V4(Ipv4Addr::new(10, 42, 0, 1)), 24).unwrap());
            async_set_interface_ip(rt_handle, if_index, ip_cidr)
                .map_err(|e| NetworkError::Netlink(format!("Failed to set IP: {}", e)))?;
            info!("Set IP {} on WireGuard interface", ip_cidr);
        }

        async_set_interface_up(rt_handle, if_index)
            .map_err(|e| NetworkError::Netlink(format!("Failed to bring interface up: {}", e)))?;
        info!("Brought WireGuard interface up");

        // Set private key natively via boringtun Unix socket
        set_private_key(&self.config.interface_name, &self.secret_key.to_bytes())
            .map_err(|e| NetworkError::Netlink(format!("Failed to set private key: {}", e)))?;

        if self.config.listen_port > 0 {
            set_listen_port(&self.config.interface_name, self.config.listen_port)
                .map_err(|e| NetworkError::Netlink(format!("Failed to set listen port: {}", e)))?;
        }

        self.running = true;

        for peer in self.peers.clone() {
            self.apply_peer(&peer)?;
        }

        Ok(())
    }

    pub fn get_interface_index(&self) -> Option<u32> {
        self.interface_index
    }

    pub fn get_listen_port(&self) -> Option<u16> {
        None
    }

    pub fn add_peer(&mut self, peer: PeerConfig) -> Result<()> {
        info!("Adding peer {} with endpoint {}", peer.public_key, peer.endpoint);

        if self.running {
            self.apply_peer(&peer)?;
        }

        self.peers.push(peer);
        Ok(())
    }

    fn apply_peer(&mut self, peer: &PeerConfig) -> Result<()> {
        if !self.running {
            return Err(NetworkError::Netlink("WireGuard not running".to_string()));
        }

        set_wireguard_peer(&self.config.interface_name, peer)?;
        info!("Applied peer {} via native boringtun", peer.public_key);
        Ok(())
    }

    pub fn remove_peer(&mut self, public_key: &str) -> Result<()> {
        info!("Removing peer {}", public_key);

        if self.running {
            remove_wireguard_peer(&self.config.interface_name, public_key)?;
        }

        self.peers.retain(|p| p.public_key != public_key);
        Ok(())
    }

    pub fn list_peers(&self) -> &[PeerConfig] {
        &self.peers
    }

    pub fn stop(&mut self) -> Result<()> {
        info!("Stopping WireGuard node");

        for peer in self.peers.clone() {
            let _ = remove_wireguard_peer(&self.config.interface_name, &peer.public_key);
        }

        if let (Some(idx), Some(rt)) = (self.interface_index, &self.rt_handle) {
            async_del_interface(rt, idx)
                .map_err(|e| NetworkError::Netlink(format!("Failed to delete interface: {}", e)))?;
        }

        self.handle = None;
        self.rt_handle = None;
        self.interface_index = None;
        self.peers.clear();
        self.running = false;

        Ok(())
    }

    pub fn is_running(&self) -> bool {
        self.running
    }

    pub fn get_public_key_base64(&self) -> String {
        self.public_key_base64()
    }
}

fn ip_octets_to_ip(ip: Ipv4Addr, last_octet: u8) -> Ipv4Addr {
    let mut octets = ip.octets();
    octets[3] = last_octet;
    Ipv4Addr::from(octets)
}

fn rtnetlink_get_interface_index(handle: &Handle, name: &str) -> Result<u32> {
    let handle = handle.clone();
    let name = name.to_string();
    let join_result: std::thread::Result<std::result::Result<u32, NetworkError>> = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut links = handle.link().get().match_name(name).execute();
            while let Some(link) = links.try_next().await.map_err(|e| NetworkError::Netlink(e.to_string()))? {
                return Ok::<u32, NetworkError>(link.header.index);
            }
            Err(NetworkError::NotFound(format!("Interface not found")))
        })
    }).join();

    match join_result {
        Ok(result) => result,
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

fn async_set_interface_ip(handle: &Handle, if_index: u32, ip: IpNetwork) -> Result<()> {
    let handle = handle.clone();
    let ip_addr = ip.ip();
    let prefix = ip.prefix();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(handle
            .address()
            .add(if_index, ip_addr, prefix)
            .execute());
    }).join().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error")))
}

fn async_set_interface_up(handle: &Handle, if_index: u32) -> Result<()> {
    let handle = handle.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(handle
            .link()
            .set(if_index)
            .up()
            .execute());
    }).join().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error")))
}

fn async_del_interface(handle: &Handle, if_index: u32) -> Result<()> {
    let handle = handle.clone();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let _ = rt.block_on(handle
            .link()
            .del(if_index)
            .execute());
    }).join().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error")))
}

fn get_interface_index_rt(handle: &Handle, name: &str) -> Result<u32> {
    rtnetlink_get_interface_index(handle, name)
}

/// Send a set of commands to the boringtun WireGuard interface via its Unix socket.
fn wireguard_socket_command(interface_name: &str, commands: &[(&str, &str)]) -> std::result::Result<(), NetworkError> {
    let sock_path = format!("/var/run/wireguard/{}.sock", interface_name);
    let mut stream = UnixStream::connect(&sock_path)
        .map_err(|e| NetworkError::Io(e))?;

    stream.write_all(b"set=1\n")
        .map_err(|e| NetworkError::Io(e))?;

    for (key, value) in commands {
        let line = format!("{}={}\n", key, value);
        stream.write_all(line.as_bytes())
            .map_err(|e| NetworkError::Io(e))?;
    }

    stream.write_all(b"\n")
        .map_err(|e| NetworkError::Io(e))?;

    let mut reader = BufReader::new(&stream);
    let mut response = String::new();
    reader.read_line(&mut response)
        .map_err(|e| NetworkError::Io(e))?;

    if response.starts_with("errno=0") {
        Ok(())
    } else {
        Err(NetworkError::Netlink(format!("WireGuard socket command failed: {}", response.trim())))
    }
}

fn set_private_key(interface_name: &str, private_key: &[u8; 32]) -> std::result::Result<(), NetworkError> {
    let hex_key = hex::encode(private_key);
    wireguard_socket_command(interface_name, &[("private_key", &hex_key)])
}

fn set_listen_port(interface_name: &str, port: u16) -> std::result::Result<(), NetworkError> {
    wireguard_socket_command(interface_name, &[("listen_port", &port.to_string())])
}

fn set_wireguard_peer(interface_name: &str, peer_config: &PeerConfig) -> std::result::Result<(), NetworkError> {
    let pk_bytes = general_purpose::STANDARD.decode(&peer_config.public_key)
        .map_err(|_| NetworkError::Netlink("Invalid base64 public key".to_string()))?;
    let pk_hex = hex::encode(&pk_bytes);

    let endpoint_str = format!("{}:{}", peer_config.endpoint.ip(), peer_config.endpoint.port());
    let keepalive_str = peer_config.keepalive.map(|k| k.to_string()).unwrap_or_default();
    let allowed_ips_str = peer_config.allowed_ips.join(",");

    let commands: Vec<(&str, &str)> = vec![
        ("public_key", &pk_hex),
        ("replace-allowed-ips", "true"),
        ("endpoint", &endpoint_str),
        ("allowed-ips", &allowed_ips_str),
        ("persistent-keepalive-interval", &keepalive_str),
    ];

    wireguard_socket_command(interface_name, &commands)
}

fn remove_wireguard_peer(interface_name: &str, public_key: &str) -> std::result::Result<(), NetworkError> {
    let pk_bytes = general_purpose::STANDARD.decode(public_key)
        .map_err(|_| NetworkError::Netlink("Invalid base64 public key".to_string()))?;
    let pk_hex = hex::encode(&pk_bytes);

    let commands: Vec<(&str, &str)> = vec![
        ("public_key", &pk_hex),
        ("remove", "true"),
    ];

    wireguard_socket_command(interface_name, &commands)
}

pub fn add_route_to_peer_endpoint(handle: &Handle, peer_ip: &str, interface_name: &str) -> Result<()> {
    let if_index = get_interface_index_rt(handle, interface_name)?;

    let peer_addr: std::net::IpAddr = peer_ip.parse()
        .map_err(|_| NetworkError::InvalidInput(format!("Invalid peer IP: {}", peer_ip)))?;

    match peer_addr {
        std::net::IpAddr::V4(_) => add_route_v4(handle, if_index, peer_addr, 32),
        std::net::IpAddr::V6(_) => add_route_v6(handle, if_index, peer_addr, 128),
    }
}

fn add_route_v4(handle: &Handle, if_index: u32, dest: std::net::IpAddr, prefix: u8) -> Result<()> {
    let handle = handle.clone();
    let result: std::thread::Result<std::result::Result<(), rtnetlink::Error>> = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            handle.route().add()
                .v4()
                .destination_prefix(
                    match dest {
                        std::net::IpAddr::V4(ip) => ip,
                        _ => panic!("Expected IPv4"),
                    },
                    prefix,
                )
                .output_interface(if_index)
                .execute().await
        })
    }).join();

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let s = e.to_string();
            if s.contains("File exists") || s.contains("already exists") {
                Ok(())
            } else {
                Err(NetworkError::Netlink(format!("Failed to add route: {}", e)))
            }
        }
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

fn add_route_v6(handle: &Handle, if_index: u32, dest: std::net::IpAddr, prefix: u8) -> Result<()> {
    let handle = handle.clone();
    let result: std::thread::Result<std::result::Result<(), rtnetlink::Error>> = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            handle.route().add()
                .v6()
                .destination_prefix(
                    match dest {
                        std::net::IpAddr::V6(ip) => ip,
                        _ => panic!("Expected IPv6"),
                    },
                    prefix,
                )
                .output_interface(if_index)
                .execute().await
        })
    }).join();

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let s = e.to_string();
            if s.contains("File exists") || s.contains("already exists") {
                Ok(())
            } else {
                Err(NetworkError::Netlink(format!("Failed to add route: {}", e)))
            }
        }
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

pub fn add_route_to_subnet(handle: &Handle, subnet: &str, peer_ip: &str, interface_name: &str) -> Result<()> {
    let if_index = get_interface_index_rt(handle, interface_name)?;

    let subnet_addr: IpNetwork = subnet.parse()
        .map_err(|_| NetworkError::InvalidInput(format!("Invalid subnet: {}", subnet)))?;
    let gateway: std::net::IpAddr = peer_ip.parse()
        .map_err(|_| NetworkError::InvalidInput(format!("Invalid gateway IP: {}", peer_ip)))?;

    match subnet_addr {
        IpNetwork::V4(net) => {
            let gw_v4 = match gateway {
                std::net::IpAddr::V4(ip) => ip,
                _ => return Err(NetworkError::InvalidInput("Invalid IPv4 gateway".to_string())),
            };
            add_route_subnet_v4(handle, if_index, net.network(), net.prefix(), gw_v4)
        }
        IpNetwork::V6(net) => {
            let gw_v6 = match gateway {
                std::net::IpAddr::V6(ip) => ip,
                _ => return Err(NetworkError::InvalidInput("Invalid IPv6 gateway".to_string())),
            };
            add_route_subnet_v6(handle, if_index, net.network(), net.prefix(), gw_v6)
        }
    }
}

fn add_route_subnet_v4(handle: &Handle, if_index: u32, dest: Ipv4Addr, prefix: u8, gateway: Ipv4Addr) -> Result<()> {
    let handle = handle.clone();
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            handle.route().add()
                .v4()
                .destination_prefix(dest, prefix)
                .gateway(gateway)
                .output_interface(if_index)
                .execute().await
        })
    }).join();

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let s = e.to_string();
            if s.contains("File exists") || s.contains("already exists") {
                Ok(())
            } else {
                Err(NetworkError::Netlink(format!("Failed to add subnet route: {}", e)))
            }
        }
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

fn add_route_subnet_v6(handle: &Handle, if_index: u32, dest: std::net::Ipv6Addr, prefix: u8, gateway: std::net::Ipv6Addr) -> Result<()> {
    let handle = handle.clone();
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            handle.route().add()
                .v6()
                .destination_prefix(dest, prefix)
                .gateway(gateway)
                .output_interface(if_index)
                .execute().await
        })
    }).join();

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let s = e.to_string();
            if s.contains("File exists") || s.contains("already exists") {
                Ok(())
            } else {
                Err(NetworkError::Netlink(format!("Failed to add subnet route: {}", e)))
            }
        }
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

pub fn remove_route_to_subnet(handle: &Handle, subnet: &str) -> Result<()> {
    let subnet_addr: IpNetwork = subnet.parse()
        .map_err(|_| NetworkError::InvalidInput(format!("Invalid subnet: {}", subnet)))?;

    let handle = handle.clone();
    let result = std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(async {
            let mut msg = netlink_packet_route::route::RouteMessage::default();
            match subnet_addr {
                IpNetwork::V4(net) => {
                    msg.header.address_family = netlink_packet_route::AddressFamily::Inet;
                    msg.header.destination_prefix_length = net.prefix();
                    msg.attributes.push(
                        netlink_packet_route::route::RouteAttribute::Destination(
                            netlink_packet_route::route::RouteAddress::Inet(net.network()),
                        ),
                    );
                }
                IpNetwork::V6(net) => {
                    msg.header.address_family = netlink_packet_route::AddressFamily::Inet6;
                    msg.header.destination_prefix_length = net.prefix();
                    msg.attributes.push(
                        netlink_packet_route::route::RouteAttribute::Destination(
                            netlink_packet_route::route::RouteAddress::Inet6(net.network()),
                        ),
                    );
                }
            }
            msg.header.table = netlink_packet_route::route::RouteHeader::RT_TABLE_MAIN;
            msg.header.protocol = netlink_packet_route::route::RouteProtocol::Static;

            handle.route().del(msg).execute().await
        })
    }).join();

    match result {
        Ok(Ok(_)) => Ok(()),
        Ok(Err(e)) => {
            let s = e.to_string();
            if s.contains("No such file") || s.contains("No such process") {
                Ok(())
            } else {
                Err(NetworkError::Netlink(format!("Failed to remove route: {}", e)))
            }
        }
        Err(_) => Err(NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread join error"))),
    }
}

pub fn get_interface_mtu(interface_name: &str) -> Result<u32> {
    let (conn, handle, _) = new_connection()
        .map_err(|e| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, format!("Failed to create rtnetlink connection: {}", e))))?;
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        rt.block_on(conn);
    });

    let handle2 = handle.clone();
    let (tx, rx) = std::sync::mpsc::channel();
    let name = interface_name.to_string();
    std::thread::spawn(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let result = rt.block_on(async {
            let mut links = handle2.link().get().match_name(name.clone()).execute();
            while let Some(link) = links.try_next().await.map_err(|e| NetworkError::Netlink(e.to_string()))? {
                for attr in &link.attributes {
                    if let LinkAttribute::Mtu(mtu) = attr {
                        return Ok(*mtu);
                    }
                }
            }
            Err(NetworkError::NotFound(format!("Interface {} not found or no MTU", name)))
        });
        let _ = tx.send(result);
    });

    rx.recv().map_err(|_| NetworkError::Io(std::io::Error::new(std::io::ErrorKind::Other, "thread channel error")))?
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::Ipv4Addr;

    fn create_test_node() -> WireGuardNode {
        let mut config = WireGuardConfig::default();
        config.interface_name = format!("test-wg-{}", rand::random::<u16>());
        config.node_ip = Some(Ipv4Addr::new(10, 42, 0, 1));
        WireGuardNode::new(config).unwrap()
    }

    #[test]
    fn test_wireguard_node_creation() {
        let node = create_test_node();
        assert!(!node.is_running());
        assert_eq!(node.list_peers().len(), 0);
    }

    #[test]
    fn test_public_key_generation() {
        let node = create_test_node();
        let pub_key = node.public_key_base64();
        assert!(!pub_key.is_empty());
        assert_eq!(pub_key.len(), 44);
    }

    #[test]
    fn test_peer_config_creation() {
        let peer = PeerConfig::new(
            "test_public_key_base64".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 51820),
        );
        assert_eq!(peer.allowed_ips.len(), 1);
        assert_eq!(peer.keepalive, Some(25));
    }

    #[test]
    fn test_peer_config_with_allowed_ips() {
        let peer = PeerConfig::new(
            "test_public_key".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 1)), 51820),
        ).with_allowed_ips(vec![
            "10.42.0.0/24".to_string(),
            "10.43.0.0/24".to_string(),
        ]);

        assert_eq!(peer.allowed_ips.len(), 2);
        assert!(peer.allowed_ips.contains(&"10.42.0.0/24".to_string()));
        assert!(peer.allowed_ips.contains(&"10.43.0.0/24".to_string()));
    }

    #[test]
    fn test_add_peer_to_node() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "test_peer_key".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2)), 51820),
        );

        node.add_peer(peer.clone()).unwrap();
        assert_eq!(node.list_peers().len(), 1);
        assert_eq!(node.list_peers()[0].public_key, "test_peer_key");
    }

    #[test]
    fn test_remove_peer_from_node() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "peer_to_remove".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 3)), 51820),
        );

        node.add_peer(peer.clone()).unwrap();
        assert_eq!(node.list_peers().len(), 1);

        node.remove_peer("peer_to_remove").unwrap();
        assert_eq!(node.list_peers().len(), 0);
    }

    #[test]
    fn test_multiple_peers() {
        let mut node = create_test_node();

        for i in 0..5 {
            let peer = PeerConfig::new(
                format!("peer_key_{}", i),
                SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 10 + i)), 51820),
            );
            node.add_peer(peer).unwrap();
        }

        assert_eq!(node.list_peers().len(), 5);
    }

    #[test]
    fn test_stop_clears_state() {
        let mut node = create_test_node();
        let peer = PeerConfig::new(
            "test_peer".to_string(),
            SocketAddr::new(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 4)), 51820),
        );

        node.add_peer(peer).unwrap();
        assert_eq!(node.list_peers().len(), 1);

        node.stop().unwrap();
        assert_eq!(node.list_peers().len(), 0);
        assert!(!node.is_running());
    }

    #[test]
    fn test_config_defaults() {
        let config = WireGuardConfig::default();
        assert_eq!(config.listen_port, 51820);
        assert_eq!(config.interface_name, "vyoma-wg0");
        assert_eq!(config.mtu, Some(1420));
        assert!(config.node_ip.is_none());
    }

    #[test]
    fn test_config_custom_values() {
        let mut config = WireGuardConfig::default();
        config.listen_port = 12345;
        config.interface_name = "custom-wg".to_string();
        config.mtu = Some(1500);
        config.node_ip = Some(Ipv4Addr::new(10, 100, 0, 1));

        assert_eq!(config.listen_port, 12345);
        assert_eq!(config.interface_name, "custom-wg");
        assert_eq!(config.mtu, Some(1500));
        assert_eq!(config.node_ip, Some(Ipv4Addr::new(10, 100, 0, 1)));
    }

    #[test]
    #[ignore = "Requires real network interface, using rtnetlink native API"]
    fn test_get_interface_mtu_invalid() {
        let _ = get_interface_mtu("nonexistent-interface-xyz");
    }

    #[test]
    #[ignore = "Requires real WireGuard interface, using native boringtun socket API"]
    fn test_route_functions_handling() {
        // These now require a real WireGuard interface, marked as ignore
    }
}