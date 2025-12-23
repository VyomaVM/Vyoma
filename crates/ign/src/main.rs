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

            match resp {
                Ok(response) => {
                     if response.status().is_success() {
                         let body: RunResponse = response.json().await?;
                         info!("Success! VM ID: {}, Status: {}", body.vm_id, body.status);
                     } else {
                         error!("Daemon returned error: {}", response.status());
                     }
                }
                Err(e) => {
                    error!("Failed to connect to daemon at {}: {}", daemon_url, e);
                    info!("Is 'ignited' running?");
                }
            }
        }
        Commands::Ps => {
            info!("ps command not yet implemented on daemon.");
        }
    }

    Ok(())
}
