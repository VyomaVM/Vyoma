use axum::{
    routing::{get, post},
    Router,
};
use tracing::{info, error, warn};
use tokio::net::TcpListener;
use tokio::sync::{broadcast, Mutex as TokioMutex};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use tower_http::cors::CorsLayer;

use ignite_core::cgroups::CgroupManager;

use clap::Parser;

mod cluster;
mod dns;
mod ui;
mod state;
mod api;

use state::{AppState, wal::Wal, recovery::Recovery};
use api::handlers;

#[derive(Parser, Debug)]
#[command(name = "ignited", about = "Ignite MicroVM Daemon", version)]
struct Args {
    /// Port to listen on
    #[arg(short, long, default_value = "3000")]
    port: u16,

    /// Host interface to bind to
    #[arg(long, default_value = "127.0.0.1")]
    host: String,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    // Parse Args (Handles --help / --version)
    let args = Args::parse();

    // Root requirement stripped in favor of AmbientCapabilities (ADR-022)

    info!("ignited (Ignite Daemon) starting up on {}:{}...", args.host, args.port);

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

    let addr = format!("{}:{}", args.host, args.port);
    let listener = TcpListener::bind(&addr).await.unwrap();
    info!("Daemon listening on TCP {}", listener.local_addr().unwrap());

    axum::serve(listener, app)
        .with_graceful_shutdown(handlers::shutdown_signal(shutdown_state))
        .await
        .unwrap();
}
