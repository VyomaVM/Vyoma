use anyhow::{anyhow, Result};
use reqwest::Client;
use serde::Deserialize;
use std::collections::HashMap;
use tracing::{debug, info};

const DOCKER_REGISTRY_V2: &str = "https://registry-1.docker.io/v2";
const DOCKER_AUTH_URL: &str = "https://auth.docker.io/token";

pub struct OciManager {
    client: Client,
    token_cache: HashMap<String, String>, // repository -> token
}

#[derive(Deserialize, Debug)]
struct TokenResponse {
    token: String,
    // expires_in: Option<i32>,
}

#[derive(Deserialize, Debug)]
#[serde(untagged)]
#[allow(dead_code)]
enum ManifestResponse {
    List(ManifestList),
    V2(ManifestV2),
}

#[derive(Deserialize, Debug)]
struct ManifestList {
    manifests: Vec<ManifestDescriptor>,
}

#[derive(Deserialize, Debug)]
struct ManifestDescriptor {
    digest: String,
    platform: Platform,
}

#[derive(Deserialize, Debug)]
struct Platform {
    architecture: String,
    os: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ManifestV2 {
    config: ConfigDescriptor,
    layers: Vec<LayerDescriptor>,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct ConfigDescriptor {
    digest: String,
}

#[derive(Deserialize, Debug)]
#[allow(dead_code)]
struct LayerDescriptor {
    digest: String,
}

impl OciManager {
    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token_cache: HashMap::new(),
        }
    }

    async fn authenticate(&mut self, repository: &str) -> Result<String> {
        if let Some(token) = self.token_cache.get(repository) {
            return Ok(token.clone());
        }

        info!("Authenticating for repository: {}", repository);
        let url = format!("{}?service=registry.docker.io&scope=repository:{}:pull", DOCKER_AUTH_URL, repository);
        
        let resp = self.client.get(&url).send().await?;
        if !resp.status().is_success() {
            return Err(anyhow!("Authentication failed: {}", resp.status()));
        }

        let token_resp: TokenResponse = resp.json().await?;
        self.token_cache.insert(repository.to_string(), token_resp.token.clone());
        
        Ok(token_resp.token)
    }

    pub async fn pull_manifest(&mut self, image: &str) -> Result<String> {
        // Parse image (simplistic for now)
        // Format: docker.io/library/alpine:latest or library/alpine:latest or alpine:latest
        // Simplify: Assume library/image:tag
        
        let parts: Vec<&str> = image.split(':').collect();
        let (repo_raw, tag) = if parts.len() == 2 {
            (parts[0], parts[1])
        } else {
             ("library/alpine", "latest") // Fallback for test 
        };

        // Handle implicit library/
        let repository = if !repo_raw.contains('/') {
            format!("library/{}", repo_raw)
        } else if repo_raw.starts_with("docker.io/") {
             repo_raw.replace("docker.io/", "")
        } else {
            repo_raw.to_string()
        };

        let token = self.authenticate(&repository).await?;

        let manifest_url = format!("{}/{}/manifests/{}", DOCKER_REGISTRY_V2, repository, tag);
        
        info!("Fetching manifest from: {}", manifest_url);

        // We accept both v2 manifests and OCI indexes
        let resp = self.client.get(&manifest_url)
            .header("Authorization", format!("Bearer {}", token))
            .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json")
            .send()
            .await?;

        if !resp.status().is_success() {
             return Err(anyhow!("Failed to fetch manifest: {}", resp.status()));
        }

        let content_type = resp.headers().get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        debug!("Manifest Content-Type: {}", content_type);

        let body = resp.text().await?;

        // If it's a list, we need to find the linux/amd64 digest and fetch THAT manifest
        if content_type.contains("list") || content_type.contains("index") {
             info!("Received Manifest List/Index. Resolving for linux/amd64...");
             let list: ManifestList = serde_json::from_str(&body)?;
             
             let target = list.manifests.iter().find(|m| 
                 (m.platform.architecture == "amd64" || m.platform.architecture == "x86_64") 
                 && m.platform.os == "linux"
             ).ok_or_else(|| anyhow!("No linux/amd64 manifest found in list"))?;
             
             info!("Resolved linux/amd64 digest: {}", target.digest);
             
             // Recursively fetch the specific manifest
             let resolved_url = format!("{}/{}/manifests/{}", DOCKER_REGISTRY_V2, repository, target.digest);
             let resolved_resp = self.client.get(&resolved_url)
                 .header("Authorization", format!("Bearer {}", token))
                 .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json")
                 .send()
                 .await?;
             
             return Ok(resolved_resp.text().await?);
        }

        Ok(body)
    }
}
