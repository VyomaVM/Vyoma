use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::State,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex};
use tracing::info;
use tokio::net::TcpListener;

#[derive(Clone)]
struct AppState {
    // In a real app, this would track multiple VMs.
    // Map<VmId, VmState>
    // For now, simple single-instance POC state
    active_vm: Arc<Mutex<Option<String>>>, // stores ID of active VM
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("ignited (Ignite Daemon) starting up...");

    let state = AppState {
        active_vm: Arc::new(Mutex::new(None)),
    };

    // build our application with a route
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // Run VM
        .route("/run", post(run_vm))
        .with_state(state);

    let listener = TcpListener::bind("127.0.0.1:3000").await.unwrap();
    info!("Daemon listening on {}", listener.local_addr().unwrap());
    
    axum::serve(listener, app).await.unwrap();
}

async fn health_check() -> &'static str {
    "OK"
}

#[derive(Deserialize)]
struct RunRequest {
    image: String, // e.g., "ubuntu:latest"
    // vcpu, mem, etc. optional
}

#[derive(Serialize)]
struct RunResponse {
    vm_id: String,
    status: String,
}

async fn run_vm(
    State(state): State<AppState>,
    Json(payload): Json<RunRequest>,
) -> Json<RunResponse> {
    info!("Received request to run image: {}", payload.image);
    
    // TODO: 
    // 1. Pull Image (OciManager)
    // 2. Unpack Layer (LayerManager)
    // 3. Create Cow (StorageManager)
    // 4. Setup Network (NetworkManager)
    // 5. Start Firecracker (VmmManager)
    
    // For now, mock response
    let vm_id = uuid::Uuid::new_v4().to_string();
    
    {
        let mut active = state.active_vm.lock().unwrap();
        *active = Some(vm_id.clone());
    }

    Json(RunResponse {
        vm_id,
        status: "Starting (Mock)".to_string(),
    })
}
