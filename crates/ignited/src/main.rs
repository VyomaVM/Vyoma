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

use ignite_core::cgroups::CgroupManager;

use clap::Parser;

mod cluster;
mod dns;
mod ui;
mod state;
mod api;
mod swarm;

use state::{AppState, wal::Wal, recovery::Recovery};
use api::handlers;

#[derive(Parser, Debug)]
#[command(name = "ignited", about = "Ignite MicroVM Daemon", version)]
struct Args {
    /// Path to listen on (Unix Socket)
    #[arg(short, long, default_value = "/var/run/ignite/ignite.sock")]
    socket_path: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Parse Args (Handles --help / --version)
    let args = Args::parse();

    // Root requirement stripped in favor of AmbientCapabilities (ADR-022)

    info!("ignited (Ignite Daemon) starting up (Unix Socket: {})...", args.socket_path);

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
    "name": "ignite-net",
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

    let cni_manager = Arc::new(ignite_core::cni::CniManager::new(
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

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
        cgroups,
        cni_manager,
        cluster_manager: Arc::new(cluster::ClusterManager::new()),
        rootless: false, // Enforced false
        events_tx,
        wal,
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
        .route("/snapshot/:id", post(handlers::snapshot_vm))
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
        .fallback(ui::ui_handler)
        .layer(CorsLayer::permissive())
        .with_state(state);

    let socket_path = args.socket_path;
    let path = std::path::Path::new(&socket_path);
    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }
    let _ = std::fs::remove_file(&socket_path);
    let listener = UnixListener::bind(&socket_path).unwrap();

    let permissions = std::fs::Permissions::from_mode(0o660);
    if let Err(e) = std::fs::set_permissions(&socket_path, permissions) {
        warn!("Could not set 0660 permissions on socket: {}. You may need root.", e);
    }
    
    // Attempt to set ownership to root:ignite if possible
    let _ = std::process::Command::new("chgrp")
        .arg("ignite")
        .arg(&socket_path)
        .output();
        
    info!("Daemon listening on Unix Socket {}", socket_path);

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
