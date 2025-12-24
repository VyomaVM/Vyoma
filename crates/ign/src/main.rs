use clap::{Parser, Subcommand};
use tracing::{info, error};
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};

#[derive(Parser)]
#[command(name = "ign")]
#[command(about = "Ignite: Docker for Micro-VMs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Run a new VM
    Run {
        /// Image to run (e.g. ubuntu:latest)
        image: String,
    },
    /// Stop a VM
    Stop {
        /// VM ID
        id: String,
    },
    /// Pause a VM
    Pause {
         /// VM ID
        id: String,
    },
    /// Resume a VM
    Resume {
         /// VM ID
        id: String,
    },
    /// List active VMs (Not implemented yet)
    Ps,
}

#[derive(Serialize)]
struct RunRequest {
    image: String,
}

#[derive(Deserialize, Debug)]
struct RunResponse {
    vm_id: String,
    status: String,
}

#[tokio::main]
async fn main() -> Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let client = Client::new();
    let daemon_url = "http://127.0.0.1:3000";

    match cli.command {
        Commands::Run { image } => {
            info!("Requesting to run image: {}", image);
            let payload = RunRequest { image };
            
            let resp = client.post(format!("{}/run", daemon_url))
                .json(&payload)
                .send()
                .await;

            handle_response(resp, daemon_url).await?;
        }
        Commands::Stop { id } => {
            info!("Requesting to stop VM: {}", id);
            let resp = client.post(format!("{}/stop/{}", daemon_url, id))
                .send()
                .await;
             handle_simple_response(resp, daemon_url).await?;
        }
        Commands::Pause { id } => {
             info!("Requesting to pause VM: {}", id);
            let resp = client.post(format!("{}/pause/{}", daemon_url, id))
                .send()
                .await;
             handle_simple_response(resp, daemon_url).await?;
        }
        Commands::Resume { id } => {
             info!("Requesting to resume VM: {}", id);
            let resp = client.post(format!("{}/resume/{}", daemon_url, id))
                .send()
                .await;
             handle_simple_response(resp, daemon_url).await?;
        }
        Commands::Ps => {
            let resp = client.get(format!("{}/ps", daemon_url))
                .send()
                .await;
            
            match resp {
                Ok(response) => {
                     if response.status().is_success() {
                         let body: ListResponse = response.json().await?;
                         println!("{:<40} {:<20} {:<10}", "VM ID", "IP ADDRESS", "STATUS");
                         for vm in body.vms {
                             println!("{:<40} {:<20} {:<10}", vm.id, vm.ip_address, "Running");
                         }
                     } else {
                         error!("Daemon returned error: {}", response.status());
                     }
                }
                Err(e) => {
                    error!("Failed to connect to daemon at {}: {}", daemon_url, e);
                }
            }
        }
    }

    Ok(())
}

#[derive(Deserialize, Debug)]
struct VmSummary {
    id: String,
    ip_address: String,
}

#[derive(Deserialize, Debug)]
struct ListResponse {
    vms: Vec<VmSummary>,
}

async fn handle_response(resp: Result<reqwest::Response, reqwest::Error>, url: &str) -> Result<()> {
     match resp {
        Ok(response) => {
             if response.status().is_success() {
                 let body: RunResponse = response.json().await?;
                 info!("Success! VM ID: {}, Status: {}", body.vm_id, body.status);
             } else {
                 let status = response.status();
                 let text = response.text().await.unwrap_or_default();
                 error!("Daemon returned error: {} - {}", status, text);
             }
        }
        Err(e) => {
            error!("Failed to connect to daemon at {}: {}", url, e);
            info!("Is 'ignited' running?");
        }
    }
    Ok(())
}

async fn handle_simple_response(resp: Result<reqwest::Response, reqwest::Error>, url: &str) -> Result<()> {
     match resp {
        Ok(response) => {
             if response.status().is_success() {
                 let text = response.text().await?;
                 info!("Success: {}", text);
             } else {
                 let status = response.status();
                 let text = response.text().await.unwrap_or_default();
                 error!("Daemon returned error: {} - {}", status, text);
             }
        }
        Err(e) => {
            error!("Failed to connect to daemon at {}: {}", url, e);
            info!("Is 'ignited' running?");
        }
    }
    Ok(())
}
