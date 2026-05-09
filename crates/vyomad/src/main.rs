use axum::{
    routing::{get, post},
    Router,
};
use tracing::{info, error, warn};
use tokio::net::UnixListener;
use tokio::sync::{broadcast, Mutex as TokioMutex};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use std::os::unix::fs::PermissionsExt;
use tower_http::cors::CorsLayer;
use tower_service::Service;
use hyper_util::rt::{TokioExecutor, TokioIo};
use hyper_util::server::conn::auto;

use vyoma_core::cgroups::CgroupManager;
use vyoma_core::policy::PolicyManager;

use clap::Parser;

mod cluster;
mod dns;
mod ui;
mod state;
mod api;
mod swarm;
mod metrics;
mod timemachine;
mod auto_snapshot;
mod hibernation;
mod grpc;
mod vm_service;
#[cfg(feature = "chaos")]
mod chaos;
#[cfg(feature = "chaos")]
mod chaos_tests;

use state::{AppState, wal::Wal, recovery::Recovery};
use api::handlers;

#[cfg(feature = "chaos")]
use chaos::enable_chaos_on_startup;

#[derive(Parser, Debug)]
#[command(name = "vyomad", about = "Ignite MicroVM Daemon", version)]
struct Args {
    /// Path to listen on (Unix Socket)
    #[arg(short, long, default_value = "/run/vyoma/vyoma.sock")]
    socket_path: String,
    /// HTTP port for dashboard (default: 3000, use 0 to disable)
    #[arg(short = 'p', long, default_value_t = 3000)]
    http_port: u16,
    /// Data directory containing kernel and firecracker binaries
    #[arg(short, long, default_value = "/var/lib/ignite")]
    data_dir: String,
    /// Enable chaos mode for crash injection testing
    #[arg(long)]
    chaos_mode: bool,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Parse Args (Handles --help / --version)
    let args = Args::parse();

    // Root requirement stripped in favor of AmbientCapabilities (ADR-022)

    info!("vyomad (Ignite Daemon) starting up (Unix Socket: {})...", args.socket_path);

    #[cfg(feature = "chaos")]
    {
        if args.chaos_mode {
            chaos::enable_chaos_on_startup(std::path::Path::new(&args.data_dir));
            info!("Chaos mode enabled!");
        }
    }

    let cgroups = CgroupManager::new();
    if let Err(e) = cgroups.init() {
        error!("Failed to init cgroups: {}", e);
    }
    let cgroups = Arc::new(cgroups);

    // CNI Initialization
    let home = dirs::home_dir().expect("No home dir");
    let cni_config_dir = home.join(".ignite").join("cni").join("net.d");
    let cni_plugin_dir = home.join(".ignite").join("cni").join("bin");

    std::fs::create_dir_all(&cni_config_dir).unwrap();
    std::fs::create_dir_all(&cni_plugin_dir).unwrap();

    // Create default bridge
    let bridge_conf = cni_config_dir.join("10-ignite-bridge.conf");
    if !bridge_conf.exists() {
        let conf = r#"{
    "cniVersion": "0.4.0",
    "name": "vyoma-net",
    "type": "bridge",
    "bridge": "ign0",
    "isGateway": true,
    "ipMasq": true,
    "ipam": {
        "type": "host-local",
        "subnet": "172.16.0.0/24",
        "routes": [ { "dst": "0.0.0.0/0" } ]
    }
}"#;
        std::fs::write(&bridge_conf, conf).unwrap();
    }

    let cni_manager = Arc::new(vyoma_core::cni::CniManager::new(
        cni_plugin_dir,
        cni_config_dir,
    ));

    // Initialize WAL (ADR-023)
    let state_dir = home.join(".ignite").join("state");
    let (_db, wal) = match Wal::open_or_create(&state_dir) {
        Ok((db, wal)) => {
            info!("WAL initialized successfully");
            (db, Arc::new(wal))
        }
        Err(e) => {
            error!("Failed to initialize WAL: {}", e);
            panic!("Cannot start without WAL");
        }
    };

    // Run crash recovery
    match Recovery::recover_on_startup(&home, &wal) {
        Ok(recovered_vms) => {
            for vm in &recovered_vms {
                info!("Recovered VM: {} with status: {:?}", vm.vm_id, vm.status);
            }
        }
        Err(e) => {
            warn!("Recovery scan failed: {}", e);
        }
    }

    let (events_tx, _rx) = broadcast::channel(100);

    // Initialize Raft Swarm
    let node_id = 1; // Default to node 1 for now
    let raft_config = std::sync::Arc::new(openraft::Config {
        heartbeat_interval: 500,
        election_timeout_min: 1500,
        election_timeout_max: 3000,
        ..Default::default()
    }.validate().unwrap_or_default());
    let raft_network = crate::swarm::raft_network::SwarmNetwork::new();
    let db_path = std::path::Path::new(&args.data_dir).join("raft_db");
    let sled_db = sled::open(&db_path).expect("Failed to open Sled DB for Raft");
    let raft_store = crate::swarm::raft_store::SwarmStore::new(wal.clone(), sled_db.clone());
    let (log_store, state_machine) = openraft::storage::Adaptor::new(raft_store);
    let raft = openraft::Raft::new(node_id, raft_config.clone(), raft_network, log_store, state_machine).await;
    
    let raft = match raft {
        Ok(r) => Some(r),
        Err(e) => {
            error!("Failed to initialize Raft: {}", e);
            None
        }
    };

    let timemachine = Arc::new(tokio::sync::RwLock::new(timemachine::TimeMachine::new(&sled_db)));

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
        cgroups,
        cni_manager,
        cluster_manager: Arc::new(cluster::ClusterManager::new()),
        rootless: false, // Enforced false
        events_tx,
        wal,
        data_dir: args.data_dir.clone(),
        raft,
        timemachine,
        policy_manager: Arc::new(StdMutex::new(PolicyManager::new())),
    };

    api::handlers::initialize_state(&state).await;

    api::handlers::start_process_monitor(state.clone()).await;

    // Start DNS
    dns::start_dns_server(state.clone()).await;

    let shutdown_state = state.clone();

    let app = Router::new()
        .route("/health", get(handlers::health_check))
        .route("/run", post(handlers::run_vm))
        .route("/pull", post(handlers::pull_image_handler))
        .route("/stop/:id", post(handlers::stop_vm))
        .route("/pause/:id", post(handlers::pause_vm))
        .route("/resume/:id", post(handlers::resume_vm))
        .route("/ps", get(handlers::list_vms))
        .route("/commit/:id", post(handlers::commit_vm))
        .route("/snapshot/:id", post(handlers::snapshot_vm))
        .route("/history/:id", get(handlers::history_vm))
        .route("/time-travel", post(handlers::time_travel_vm))
        .route("/restore", post(handlers::restore_vm))
        .route("/logs/:id", get(handlers::stream_logs))
        .route("/build", post(handlers::build_image))
        .route("/images", get(handlers::list_images_handler))
        .route("/volumes", get(handlers::list_volumes_handler))
        .route(
            "/networks",
            get(handlers::list_networks_handler).post(handlers::create_network_handler),
        )
        .route(
            "/networks/:name",
            axum::routing::delete(handlers::delete_network_handler),
        )
        .route("/events", get(handlers::events_handler))
        .route("/vms/:id", get(handlers::inspect_vm_handler))
        .route("/swarm/init", post(handlers::swarm_init_handler))
        .route("/swarm/join", post(handlers::swarm_join_handler))
        .route("/swarm/register", post(handlers::swarm_register_handler))
        .route("/swarm/nodes", get(handlers::swarm_nodes_handler))
        .route("/raft/append", post(handlers::raft_append_handler))
        .route("/raft/snapshot", post(handlers::raft_snapshot_handler))
        .route("/raft/vote", post(handlers::raft_vote_handler))
        .route("/teleport", post(handlers::teleport_handler))
        .route("/receive-teleport", post(handlers::receive_teleport_handler))
        .route("/policy", get(handlers::get_policy_handler).post(handlers::set_policy_handler))
        .fallback(ui::ui_handler)
        .layer(CorsLayer::permissive())
        .with_state(state.clone());

    let socket_path = args.socket_path;
    let path = std::path::Path::new(&socket_path);
    if let Some(parent) = path.parent() {
        // Try to create directory
        match std::fs::create_dir_all(parent) {
            Ok(_) => {}
            Err(e) => {
                warn!("Cannot create directory {}: {}", parent.display(), e);
            }
        }
    }
    
    // Try to bind, if fails try alternative locations
    let actual_socket_path: String;
    let listener = match UnixListener::bind(&socket_path) {
        Ok(l) => {
            actual_socket_path = socket_path.clone();
            l
        }
        Err(_) => {
            // Try alternative: XDG_RUNTIME_DIR or /tmp
            let alt_dir = std::env::var("XDG_RUNTIME_DIR")
                .map(|d| std::path::PathBuf::from(d))
                .unwrap_or_else(|_| std::env::temp_dir());
            
            let alt_socket_path = alt_dir.join("vyoma.sock");
            warn!("Cannot bind to {}. Trying alternative: {}", socket_path, alt_socket_path.display());
            
            let _ = std::fs::remove_file(&alt_socket_path);
            match UnixListener::bind(&alt_socket_path) {
                Ok(l) => {
                    actual_socket_path = alt_socket_path.to_string_lossy().to_string();
                    warn!("Using alternative socket at {}", actual_socket_path);
                    l
                }
                Err(e) => {
                    error!("Failed to bind socket at any location: {}", e);
                    std::process::exit(1);
                }
            }
        }
    };

    // Set socket permissions: root:vyoma (0660) - users in vyoma group can access
    let permissions = std::fs::Permissions::from_mode(0o660);
    if let Err(e) = std::fs::set_permissions(&actual_socket_path, permissions) {
        warn!("Could not set 0660 permissions on socket: {}", e);
    }
    
    info!("Daemon listening on Unix Socket {}", actual_socket_path);

    // Start HTTP server for dashboard if port is not 0
    if args.http_port > 0 {
        let http_app = app.clone();
        tokio::spawn(async move {
            let addr = format!("0.0.0.0:{}", args.http_port);
            info!("Dashboard available at http://localhost:{}", args.http_port);
            let listener = tokio::net::TcpListener::bind(&addr).await.unwrap();
            loop {
                match listener.accept().await {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        let tower_service = http_app.clone();
                        tokio::spawn(async move {
                            let hyper_service = hyper::service::service_fn(move |request| {
                                tower_service.clone().call(request)
                            });
                            let _ = auto::Builder::new(TokioExecutor::new())
                                .serve_connection(io, hyper_service)
                                .await;
                        });
                    }
                    Err(e) => error!("HTTP accept error: {}", e),
                }
            }
        });
    }

    let grpc_state = state.clone();
    tokio::spawn(async move {
        let addr = "[::1]:7071".parse().unwrap();
        info!("gRPC interface available at {}", addr);
        use vyoma_proto::v1::vm_service_server::VmServiceServer;
        
        let svc = VmServiceServer::new(grpc::GrpcVmService::new(grpc_state.clone()));
        
        if let Err(e) = tonic::transport::Server::builder()
            .add_service(svc)
            .serve(addr)
            .await
        {
            error!("gRPC server error: {}", e);
        }
    });

    let shutdown_rx = handlers::shutdown_signal(shutdown_state);
    let mut shutdown_flag = Box::pin(shutdown_rx);

    loop {
        tokio::select! {
            result = listener.accept() => {
                match result {
                    Ok((stream, _)) => {
                        let io = TokioIo::new(stream);
                        let tower_service = app.clone();
                        
                        tokio::spawn(async move {
                            let hyper_service = hyper::service::service_fn(move |request: axum::extract::Request<hyper::body::Incoming>| {
                                tower_service.clone().call(request)
                            });

                            if let Err(err) = auto::Builder::new(TokioExecutor::new())
                                .serve_connection(io, hyper_service)
                                .await
                            {
                                error!("Error parsing connection: {:?}", err);
                            }
                        });
                    }
                    Err(e) => error!("Failed to accept connection: {}", e),
                }
            }
            _ = &mut shutdown_flag => {
                info!("Daemon shutting down gracefully...");
                break;
            }
        }
    }
    
    let _ = std::fs::remove_file(&socket_path);
}
