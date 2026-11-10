use anyhow::{anyhow, Context as AnyhowContext, Result};
use reqwest::header::{HeaderMap, HeaderValue, AUTHORIZATION};
use reqwest::Client;
use serde::{Deserialize, Serialize};
use std::path::{Path, PathBuf};

/// Named CLI connection context pointing at a local or remote `vyomad` daemon.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VyomaContext {
    pub name: String,
    pub host: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub api_token: Option<String>,
}

/// On-disk store of saved `vyoma` contexts (`contexts.json`).
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
pub struct ContextStore {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub current_context: Option<String>,
    #[serde(default)]
    pub contexts: Vec<VyomaContext>,
}

/// Resolved daemon transport target.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum TransportTarget {
    #[cfg(unix)]
    UnixSocket {
        socket_path: String,
        base_url: String,
    },
    Tcp {
        base_url: String,
    },
}

impl TransportTarget {
    pub fn base_url(&self) -> &str {
        match self {
            #[cfg(unix)]
            TransportTarget::UnixSocket { base_url, .. } => base_url,
            TransportTarget::Tcp { base_url } => base_url,
        }
    }
}

/// Resolved daemon endpoint and optional Bearer API token.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedConnection {
    pub target: TransportTarget,
    pub api_token: Option<String>,
    pub active_context: Option<String>,
}

/// Default location for `contexts.json` under the user's platform config dir.
pub fn default_contexts_path() -> PathBuf {
    dirs::config_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join("vyoma")
        .join("contexts.json")
}

pub fn load_store(path: &Path) -> Result<ContextStore> {
    if !path.exists() {
        return Ok(ContextStore::default());
    }
    let raw = std::fs::read_to_string(path)
        .with_context(|| format!("Failed to read context store at {}", path.display()))?;
    if raw.trim().is_empty() {
        return Ok(ContextStore::default());
    }
    serde_json::from_str(&raw)
        .with_context(|| format!("Failed to parse context store JSON at {}", path.display()))
}

pub fn save_store(path: &Path, store: &ContextStore) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "Failed to create config directory {}",
                parent.display()
            )
        })?;
    }
    let json = serde_json::to_string_pretty(store)?;
    std::fs::write(path, json)
        .with_context(|| format!("Failed to write context store at {}", path.display()))
}

pub fn create_context(
    path: &Path,
    name: String,
    host: String,
    api_token: Option<String>,
    activate: bool,
) -> Result<ContextStore> {
    if name.trim().is_empty() {
        return Err(anyhow!("Context name cannot be empty"));
    }
    if host.trim().is_empty() {
        return Err(anyhow!("Context host cannot be empty"));
    }

    let mut store = load_store(path)?;
    let entry = VyomaContext {
        name: name.clone(),
        host,
        api_token: api_token.filter(|t| !t.trim().is_empty()),
    };

    if let Some(existing) = store.contexts.iter_mut().find(|c| c.name == name) {
        *existing = entry;
    } else {
        store.contexts.push(entry);
    }

    if activate || store.current_context.is_none() {
        store.current_context = Some(name);
    }

    save_store(path, &store)?;
    Ok(store)
}

pub fn use_context(path: &Path, name: &str) -> Result<ContextStore> {
    let mut store = load_store(path)?;
    if !store.contexts.iter().any(|c| c.name == name) {
        return Err(anyhow!(
            "Context '{}' not found. Run `vyoma context ls` to inspect configured contexts.",
            name
        ));
    }
    store.current_context = Some(name.to_string());
    save_store(path, &store)?;
    Ok(store)
}

pub fn remove_context(path: &Path, name: &str) -> Result<ContextStore> {
    let mut store = load_store(path)?;
    let orig_len = store.contexts.len();
    store.contexts.retain(|c| c.name != name);
    if store.contexts.len() == orig_len {
        return Err(anyhow!("Context '{}' not found.", name));
    }
    if store.current_context.as_deref() == Some(name) {
        store.current_context = None;
    }
    save_store(path, &store)?;
    Ok(store)
}

/// Normalize a host specification into either a Unix socket target or TCP base URL.
pub fn parse_host_target(host: &str, http_port: u16) -> Result<TransportTarget> {
    let trimmed = host.trim();
    if let Some(socket) = trimmed.strip_prefix("unix://") {
        #[cfg(unix)]
        {
            return Ok(TransportTarget::UnixSocket {
                socket_path: socket.to_string(),
                base_url: format!("http://127.0.0.1:{}", http_port),
            });
        }
        #[cfg(not(unix))]
        {
            let _ = socket;
            return Err(anyhow!(
                "unix:// socket endpoints are only supported on Unix systems; use an http:// or https:// --host URL on this platform"
            ));
        }
    }

    let normalized = if trimmed.starts_with("http://") || trimmed.starts_with("https://") {
        trimmed.trim_end_matches('/').to_string()
    } else {
        format!("http://{}", trimmed.trim_end_matches('/'))
    };

    Ok(TransportTarget::Tcp {
        base_url: normalized,
    })
}

/// Resolve the final connection target and token using precedence:
/// CLI flags -> environment variables -> active saved context -> platform default.
pub fn resolve_connection(
    cli_host: Option<&str>,
    env_host: Option<&str>,
    cli_token: Option<&str>,
    env_token: Option<&str>,
    socket_path_arg: Option<&str>,
    default_socket_path: &str,
    http_port: u16,
    store: &ContextStore,
    resolve_local_socket: impl Fn(&str) -> String,
) -> Result<ResolvedConnection> {
    let active_ctx = store
        .current_context
        .as_deref()
        .and_then(|name| store.contexts.iter().find(|c| c.name == name));

    let explicit_host = cli_host
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .or_else(|| env_host.map(str::trim).filter(|s| !s.is_empty()));

    let (target, used_context_name) = if let Some(host) = explicit_host {
        (parse_host_target(host, http_port)?, None)
    } else if let Some(socket) = socket_path_arg.map(str::trim).filter(|s| !s.is_empty()) {
        #[cfg(unix)]
        {
            let resolved = resolve_local_socket(socket);
            (
                TransportTarget::UnixSocket {
                    socket_path: resolved,
                    base_url: format!("http://127.0.0.1:{}", http_port),
                },
                None,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (socket, &resolve_local_socket);
            return Err(anyhow!(
                "--socket-path is only supported on Unix hosts; pass --host http://<ip>:{} or configure a context",
                http_port
            ));
        }
    } else if let Some(ctx) = active_ctx {
        (
            parse_host_target(&ctx.host, http_port)?,
            Some(ctx.name.clone()),
        )
    } else {
        #[cfg(unix)]
        {
            let resolved = resolve_local_socket(default_socket_path);
            (
                TransportTarget::UnixSocket {
                    socket_path: resolved,
                    base_url: format!("http://127.0.0.1:{}", http_port),
                },
                None,
            )
        }
        #[cfg(not(unix))]
        {
            let _ = (default_socket_path, &resolve_local_socket);
            (
                TransportTarget::Tcp {
                    base_url: format!("http://127.0.0.1:{}", http_port),
                },
                None,
            )
        }
    };

    let api_token = cli_token
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
        .or_else(|| {
            env_token
                .map(str::trim)
                .filter(|s| !s.is_empty())
                .map(str::to_string)
        })
        .or_else(|| active_ctx.and_then(|c| c.api_token.clone()));

    Ok(ResolvedConnection {
        target,
        api_token,
        active_context: used_context_name,
    })
}

/// Construct a `reqwest::Client` configured for Unix socket or TCP and optional Bearer auth.
pub fn build_http_client(conn: &ResolvedConnection) -> Result<Client> {
    let mut builder = Client::builder();

    if let Some(ref token) = conn.api_token {
        let mut headers = HeaderMap::new();
        let value = HeaderValue::from_str(&format!("Bearer {}", token))
            .context("Invalid characters in API token")?;
        headers.insert(AUTHORIZATION, value);
        builder = builder.default_headers(headers);
    }

    match &conn.target {
        #[cfg(unix)]
        TransportTarget::UnixSocket { socket_path, .. } => {
            builder = builder.unix_socket(socket_path.as_str());
        }
        TransportTarget::Tcp { .. } => {}
    }

    builder.build().context("Failed to initialize HTTP client")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_context_crud_lifecycle() {
        let dir = tempfile::tempdir().unwrap();
        let store_path = dir.path().join("contexts.json");

        // Create first context -> automatically becomes active when none set
        let store = create_context(
            &store_path,
            "prod".to_string(),
            "https://vyoma.example.com:8443".to_string(),
            Some("secret-123".to_string()),
            false,
        )
        .unwrap();
        assert_eq!(store.current_context.as_deref(), Some("prod"));
        assert_eq!(store.contexts.len(), 1);

        // Create second context and switch to it
        let _ = create_context(
            &store_path,
            "staging".to_string(),
            "10.0.1.25:8080".to_string(),
            None,
            false,
        )
        .unwrap();
        let store = use_context(&store_path, "staging").unwrap();
        assert_eq!(store.current_context.as_deref(), Some("staging"));

        // Remove active context clears current_context
        let store = remove_context(&store_path, "staging").unwrap();
        assert!(store.current_context.is_none());
        assert_eq!(store.contexts.len(), 1);
    }

    #[test]
    fn test_resolve_connection_precedence_and_tokens() {
        let store = ContextStore {
            current_context: Some("remote".to_string()),
            contexts: vec![VyomaContext {
                name: "remote".to_string(),
                host: "192.168.10.5:8080".to_string(),
                api_token: Some("ctx-token".to_string()),
            }],
        };

        // 1. Saved context used when no --host / VYOMA_HOST / --socket-path supplied
        let conn = resolve_connection(
            None,
            None,
            None,
            None,
            None,
            "/run/vyoma/vyoma.sock",
            8080,
            &store,
            |s| s.to_string(),
        )
        .unwrap();
        assert_eq!(
            conn.target,
            TransportTarget::Tcp {
                base_url: "http://192.168.10.5:8080".to_string()
            }
        );
        assert_eq!(conn.api_token.as_deref(), Some("ctx-token"));
        assert_eq!(conn.active_context.as_deref(), Some("remote"));

        // 2. VYOMA_HOST env overrides saved context, --api-token overrides context token
        let conn2 = resolve_connection(
            None,
            Some("https://override.local:9090/"),
            Some("cli-token"),
            Some("env-token"),
            None,
            "/run/vyoma/vyoma.sock",
            8080,
            &store,
            |s| s.to_string(),
        )
        .unwrap();
        assert_eq!(
            conn2.target,
            TransportTarget::Tcp {
                base_url: "https://override.local:9090".to_string()
            }
        );
        assert_eq!(conn2.api_token.as_deref(), Some("cli-token"));
        assert!(conn2.active_context.is_none());

        // 3. --host wins over VYOMA_HOST
        let conn3 = resolve_connection(
            Some("http://127.0.0.1:8081"),
            Some("http://10.0.0.1:8080"),
            None,
            Some("env-token"),
            None,
            "/run/vyoma/vyoma.sock",
            8080,
            &store,
            |s| s.to_string(),
        )
        .unwrap();
        assert_eq!(
            conn3.target,
            TransportTarget::Tcp {
                base_url: "http://127.0.0.1:8081".to_string()
            }
        );
        assert_eq!(conn3.api_token.as_deref(), Some("env-token"));
    }

    #[test]
    fn test_build_http_client_with_tcp_and_token() {
        let conn = ResolvedConnection {
            target: TransportTarget::Tcp {
                base_url: "http://127.0.0.1:8080".to_string(),
            },
            api_token: Some("tok-abc".to_string()),
            active_context: None,
        };
        assert!(build_http_client(&conn).is_ok());
    }
}
