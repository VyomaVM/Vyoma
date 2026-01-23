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

#[derive(Deserialize, Debug, Clone)]
pub struct ManifestList {
    pub manifests: Vec<ManifestDescriptor>,
}

#[derive(Deserialize, Debug, Clone)]
pub struct ManifestDescriptor {
    pub digest: String,
    pub platform: Platform,
}

#[derive(Deserialize, Debug, Clone)]
pub struct Platform {
    pub architecture: String,
    pub os: String,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct ManifestV2 {
    pub config: ConfigDescriptor,
    pub layers: Vec<LayerDescriptor>,
}

#[derive(Deserialize, Debug, Clone)]
#[allow(dead_code)]
pub struct ConfigDescriptor {
    pub digest: String,
}

#[derive(Deserialize, Debug, Clone)]
pub struct LayerDescriptor {
    pub digest: String,
}

#[derive(Deserialize, Debug)]
struct DockerConfig {
    auths: HashMap<String, DockerAuth>,
}

#[derive(Deserialize, Debug)]
struct DockerAuth {
    auth: String,
}

impl OciManager {
    fn load_docker_creds(&self, registry: &str) -> Option<(String, String)> {
        // 1. Find config file
        let home = dirs::home_dir()?;
        let config_path = home.join(".docker").join("config.json");
        if !config_path.exists() {
            return None;
        }

        // 2. Parse
        let content = std::fs::read_to_string(config_path).ok()?;
        let config: DockerConfig = serde_json::from_str(&content).ok()?;

        // 3. Match Registry
        // Docker Hub often uses "https://index.docker.io/v1/" in config, but we might be talking to registry-1.
        let keys_to_try = vec![
            registry.to_string(),
            format!("https://{}", registry),
            "https://index.docker.io/v1/".to_string(), // Legacy Docker Hub
        ];

        use base64::prelude::*;
        for key in keys_to_try {
            if let Some(auth_entry) = config.auths.get(&key) {
                // Decode base64
                if let Ok(decoded_bytes) = BASE64_STANDARD.decode(&auth_entry.auth) {
                    if let Ok(decoded_str) = String::from_utf8(decoded_bytes) {
                        // Format is user:pass
                        if let Some((u, p)) = decoded_str.split_once(':') {
                            return Some((u.to_string(), p.to_string()));
                        }
                    }
                }
            }
        }
        None
    }

    pub fn new() -> Self {
        Self {
            client: Client::new(),
            token_cache: HashMap::new(),
        }
    }

    async fn fetch_token(&self, realm: &str, service: Option<&str>, scope: Option<&str>, registry_host: &str) -> Result<String> {
        let mut url = realm.to_string();
        let mut query = vec![];
        if let Some(s) = service { query.push(format!("service={}", s)); }
        if let Some(s) = scope { query.push(format!("scope={}", s)); }
        if !query.is_empty() {
             url = format!("{}?{}", url, query.join("&"));
        }

        let mut req = self.client.get(&url);
        
        // Credentials?
        if let Some((user, pass)) = self.load_docker_creds(registry_host) {
             info!("Using authenticated access for {}", registry_host);
             req = req.basic_auth(user, Some(pass));
        }

        let resp = req.send().await?;
        if !resp.status().is_success() {
             return Err(anyhow!("Token request failed for {}: {}", registry_host, resp.status()));
        }
        let token_resp: TokenResponse = resp.json().await?;
        Ok(token_resp.token)
    }

    pub async fn pull_manifest(&mut self, image: &str) -> Result<String> {
        let (registry, repository, tag) = self.parse_image_ref(image);
        let proto = if registry.starts_with("localhost") { "http" } else { "https" };
        let manifest_url = format!("{}://{}/v2/{}/manifests/{}", proto, registry, repository, tag);
        
        info!("Fetching manifest from: {}", manifest_url);

        let cache_key = format!("{}/{}", registry, repository);
        let token = self.token_cache.get(&cache_key).cloned();

        let mut req = self.client.get(&manifest_url)
            .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json");
        
        if let Some(t) = &token {
             req = req.header("Authorization", format!("Bearer {}", t));
        }

        let resp = req.send().await?;

        let resp = if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
             debug!("401 Unauthorized. Attempting auth negotiation...");
             let auth_header = resp.headers().get("www-authenticate")
                 .ok_or_else(|| anyhow!("401 Unauthorized but missing Www-Authenticate header"))?
                 .to_str()?;
             
             let realm = extract_field(auth_header, "realm").ok_or_else(|| anyhow!("Missing realm"))?;
             let service = extract_field(auth_header, "service");
             let scope = extract_field(auth_header, "scope");
             
             let new_token = self.fetch_token(&realm, service.as_deref(), scope.as_deref(), &registry).await?;
             self.token_cache.insert(cache_key.clone(), new_token.clone());
             
             // Retry
             self.client.get(&manifest_url)
                 .header("Authorization", format!("Bearer {}", new_token))
                 .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json, application/vnd.oci.image.index.v1+json, application/vnd.docker.distribution.manifest.list.v2+json")
                 .send().await?
        } else {
             resp
        };

        if !resp.status().is_success() {
             return Err(anyhow!("Failed to fetch manifest: {}", resp.status()));
        }

        let content_type = resp.headers().get("content-type")
            .and_then(|h| h.to_str().ok())
            .unwrap_or("unknown")
            .to_string();

        let body = resp.text().await?;

        if content_type.contains("list") || content_type.contains("index") {
             info!("Received Manifest List/Index. Resolving for linux/amd64...");
             let list: ManifestList = serde_json::from_str(&body)?;
             
             let target = list.manifests.iter().find(|m| 
                 (m.platform.architecture == "amd64" || m.platform.architecture == "x86_64") 
                 && m.platform.os == "linux"
             ).ok_or_else(|| anyhow!("No linux/amd64 manifest found in list"))?;
             
             info!("Resolved linux/amd64 digest: {}", target.digest);
             
             let resolved_url = format!("{}://{}/v2/{}/manifests/{}", proto, registry, repository, target.digest);
             
             let mut req = self.client.get(&resolved_url)
                  .header("Accept", "application/vnd.docker.distribution.manifest.v2+json, application/vnd.oci.image.manifest.v1+json");
             
             // Use token from cache (we just updated it if needed)
             if let Some(t) = self.token_cache.get(&cache_key) {
                  req = req.header("Authorization", format!("Bearer {}", t));
             }

             let resolved_resp = req.send().await?;
             // Note: recursive 401 handling omitted here for brevity, usually the token covers the repo.
             
             return Ok(resolved_resp.text().await?);
         }

        Ok(body)
    }

    pub async fn pull_layer(&mut self, image: &str, digest: &str) -> Result<Vec<u8>> {
        let (registry, repository, _) = self.parse_image_ref(image);
        let proto = if registry.contains("localhost") { "http" } else { "https" };
        let layer_url = format!("{}://{}/v2/{}/blobs/{}", proto, registry, repository, digest);
        
        info!("Fetching layer blob: {}", digest);

        let cache_key = format!("{}/{}", registry, repository);
        let token = self.token_cache.get(&cache_key).cloned();

        let mut req = self.client.get(&layer_url);
        if let Some(t) = &token {
             req = req.header("Authorization", format!("Bearer {}", t));
        }

        let resp = req.send().await?;

        let resp = if resp.status() == reqwest::StatusCode::UNAUTHORIZED {
             debug!("Layer 401. Refreshing token...");
             let auth_header = resp.headers().get("www-authenticate")
                 .ok_or_else(|| anyhow!("401 but no Www-Authenticate"))?
                 .to_str()?;
             
             let realm = extract_field(auth_header, "realm").ok_or_else(|| anyhow!("Missing realm"))?;
             let service = extract_field(auth_header, "service");
             let scope = extract_field(auth_header, "scope");
             
             let new_token = self.fetch_token(&realm, service.as_deref(), scope.as_deref(), &registry).await?;
             self.token_cache.insert(cache_key, new_token.clone());
             
             self.client.get(&layer_url)
                 .header("Authorization", format!("Bearer {}", new_token))
                 .send().await?
        } else {
             resp
        };
        
        if !resp.status().is_success() {
             return Err(anyhow!("Failed to fetch layer {}: {}", digest, resp.status()));
        }

        let bytes = resp.bytes().await?;
        Ok(bytes.to_vec())
    }

    fn parse_image_ref(&self, image: &str) -> (String, String, String) {
        // 1. Split Tag
        let (rest, tag) = if let Some((r, t)) = image.rsplit_once(':') {
             (r, t)
        } else {
             (image, "latest")
        };

        // 2. Split Registry
        // Heuristic: First component has "." or ":" or is "localhost".
        let (registry, repository) = if let Some((reg, repo)) = rest.split_once('/') {
            if reg.contains('.') || (reg.contains(':') && !reg.contains("docker.io")) || reg == "localhost" {
                (reg, repo)
            } else {
                ("registry-1.docker.io", rest)
            }
        } else {
            ("registry-1.docker.io", rest)
        };

        // 3. Handle Hub "library/" expansion
        let final_repo = if registry == "registry-1.docker.io" && !repository.contains('/') {
             format!("library/{}", repository)
        } else {
             repository.to_string()
        };

        // 4. Handle "docker.io" alias
        let final_reg = if registry == "docker.io" { "registry-1.docker.io" } else { registry };

        (final_reg.to_string(), final_repo, tag.to_string())
    }

    pub fn parse_layers(&self, manifest_json: &str) -> Result<Vec<String>> {
        let v2: ManifestV2 = serde_json::from_str(manifest_json)
            .map_err(|e| anyhow!("Failed to parse Manifest V2: {}", e))?;
        
        Ok(v2.layers.iter().map(|l| l.digest.clone()).collect())
    }
}

fn extract_field(header: &str, key: &str) -> Option<String> {
    let key_eq = format!("{}=", key);
    header.split(',')
        .find(|p| p.trim().starts_with(&key_eq))
        .map(|p| p.trim().split('"').nth(1).unwrap_or("").to_string())
}
