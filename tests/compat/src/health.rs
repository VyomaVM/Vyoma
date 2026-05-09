use crate::types::{HealthcheckType, ImageConfig};
use anyhow::{Context, Result};
use std::net::TcpStream;
use std::time::Duration;
use tokio::io::{AsyncReadExt, AsyncWriteExt};
use tokio::net::TcpStream as TokioTcp;
use tracing::{debug, info, warn};

pub struct Healthchecker {
    host_port: u16,
    timeout: Duration,
}

impl Healthchecker {
    pub fn new(host_port: u16, timeout: Duration) -> Self {
        Self { host_port, timeout }
    }

    pub async fn check(&self, config: &ImageConfig) -> Result<HealthResult> {
        match config.healthcheck_type {
            HealthcheckType::Tcp => self.check_tcp().await,
            HealthcheckType::Http => {
                let path = config.healthcheck_path.as_deref().unwrap_or("/");
                let expected = config.expected_status.clone().unwrap_or(vec![200, 204]);
                self.check_http(path, &expected).await
            }
            HealthcheckType::Redis => self.check_redis().await,
            HealthcheckType::Postgres => self.check_postgres(config).await,
            HealthcheckType::Exec => {
                Ok(HealthResult::skipped("Exec healthcheck requires agent access"))
            }
            HealthcheckType::Generic => {
                Ok(HealthResult::skipped("Generic healthcheck requires agent access"))
            }
        }
    }

    async fn check_tcp(&self) -> Result<HealthResult> {
        let start = std::time::Instant::now();
        let addr = format!("127.0.0.1:{}", self.host_port);

        match tokio::time::timeout(self.timeout, async {
            TokioTcp::connect(&addr).await
        })
        .await
        {
            Ok(Ok(_stream)) => {
                let duration = start.elapsed().as_millis() as u64;
                Ok(HealthResult::healthy(duration, format!("TCP connected to port {}", self.host_port)))
            }
            Ok(Err(e)) => Ok(HealthResult::unhealthy(
                start.elapsed().as_millis() as u64,
                format!("Connection failed: {}", e),
            )),
            Err(_) => Ok(HealthResult::timeout(self.timeout.as_millis() as u64)),
        }
    }

    async fn check_http(&self, path: &str, expected_status: &[u16]) -> Result<HealthResult> {
        let start = std::time::Instant::now();
        let url = format!("http://127.0.0.1:{}{}", self.host_port, path);

        let client = reqwest::Client::builder()
            .timeout(self.timeout)
            .danger_accept_invalid_certs(true)
            .build()
            .context("Failed to create HTTP client")?;

        match client.get(&url).send().await {
            Ok(response) => {
                let duration = start.elapsed().as_millis() as u64;
                let status = response.status().as_u16();

                if expected_status.contains(&status) {
                    Ok(HealthResult::healthy(duration, format!("HTTP {} from {}", status, url)))
                } else {
                    Ok(HealthResult::unhealthy(
                        duration,
                        format!("Unexpected HTTP status {} (expected {:?}", status, expected_status)),
                    ))
                }
            }
            Err(e) => {
                let duration = start.elapsed().as_millis() as u64;
                if e.is_timeout() {
                    Ok(HealthResult::timeout(duration))
                } else {
                    Ok(HealthResult::unhealthy(duration, format!("HTTP request failed: {}", e)))
                }
            }
        }
    }

    async fn check_redis(&self) -> Result<HealthResult> {
        let start = std::time::Instant::now();
        let addr = format!("127.0.0.1:{}", self.host_port);

        match tokio::time::timeout(self.timeout, async {
            let mut stream = TokioTcp::connect(&addr).await?;
            stream.write_all(b"PING\r\n").await?;
            let mut buf = [0u8; 128];
            stream.read_exact(&mut buf[..7]).await?;
            Ok(buf[..7].to_vec())
        })
        .await
        {
            Ok(Ok(ref response)) if response == b"+PONG\r\n" => {
                let duration = start.elapsed().as_millis() as u64;
                Ok(HealthResult::healthy(duration, "Redis PONG received".to_string()))
            }
            Ok(Ok(response)) => Ok(HealthResult::unhealthy(
                start.elapsed().as_millis() as u64,
                format!("Unexpected Redis response: {:?}", String::from_utf8_lossy(response)),
            )),
            Ok(Err(e)) => Ok(HealthResult::unhealthy(
                start.elapsed().as_millis() as u64,
                format!("Redis connection failed: {}", e),
            )),
            Err(_) => Ok(HealthResult::timeout(self.timeout.as_millis() as u64)),
        }
    }

    async fn check_postgres(&self, config: &ImageConfig) -> Result<HealthResult> {
        let cmd = config
            .healthcheck_cmd
            .as_ref()
            .map(|c| c.join(" "))
            .unwrap_or_else(|| "pg_isready -U postgres".to_string());

        let start = std::time::Instant::now();
        let output = tokio::process::Command::new("sh")
            .arg("-c")
            .arg(&format!(
                "nc -z 127.0.0.1 {} && {} || true",
                self.host_port, cmd
            ))
            .output()
            .await
            .context("Failed to run postgres healthcheck")?;

        let duration = start.elapsed().as_millis() as u64;
        let stdout = String::from_utf8_lossy(&output.stdout);

        if output.status.success() || stdout.contains("accepting connections") {
            Ok(HealthResult::healthy(duration, format!("PostgreSQL ready: {}", stdout.trim())))
        } else {
            Ok(HealthResult::unhealthy(duration, format!("PostgreSQL not ready: {}", stdout.trim())))
        }
    }
}

#[derive(Debug, Clone)]
pub struct HealthResult {
    pub healthy: bool,
    pub skipped: bool,
    pub duration_ms: u64,
    pub message: String,
}

impl HealthResult {
    pub fn healthy(duration_ms: u64, message: String) -> Self {
        Self {
            healthy: true,
            skipped: false,
            duration_ms,
            message,
        }
    }

    pub fn unhealthy(duration_ms: u64, message: String) -> Self {
        Self {
            healthy: false,
            skipped: false,
            duration_ms,
            message,
        }
    }

    pub fn timeout(duration_ms: u64) -> Self {
        Self {
            healthy: false,
            skipped: false,
            duration_ms,
            message: format!("Healthcheck timed out after {}ms", duration_ms),
        }
    }

    pub fn skipped(message: impl Into<String>) -> Self {
        Self {
            healthy: true,
            skipped: true,
            duration_ms: 0,
            message: message.into(),
        }
    }
}
