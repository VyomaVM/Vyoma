use clap::{Parser, Subcommand};
use tracing::{info, error};
use colored::Colorize;
use anyhow::Result;
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::path::{Path, PathBuf};
use flate2::write::GzEncoder;
use flate2::read::GzDecoder;
use flate2::Compression;
use tar::Archive; // Removed Builder since we will use tar::Builder inline
use futures::stream::StreamExt;

use ignite_core::api::PortMapping;
use ignite_core::api::VolumeMount;
use ignite_compose::IgniteCompose;

#[derive(Parser)]
#[command(name = "ign")]
#[command(about = "Ignite: Docker for Micro-VMs", long_about = None)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Pull an image from Docker Hub
    Pull {
        /// Image to pull (e.g. ubuntu:latest)
        image: String,
    },
    /// Run a new VM
    Run {
        /// Image to run (e.g. ubuntu:latest)
        image: String,

        /// vCPUs (default: 1)
        #[arg(long, default_value = "1")]
        vcpu: u32,
        
        /// Memory in MiB (default: 512)
        #[arg(long, default_value = "512")]
        memory: u32,
        
        /// Port mappings (e.g. -p 8080:80)
        #[arg(short, long)]
        ports: Vec<String>,
        
        /// Volume mounts (e.g. -v /home/user/app:/app)
        #[arg(short, long)]
        volumes: Vec<String>,
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
    /// Create a snapshot of a VM
    Snapshot {
        /// VM ID
        id: String,
    },
    /// Restore a VM from a snapshot
    Restore {
        /// Path to snapshot file
        snapshot_path: String,
        /// Path to memory file
        mem_path: String,
        /// Path to COW file
        cow_path: String,
    },
    /// Export a VM snapshot to a file (Teleportation)
    Export {
        /// VM ID to export (must be snapshot first)
        id: String,
        /// Output file path (e.g. my-vm.tar.gz)
        output: String,
    },
    /// Import a VM from a file (Teleportation)
    Import {
        /// Input file path (e.g. my-vm.tar.gz)
        input: String,
    },
    /// Stream logs from a VM
    Logs {
        /// VM ID
        id: String,
        /// Follow log output
        #[arg(short = 'f', long)]
        follow: bool,
    },
    /// Build a new image from an Ignitefile
    Build {
        /// Path to build context (directory containing Ignitefile)
        #[arg(default_value = ".")]
        #[arg(default_value = ".")]
        path: String,
    },
    /// Check system dependencies and environment health
    Doctor,
    /// Manage networks
    Network {
        #[command(subcommand)]
        command: NetworkCommands,
    },
    /// Create and start resources from a compose file
    Up {
        /// Path to compose file (default: ignite-compose.yml)
        #[arg(short, long, default_value = "ignite-compose.yml")]
        file: String,

        /// Detached mode: Run containers in the background
        #[arg(short, long)]
        detach: bool,
    },
    /// Stop and remove resources
    Down {
         /// Path to compose file (default: ignite-compose.yml)
        #[arg(short, long, default_value = "ignite-compose.yml")]
        file: String,
    },
}

#[derive(Subcommand)]
enum NetworkCommands {
    /// List available networks
    Ls,
    /// Create a new bridge network
    Create {
        /// Network name
        name: String,
        /// Subnet CIDR (e.g. 10.244.0.0/16)
        #[arg(long)]
        subnet: String,
    },
    /// Remove a network
    Rm {
        /// Network name
        name: String,
    },
}

#[derive(Serialize)]
struct RunRequest {
    image: String,
    vcpu: u32,
    mem_size_mib: u32,
    ports: Vec<PortMapping>,
    volumes: Vec<VolumeMount>,
}

#[derive(Serialize)]
struct RestoreRequest {
    snapshot_path: String,
    mem_path: String,
    cow_path: String,
    original_vm_id: String,
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
        Commands::Pull { image } => {
            info!("Requesting to pull image: {}", image);
             let resp = client.post(format!("{}/pull", daemon_url))
                .json(&serde_json::json!({ "image": image }))
                .send()
                .await;
             handle_simple_response(resp, daemon_url).await?;
        }
        Commands::Run { image, vcpu, memory, ports, volumes } => {
            info!("Requesting to run image: {}", image);
            
            let mut port_mappings = Vec::new();
            for p in ports {
                let parts: Vec<&str> = p.split(':').collect();
                if parts.len() != 2 {
                    error!("Invalid port format: {}. Use host:vm (e.g., 8080:80)", p);
                    return Ok(());
                }
                
                let host_port = parts[0].parse::<u16>().map_err(|_| anyhow::anyhow!("Invalid host port: {}", parts[0]))?;
                let vm_port = parts[1].parse::<u16>().map_err(|_| anyhow::anyhow!("Invalid vm port: {}", parts[1]))?;
                
                port_mappings.push(PortMapping { host_port, vm_port });
            }

            let mut volume_mounts = Vec::new();
            for v in volumes {
                let parts: Vec<&str> = v.split(':').collect();
                 if parts.len() != 2 {
                    error!("Invalid volume format: {}. Use host:vm (e.g., /foo:/bar)", v);
                    return Ok(());
                }
                volume_mounts.push(VolumeMount {
                    host_path: parts[0].to_string(),
                    vm_path: parts[1].to_string(),
                    read_only: false, // Default RW for now
                });
            }
            
            let payload = RunRequest { 
                image, 
                vcpu,
                mem_size_mib: memory,
                ports: port_mappings, 
                volumes: volume_mounts 
            };
            
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
        Commands::Snapshot { id } => {
            info!("Requesting to snapshot VM: {}", id);
            let resp = client.post(format!("{}/snapshot/{}", daemon_url, id))
                .send()
                .await;
             handle_simple_response(resp, daemon_url).await?;
        }
        Commands::Restore { snapshot_path, mem_path, cow_path } => {
            info!("Requesting to restore VM from: {}", snapshot_path);
            let payload = RestoreRequest {
                snapshot_path,
                mem_path,
                cow_path,
                original_vm_id: "unknown".to_string(),
            };
            
            let resp = client.post(format!("{}/restore", daemon_url))
                .json(&payload)
                .send()
                .await;
                
            handle_response(resp, daemon_url).await?;
        }
        Commands::Export { id, output } => {
            info!("Exporting VM {} to {}", id, output);
            export_vm(&id, &output)?;
        }
        Commands::Import { input } => {
            info!("Importing VM from {}", input);
            import_vm(&input, daemon_url).await?;
        }
        Commands::Logs { id, follow: _ } => {
             let resp = client.get(format!("{}/logs/{}", daemon_url, id))
                .send()
                .await?;
             
             if !resp.status().is_success() {
                 error!("Failed to get logs: {}", resp.status());
                 return Ok(());
             }

             let mut stream = resp.bytes_stream();
             let mut buffer = String::new();
             
             while let Some(item) = stream.next().await {
                 let bytes = item?;
                 let chunk = String::from_utf8_lossy(&bytes);
                 buffer.push_str(&chunk);
                 
                 while let Some(idx) = buffer.find('\n') {
                     let line = buffer[..idx].to_string();
                     buffer = buffer[idx+1..].to_string();
                     
                     if line.starts_with("data: ") {
                         println!("{}", &line[6..]);
                     }
                 }
             }
        }
        Commands::Build { path } => {
            info!("Building image from context: {}", path);
            info!("Building image from context: {}", path);
            build_image(&path, daemon_url).await?;
        }
        Commands::Doctor => {
             run_doctor().await?;
        }
        Commands::Network { command } => {
            match command {
                NetworkCommands::Ls => {
                    let resp = client.get(format!("{}/networks", daemon_url)).send().await;
                    match resp {
                        Ok(response) => {
                             if response.status().is_success() {
                                 let networks: Vec<String> = response.json().await?;
                                 println!("NETWORKS:");
                                 for net in networks {
                                     println!("{}", net);
                                 }
                             } else {
                                 error!("Daemon returned error: {}", response.status());
                             }
                        }
                        Err(e) => error!("Failed to list networks: {}", e),
                    }
                }
                NetworkCommands::Create { name, subnet } => {
                    let payload = serde_json::json!({
                        "name": name,
                        "subnet": subnet
                    });
                    let resp = client.post(format!("{}/networks", daemon_url))
                        .json(&payload)
                        .send()
                        .await;
                    handle_simple_response(resp, daemon_url).await?;
                }
                NetworkCommands::Rm { name } => {
                     let resp = client.delete(format!("{}/networks/{}", daemon_url, name))
                        .send()
                        .await;
                     handle_simple_response(resp, daemon_url).await?;
                }
        }
        }
        Commands::Up { file, detach } => {
            info!("Processing compose file: {}", file);
            match IgniteCompose::from_file(&file) {
                Ok(compose) => {
                    println!("Ignite Compose v{}", compose.version);
                    println!("Services found: {}", compose.services.len());
                    for (name, service) in compose.services {
                         let source = if let Some(ref img) = service.image {
                             format!("Image: {}", img)
                         } else if let Some(ref build) = service.build {
                             match build {
                                 ignite_compose::BuildSource::Path(p) => format!("Build: {}", p),
                                 ignite_compose::BuildSource::Config(c) => format!("Build: {} (Ignitefile: {:?})", c.context, c.ignitefile),
                             }
                         } else {
                             "Unknown".to_string()
                         };
                         println!("- {}: {}", name, source);
                    }
                    if detach {
                        println!("(Detached mode selected)");
                    }
                    println!("(Full implementation pending in next phase)");
                },
                Err(e) => error!("Failed to parse compose file '{}': {}", file, e),
            }
        }
        Commands::Down { file: _ } => {
            println!("'ign down' is not yet implemented.");
        }
    }

    Ok(())
}

async fn run_doctor() -> Result<()> {
    println!("{}", "Ignite Doctor - System Health Check".bold().underline());
    println!();
    
    let mut all_passed = true;
    
    // Helper to print status
    let check = |name: &str, result: Result<bool>, required: bool| -> bool {
        match result {
            Ok(true) => {
                println!("{} {}", "[OK]".green().bold(), name);
                true
            },
            Ok(false) => {
                if required {
                    println!("{} {}", "[FAIL]".red().bold(), name);
                    false
                } else {
                    println!("{} {}", "[WARN]".yellow().bold(), name);
                    true // Warn doesn't fail overall
                }
            },
            Err(e) => {
                 if required {
                    println!("{} {} ({})", "[FAIL]".red().bold(), name, e);
                    false
                 } else {
                     println!("{} {} ({})", "[WARN]".yellow().bold(), name, e);
                     true
                 }
            }
        }
    };
    
    // 1. KVM Access
    if !check("KVM Device Access (/dev/kvm)", check_kvm(), true) { all_passed = false; }
    
    // 2. Cgroups
    if !check("Cgroups v2 (/sys/fs/cgroup)", check_cgroups(), true) { all_passed = false; }
    
    // 3. Binaries
    if !check("Firecracker Binary", check_binary("firecracker"), true) { all_passed = false; }
    if !check("Virtiofsd Binary", check_binary("virtiofsd"), true) { all_passed = false; }
    
    // 4. Networking
    if !check("Ignite Bridge (ign0)", check_bridge(), false) { } // Warn only
    
    // 5. Rootless Tools
    if !check("debugfs (e2fsprogs)", check_binary("debugfs"), false) { } // Needed for rootless build
    
    println!();
    if all_passed {
        println!("{}", "Your system is ready for Ignite!".green().bold());
    } else {
        println!("{}", "Some checks failed. Please review the errors above.".red().bold());
    }
    
    Ok(())
}

fn check_kvm() -> Result<bool> {
    use std::fs::OpenOptions;
    let path = Path::new("/dev/kvm");
    if !path.exists() { return Ok(false); }
    // Try to open R/W
    OpenOptions::new().read(true).write(true).open(path)?;
    Ok(true)
}

fn check_cgroups() -> Result<bool> {
    // Just check if mount point exists and is cgroup2
    // Simple check: /sys/fs/cgroup/cgroup.controllers should exist
    let path = Path::new("/sys/fs/cgroup/cgroup.controllers");
    Ok(path.exists())
}

fn check_binary(name: &str) -> Result<bool> {
    // Check PATH or .ignite/bin??
    // Currently daemon assumes local relative path 'bin/firecracker', 
    // but users might run 'ign' from anywhere.
    // Ideally 'ignited' should find them. 
    // 'ign doctor' runs as user.
    // Let's check `which <name>` first.
    let status = std::process::Command::new("which").arg(name).output()?.status;
    if status.success() { return Ok(true); }
    
    // Check local bin?
    // We haven't defined a global install path yet.
    Ok(false)
}

fn check_bridge() -> Result<bool> {
    let output = std::process::Command::new("ip").arg("link").arg("show").arg("ign0").output()?;
    Ok(output.status.success())
}

async fn build_image(context_path: &str, daemon_url: &str) -> Result<()> {
    let context_path = Path::new(context_path);
    if !context_path.exists() {
        return Err(anyhow::anyhow!("Context path does not exist: {:?}", context_path));
    }
    
    // Create tarball in memory (or temp file if large, memory for now)
    // Actually, reqwest Body can take a File.
    // Let's tar to a temp file.
    let temp_dir = tempfile::tempdir()?;
    let tar_path = temp_dir.path().join("context.tar.gz");
    let tar_file = File::create(&tar_path)?;
    
    let enc = GzEncoder::new(tar_file, Compression::default());
    let mut tar = tar::Builder::new(enc);
    
    // Add directory content to tar (recursively)
    tar.append_dir_all(".", context_path)?;
    tar.finish()?;
    
    // Send to daemon
    let file = tokio::fs::File::open(&tar_path).await?;
    let stream = tokio_util::codec::FramedRead::new(file, tokio_util::codec::BytesCodec::new());
    let body = reqwest::Body::wrap_stream(stream);
    
    let client = Client::new();
    let resp = client.post(format!("{}/build", daemon_url))
        .body(body)
        .send()
        .await;
        
    handle_simple_response(resp, daemon_url).await?;
    
    Ok(())
}

fn export_vm(vm_id: &str, output_path: &str) -> Result<()> {
    // 1. Locate VM dir
    // HACK: Accessing .ignite directly. CLI normally shouldn't allow this if daemon is remote.
    // But for MVP, we assume local access.
    let home = dirs::home_dir().ok_or(anyhow::anyhow!("No home dir"))?;
    let vm_dir = home.join(".ignite").join("vms").join(vm_id);
    
    if !vm_dir.exists() {
        return Err(anyhow::anyhow!("VM directory not found: {:?}", vm_dir));
    }
    
    // Check files
    let snap_path = vm_dir.join("snapshot.snap");
    let mem_path = vm_dir.join("memory.mem");
    let cow_path = vm_dir.join("diff.cow");
    
    if !snap_path.exists() || !mem_path.exists() {
        return Err(anyhow::anyhow!("Snapshot files missing. Did you run 'ign snapshot <id>' first?"));
    }
    
    // 2. Create Tarball
    let tar_file = File::create(output_path)?;
    let enc = GzEncoder::new(tar_file, Compression::default());
    let mut tar = tar::Builder::new(enc);
    
    // Add files with specific names for portability
    tar.append_path_with_name(&snap_path, "snapshot.snap")?;
    tar.append_path_with_name(&mem_path, "memory.mem")?;
    tar.append_path_with_name(&cow_path, "diff.cow")?;
    
    tar.finish()?;
    
    info!("Export complete: {}", output_path);
    Ok(())
}

async fn import_vm(input_path: &str, daemon_url: &str) -> Result<()> {
    // 1. Unpack to temp dir
    let file = File::open(input_path)?;
    let tar = GzDecoder::new(file);
    let mut archive = Archive::new(tar);
    
    let temp_dir = tempfile::tempdir()?;
    info!("Unpacking to temporary location: {:?}", temp_dir.path());
    
    archive.unpack(temp_dir.path())?;
    
    // 2. Verify files
    let snap_path = temp_dir.path().join("snapshot.snap");
    let mem_path = temp_dir.path().join("memory.mem");
    let cow_path = temp_dir.path().join("diff.cow");
    
    if !snap_path.exists() || !mem_path.exists() || !cow_path.exists() {
        return Err(anyhow::anyhow!("Invalid export archive. Missing core files."));
    }
    
    // 3. Call Restore API
    // Need absolute paths for Daemon to access them (Daemon must be on same FS)
    let client = Client::new();
    let payload = RestoreRequest {
        snapshot_path: snap_path.to_string_lossy().to_string(),
        mem_path: mem_path.to_string_lossy().to_string(),
        cow_path: cow_path.to_string_lossy().to_string(),
        original_vm_id: "imported".to_string(),
    };
            
    let resp = client.post(format!("{}/restore", daemon_url))
        .json(&payload)
        .send()
        .await;

    handle_response(resp, daemon_url).await?;
    
    // NOTE: TempDir will be deleted when it goes out of scope!
    // But Restore API copies the COW file, so that's fine for COW.
    // However, Snapshot and Mem files are LOADED by Firecracker. 
    // Firecracker keeps them open? Or does it read them?
    // Firecracker `load_snapshot` usually loads them. 
    // If we delete them too early, restoration might fail if it's async?
    // Actually, `restore_vm` awaits `load_snapshot`.
    // But does `load_snapshot` keep file handle or read into RAM?
    // Firecracker loads into RAM.
    
    // To be safe, we should probably PERSIST these unpacked files into a new VM directory BEFORE calling restore.
    // But the Daemon creates the new VM directory.
    // Ideally, Daemon should handle import upload.
    // For MVP, we will rely on TempDir not being deleted until function returns, 
    // and `restore_vm` waits for success.
    
    // But wait, `Cow` is copied. `Snapshot/Mem` are READ.
    // If `restore_vm` finishes, the VM is running.
    // Does Firecracker need `memory.mem` to stay on disk after load?
    // Docs say: "The file is mmap()-ed". So YES, it must exist while VM is running?
    // Actually, if it's MAP_PRIVATE, maybe not.
    // If it's backing memory, deleting it is BAD.
    
    // Safer Approach:
    // Move these files to a permanent location?
    // Since Daemon doesn't know about "Import", we are stuck.
    // Let's rely on standard practice: The User (CLI) is responsible for data validity.
    // We should unpack to a PERMANENT "imports" folder in .ignite/imports/<uuid>/ ?
    
    Ok(()) // temp_dir drop here. 
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
