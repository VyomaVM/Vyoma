use anyhow::{anyhow, Context as AnyhowContext, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::{Client, StatusCode};
use serde::{Deserialize, Serialize};
use std::net::TcpStream;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use crate::context;

/// Persisted SSH tunnel entry linking a local forwarded port to a remote Linux KVM `vyomad`.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct TunnelSpec {
    pub name: String,
    pub ssh_target: String,
    pub ssh_port: u16,
    pub local_port: u16,
    pub remote_port: u16,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub identity: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pid: Option<u32>,
    pub context_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct TunnelStore {
    #[serde(default)]
    pub tunnels: Vec<TunnelSpec>,
}

/// Verification result against a remote `vyomad` HTTP(S) endpoint.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemoteProbeResult {
    pub normalized_url: String,
    pub reachable: bool,
    pub authenticated: bool,
    pub detail: String,
}

pub fn default_tunnels_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vyoma")
        .join("tunnels.json")
}

pub fn load_tunnels(path: &Path) -> Result<TunnelStore> {
    if !path.exists() {
        return Ok(TunnelStore::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read tunnel store at {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(TunnelStore::default());
    }
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse tunnel store JSON at {}", path.display()))
}

pub fn save_tunnels(path: &Path, store: &TunnelStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    std::fs::write(path, serde_json::to_string_pretty(store)?)?;
    Ok(())
}

pub fn normalize_http_url(host: &str) -> String {
    let trimmed = host.trim().trim_end_matches('/');
    if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.to_string()
    } else {
        format!("http://{}", trimmed)
    }
}

/// Build cross-platform OpenSSH CLI arguments (`ssh` on macOS/Linux, built-in `ssh.exe` on Windows 10/11).
pub fn build_ssh_tunnel_args(
    ssh_target: &str,
    ssh_port: u16,
    local_port: u16,
    remote_port: u16,
    identity: Option<&str>,
) -> Result<Vec<String>> {
    if ssh_target.trim().is_empty() {
        return Err(anyhow!("SSH target (e.g. user@kvm-host) cannot be empty"));
    }

    let mut args = vec![
        "-N".to_string(),
        "-L".to_string(),
        format!("127.0.0.1:{}:127.0.0.1:{}", local_port, remote_port),
        "-p".to_string(),
        ssh_port.to_string(),
        "-o".to_string(),
        "ExitOnForwardFailure=yes".to_string(),
        "-o".to_string(),
        "ServerAliveInterval=30".to_string(),
        "-o".to_string(),
        "ServerAliveCountMax=3".to_string(),
    ];

    if let Some(key) = identity.map(str::trim).filter(|s| !s.is_empty()) {
        args.push("-i".to_string());
        args.push(key.to_string());
    }

    args.push(ssh_target.trim().to_string());
    Ok(args)
}

/// Probe `/health` and `/ps` against a remote `vyomad` instance to verify connectivity and Bearer auth.
pub async fn verify_remote_endpoint(
    host: &str,
    api_token: Option<&str>,
) -> Result<RemoteProbeResult> {
    let normalized = normalize_http_url(host);
    let mut builder = Client::builder().timeout(Duration::from_secs(5));

    if let Some(token) = api_token.map(str::trim).filter(|s| !s.is_empty()) {
        let mut headers = HeaderMap::new();
        headers.insert(
            AUTHORIZATION,
            HeaderValue::from_str(&format!("Bearer {}", token))
                .context("Invalid characters in API token")?,
        );
        builder = builder.default_headers(headers);
    }

    let client = builder.build()?;

    let health_resp = match client.get(format!("{}/health", normalized)).send().await {
        Ok(r) => r,
        Err(e) => {
            return Ok(RemoteProbeResult {
                normalized_url: normalized,
                reachable: false,
                authenticated: false,
                detail: format!(
                    "Unreachable ({}). Verify vyomad is running with `--http-bind-ip 0.0.0.0` or use `vyoma remote tunnel`.",
                    e
                ),
            });
        }
    };

    if !health_resp.status().is_success() {
        return Ok(RemoteProbeResult {
            normalized_url: normalized,
            reachable: false,
            authenticated: false,
            detail: format!("/health returned unexpected HTTP {}", health_resp.status()),
        });
    }

    // Now test `/ps` (which sits behind vyomad's Bearer token middleware when --api-token is set)
    let ps_resp = client.get(format!("{}/ps", normalized)).send().await;
    match ps_resp {
        Ok(r) if r.status().is_success() => Ok(RemoteProbeResult {
            normalized_url: normalized,
            reachable: true,
            authenticated: true,
            detail: "Connected and authenticated".to_string(),
        }),
        Ok(r) if r.status() == StatusCode::UNAUTHORIZED => Ok(RemoteProbeResult {
            normalized_url: normalized,
            reachable: true,
            authenticated: false,
            detail: "Unauthorized (HTTP 401): provide matching `--api-token` or VYOMA_API_TOKEN"
                .to_string(),
        }),
        Ok(r) => Ok(RemoteProbeResult {
            normalized_url: normalized,
            reachable: true,
            authenticated: false,
            detail: format!("Authenticated check returned HTTP {}", r.status()),
        }),
        Err(e) => Ok(RemoteProbeResult {
            normalized_url: normalized,
            reachable: true,
            authenticated: false,
            detail: format!("Auth probe failed: {}", e),
        }),
    }
}

pub async fn connect_remote(
    name: String,
    host: String,
    api_token: Option<String>,
    verify: bool,
    activate: bool,
) -> Result<()> {
    let normalized = normalize_http_url(&host);

    if verify {
        let probe = verify_remote_endpoint(&normalized, api_token.as_deref()).await?;
        if !probe.reachable {
            return Err(anyhow!(
                "Cannot reach remote daemon at {}: {}",
                probe.normalized_url,
                probe.detail
            ));
        }
        if !probe.authenticated {
            return Err(anyhow!(
                "Remote daemon at {} rejected credentials: {}",
                probe.normalized_url,
                probe.detail
            ));
        }
    }

    let ctx_path = context::default_contexts_path();
    let store = context::create_context(
        &ctx_path,
        name.clone(),
        normalized.clone(),
        api_token,
        activate,
    )?;

    println!(
        "Remote context '{}' saved ({}){}",
        name,
        normalized,
        if store.current_context.as_deref() == Some(name.as_str()) {
            " [active]"
        } else {
            ""
        }
    );
    Ok(())
}

pub fn start_ssh_tunnel(
    name: String,
    ssh_target: String,
    ssh_port: u16,
    local_port: u16,
    remote_port: u16,
    identity: Option<String>,
    api_token: Option<String>,
    background: bool,
) -> Result<()> {
    let args = build_ssh_tunnel_args(
        &ssh_target,
        ssh_port,
        local_port,
        remote_port,
        identity.as_deref(),
    )?;

    let ctx_url = format!("http://127.0.0.1:{}", local_port);
    let ctx_path = context::default_contexts_path();
    context::create_context(&ctx_path, name.clone(), ctx_url.clone(), api_token, true)?;

    let spec = TunnelSpec {
        name: name.clone(),
        ssh_target: ssh_target.clone(),
        ssh_port,
        local_port,
        remote_port,
        identity,
        pid: None,
        context_name: name.clone(),
    };

    if background {
        let mut child = Command::new("ssh")
            .args(&args)
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::piped())
            .spawn()
            .context("Failed to launch `ssh` (ensure OpenSSH client is installed)")?;

        let pid = child.id();

        // Wait briefly for the local forwarded port to accept TCP connections
        let addr = format!("127.0.0.1:{}", local_port);
        let mut ready = false;
        for _ in 0..50 {
            if let Ok(Some(status)) = child.try_wait() {
                return Err(anyhow!(
                    "SSH tunnel exited prematurely with status {} (check SSH keys/host/port forwarding)",
                    status
                ));
            }
            if TcpStream::connect_timeout(&addr.parse()?, Duration::from_millis(100)).is_ok() {
                ready = true;
                break;
            }
            std::thread::sleep(Duration::from_millis(100));
        }

        let mut store = load_tunnels(&default_tunnels_path())?;
        store.tunnels.retain(|t| t.name != name);
        let mut stored_spec = spec;
        stored_spec.pid = Some(pid);
        store.tunnels.push(stored_spec);
        save_tunnels(&default_tunnels_path(), &store)?;

        println!(
            "SSH tunnel '{}' running in background (PID: {}, {} -> {}:{}) [context '{}' active]",
            name,
            pid,
            ctx_url,
            ssh_target,
            remote_port,
            name
        );
        if !ready {
            println!("Note: tunnel process started; port {} is still establishing.", local_port);
        }
        println!("Run `vyoma ps` now from Windows/macOS, or `vyoma remote disconnect {}` when done.", name);
        // Detach child handle so background SSH stays alive after vyoma CLI exits
        std::mem::forget(child);
    } else {
        println!(
            "Opening SSH tunnel '{}' ({} -> {}:{}) [context '{}' active]. Press Ctrl+C to close.",
            name, ctx_url, ssh_target, remote_port, name
        );
        let status = Command::new("ssh")
            .args(&args)
            .status()
            .context("Failed to execute `ssh`")?;
        if !status.success() {
            return Err(anyhow!("SSH tunnel exited with status {}", status));
        }
    }

    Ok(())
}

pub fn disconnect_tunnel(name: &str) -> Result<()> {
    let path = default_tunnels_path();
    let mut store = load_tunnels(&path)?;
    let idx = store
        .tunnels
        .iter()
        .position(|t| t.name == name)
        .ok_or_else(|| anyhow!("No tracked tunnel named '{}'", name))?;

    let spec = store.tunnels.remove(idx);
    if let Some(pid) = spec.pid {
        #[cfg(target_os = "windows")]
        {
            let _ = Command::new("taskkill")
                .args(["/PID", &pid.to_string(), "/F"])
                .status();
        }
        #[cfg(not(target_os = "windows"))]
        {
            let _ = Command::new("kill")
                .args(["-TERM", &pid.to_string()])
                .status();
        }
        println!("Stopped SSH tunnel '{}' (PID: {}).", name, pid);
    }

    save_tunnels(&path, &store)?;
    Ok(())
}

pub async fn list_remote_status(probe: bool) -> Result<()> {
    let ctx_store = context::load_store(&context::default_contexts_path())?;
    let tunnel_store = load_tunnels(&default_tunnels_path()).unwrap_or_default();

    println!("{}", "Configured Contexts:".bold_header("Configured Contexts"));
    if ctx_store.contexts.is_empty() {
        println!("  (none)");
    } else {
        for ctx in &ctx_store.contexts {
            let active = if ctx_store.current_context.as_deref() == Some(ctx.name.as_str()) {
                "*"
            } else {
                " "
            };
            if probe && !ctx.host.starts_with("unix://") {
                let res = verify_remote_endpoint(&ctx.host, ctx.api_token.as_deref()).await?;
                println!(
                    "  {} {:<16} {:<30} [{}]",
                    active, ctx.name, ctx.host, res.detail
                );
            } else {
                println!("  {} {:<16} {}", active, ctx.name, ctx.host);
            }
        }
    }

    println!("\nTracked SSH Tunnels:");
    if tunnel_store.tunnels.is_empty() {
        println!("  (none)");
    } else {
        for t in &tunnel_store.tunnels {
            println!(
                "  {:<16} 127.0.0.1:{} -> {}:{} (ssh={}, pid={:?})",
                t.name, t.local_port, t.ssh_target, t.remote_port, t.ssh_port, t.pid
            );
        }
    }
    Ok(())
}

trait HeaderExt {
    fn bold_header(&self, label: &str) -> String;
}

impl HeaderExt for str {
    fn bold_header(&self, label: &str) -> String {
        format!("{}:", label)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tokio::io::{AsyncReadExt, AsyncWriteExt};
    use tokio::net::TcpListener;

    #[test]
    fn test_build_ssh_tunnel_args() {
        let args = build_ssh_tunnel_args("ubuntu@192.168.1.50", 2222, 18080, 8080, Some("~/.ssh/id_ed25519"))
            .unwrap();
        assert!(args.windows(2).any(|w| w == ["-L", "127.0.0.1:18080:127.0.0.1:8080"]));
        assert!(args.windows(2).any(|w| w == ["-p", "2222"]));
        assert!(args.windows(2).any(|w| w == ["-i", "~/.ssh/id_ed25519"]));
        assert_eq!(args.last().unwrap(), "ubuntu@192.168.1.50");
        assert!(build_ssh_tunnel_args("", 22, 18080, 8080, None).is_err());
    }

    #[test]
    fn test_tunnel_store_roundtrip() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("tunnels.json");
        let store = TunnelStore {
            tunnels: vec![TunnelSpec {
                name: "gpu-kvm".to_string(),
                ssh_target: "dev@10.20.0.5".to_string(),
                ssh_port: 22,
                local_port: 18080,
                remote_port: 8080,
                identity: None,
                pid: Some(4242),
                context_name: "gpu-kvm".to_string(),
            }],
        };
        save_tunnels(&path, &store).unwrap();
        let loaded = load_tunnels(&path).unwrap();
        assert_eq!(loaded.tunnels, store.tunnels);
    }

    #[tokio::test]
    async fn test_verify_remote_endpoint_auth_and_health() {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let addr = listener.local_addr().unwrap();

        let server = tokio::spawn(async move {
            // Expect 4 requests: (/health + /ps unauth) then (/health + /ps auth)
            for _ in 0..4 {
                if let Ok((mut sock, _)) = listener.accept().await {
                    let mut buf = [0u8; 2048];
                    let n = sock.read(&mut buf).await.unwrap_or(0);
                    let req = String::from_utf8_lossy(&buf[..n]);
                    let resp = if req.starts_with("GET /health ") {
                        "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nConnection: close\r\n\r\nok"
                    } else if req.contains("Authorization: Bearer secret-tok")
                        || req.contains("authorization: Bearer secret-tok")
                    {
                        "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: 10\r\nConnection: close\r\n\r\n{\"vms\":[]}"
                    } else {
                        "HTTP/1.1 401 Unauthorized\r\nContent-Length: 12\r\nConnection: close\r\n\r\nunauthorized"
                    };
                    let _ = sock.write_all(resp.as_bytes()).await;
                }
            }
        });

        let url = format!("http://{}", addr);

        // Missing token triggers clear Unauthorized diagnosis
        let r1 = verify_remote_endpoint(&url, None).await.unwrap();
        assert!(r1.reachable);
        assert!(!r1.authenticated);
        assert!(r1.detail.contains("401"));

        // Matching token succeeds
        let r2 = verify_remote_endpoint(&url, Some("secret-tok")).await.unwrap();
        assert!(r2.reachable);
        assert!(r2.authenticated);

        let _ = server.await;
    }
}
