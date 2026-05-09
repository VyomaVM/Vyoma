use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageConfig {
    pub name: String,
    pub healthcheck_type: HealthcheckType,
    #[serde(default)]
    pub port: Option<u16>,
    #[serde(default)]
    pub healthcheck_cmd: Option<Vec<String>>,
    #[serde(default)]
    pub healthcheck_path: Option<String>,
    #[serde(default)]
    pub expected_status: Option<Vec<u16>>,
    #[serde(default)]
    pub expected_exit_code: Option<i32>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub timeout_secs: Option<u64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum HealthcheckType {
    Generic,
    Http,
    Tcp,
    Redis,
    Postgres,
    Exec,
}

impl Default for HealthcheckType {
    fn default() -> Self {
        Self::Generic
    }
}

impl ImageConfig {
    pub fn timeout(&self) -> Duration {
        Duration::from_secs(self.timeout_secs.unwrap_or(120))
    }

    pub fn check_port(&self) -> Option<u16> {
        self.port.or_else(|| {
            match self.healthcheck_type {
                HealthcheckType::Http => Some(80),
                HealthcheckType::Tcp => None,
                HealthcheckType::Redis => Some(6379),
                HealthcheckType::Postgres => Some(5432),
                _ => None,
            }
        })
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ImageList {
    pub images: Vec<ImageConfig>,
}

impl ImageList {
    pub fn load(path: &std::path::Path) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let list: ImageList = serde_json::from_str(&content)?;
        Ok(list)
    }

    pub fn load_from_text_file(path: &std::path::Path) -> anyhow::Result<Vec<String>> {
        let content = std::fs::read_to_string(path)?;
        let images: Vec<String> = content
            .lines()
            .filter(|line| !line.trim().is_empty() && !line.trim().starts_with('#'))
            .map(|line| line.trim().to_string())
            .collect();
        Ok(images)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TestResult {
    pub image: String,
    pub phase: TestPhase,
    pub success: bool,
    pub message: String,
    pub duration_ms: u64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<serde_json::Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum TestPhase {
    Pull,
    Build,
    Boot,
    Healthcheck,
    Teardown,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatReport {
    pub timestamp: String,
    pub total_images: usize,
    pub passed: usize,
    pub failed: usize,
    pub results: Vec<TestResult>,
    pub summary: CompatSummary,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CompatSummary {
    pub pull_success_rate: f64,
    pub build_success_rate: f64,
    pub boot_success_rate: f64,
    pub healthcheck_success_rate: f64,
    pub overall_success_rate: f64,
}

impl CompatReport {
    pub fn new(results: Vec<TestResult>) -> Self {
        let total = results.len();
        let passed = results.iter().filter(|r| r.success).count();
        let failed = total - passed;

        let pull_success = results.iter().filter(|r| r.phase == TestPhase::Pull && r.success).count();
        let build_success = results.iter().filter(|r| r.phase == TestPhase::Build && r.success).count();
        let boot_success = results.iter().filter(|r| r.phase == TestPhase::Boot && r.success).count();
        let health_success = results.iter().filter(|r| r.phase == TestPhase::Healthcheck && r.success).count();

        let pull_rate = if total > 0 { pull_success as f64 / total as f64 } else { 0.0 };
        let build_rate = if total > 0 { build_success as f64 / total as f64 } else { 0.0 };
        let boot_rate = if total > 0 { boot_success as f64 / total as f64 } else { 0.0 };
        let health_rate = if total > 0 { health_success as f64 / total as f64 } else { 0.0 };
        let overall = if total > 0 { passed as f64 / total as f64 } else { 0.0 };

        Self {
            timestamp: chrono::Utc::now().to_rfc3339(),
            total_images: total,
            passed,
            failed,
            results,
            summary: CompatSummary {
                pull_success_rate: pull_rate,
                build_success_rate: build_rate,
                boot_success_rate: boot_rate,
                healthcheck_success_rate: health_rate,
                overall_success_rate: overall,
            },
        }
    }

    pub fn to_json(&self) -> anyhow::Result<String> {
        Ok(serde_json::to_string_pretty(self)?)
    }
}
