use axum::{
    routing::{get, post},
    Router,
    Json,
    extract::{State, Path},
    http::StatusCode,
};
use serde::{Deserialize, Serialize};
use std::sync::{Arc, Mutex as StdMutex};
use std::collections::HashMap;
use tracing::{info, error};
use tokio::net::TcpListener;
use tokio::sync::Mutex as TokioMutex;
use ignite_core::vmm::VmmManager;

#[derive(Clone)]
struct AppState {
    // Map<VmId, Arc<TokioMutex<VmmManager>>>
    // Outer Mutex (Std) creates synchronization for the Map.
    // Inner Mutex (Tokio) creates synchronization for the VmmManager (and allows async locking).
    vms: Arc<StdMutex<HashMap<String, Arc<TokioMutex<VmmManager>>>>>,
}

#[tokio::main]
async fn main() {
    tracing_subscriber::fmt::init();

    info!("ignited (Ignite Daemon) starting up...");

    let state = AppState {
        vms: Arc::new(StdMutex::new(HashMap::new())),
    };

    // build our application with a route
    let app = Router::new()
        // Health check
        .route("/health", get(health_check))
        // VM Management
        .route("/run", post(run_vm))
        .route("/stop/:id", post(stop_vm))
        .route("/pause/:id", post(pause_vm))
        .route("/resume/:id", post(resume_vm))
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
    image: String,
}

#[derive(Serialize)]
struct RunResponse {
    vm_id: String,
    status: String,
}

async fn run_vm(
    State(state): State<AppState>,
    Json(payload): Json<RunRequest>,
) -> Result<Json<RunResponse>, (StatusCode, String)> {
    info!("Received request to run image: {}", payload.image);
    
    // Generate ID
    let vm_id = uuid::Uuid::new_v4().to_string();
    let socket_path = format!("/tmp/firecracker_{}.socket", vm_id);
    
    // Create VmmManager
    let mut vmm = VmmManager::new(&socket_path);
    
    let binary = "bin/firecracker";
    
    // Start Daemon
    if let Err(e) = vmm.start_daemon(binary) {
        error!("Failed to start firecracker: {}", e);
        return Err((StatusCode::INTERNAL_SERVER_ERROR, format!("Failed to start VMM: {}", e)));
    }

    // Store in state
    {
        let mut vms = state.vms.lock().unwrap();
        vms.insert(vm_id.clone(), Arc::new(TokioMutex::new(vmm)));
    }

    Ok(Json(RunResponse {
        vm_id,
        status: "Running".to_string(),
    }))
}

async fn stop_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to stop VM: {}", id);
    
    let vmm_arc = {
        let mut vms = state.vms.lock().unwrap();
        vms.remove(&id)
    };

    if let Some(vmm_mutex) = vmm_arc {
        let mut vmm = vmm_mutex.lock().await;
        vmm.kill().map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} stopped", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

async fn pause_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to pause VM: {}", id);
    
    let vmm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };
    
    if let Some(vmm_mutex) = vmm_arc {
        let vmm = vmm_mutex.lock().await;
        vmm.pause_instance().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} paused", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}

async fn resume_vm(
    State(state): State<AppState>,
    Path(id): Path<String>,
) -> Result<String, (StatusCode, String)> {
    info!("Request to resume VM: {}", id);
    
    let vmm_arc = {
        let vms = state.vms.lock().unwrap();
        vms.get(&id).cloned()
    };
    
    if let Some(vmm_mutex) = vmm_arc {
        let vmm = vmm_mutex.lock().await;
        vmm.resume_instance().await.map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(format!("VM {} resumed", id))
    } else {
        Err((StatusCode::NOT_FOUND, "VM not found".to_string()))
    }
}
