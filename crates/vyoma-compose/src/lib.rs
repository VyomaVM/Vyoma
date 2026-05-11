pub mod schema_v3;

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

pub use schema_v3::{ComposeV3, ServiceV3, NetworkV3, VolumeV3, PortEntry, VolumeEntry};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VyomaCompose {
    pub version: String,
    pub services: HashMap<String, Service>,

    #[serde(default)]
    pub networks: HashMap<String, NetworkConfig>,

    #[serde(default)]
    pub volumes: HashMap<String, VolumeConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum BuildSource {
    Path(String),
    Config(BuildConfig),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BuildConfig {
    pub context: String,
    pub vyomafile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkConfig {
    pub driver: Option<String>,
    pub external: Option<bool>,
    #[serde(default)]
    pub ipam: IpamConfig,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct IpamConfig {
    #[serde(default)]
    pub config: Vec<IpamSubnet>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IpamSubnet {
    pub subnet: Option<String>,
    pub gateway: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeConfig {
    pub driver: Option<String>,
    pub external: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<BuildSource>,
    pub cpus: Option<u32>,
    pub memory: Option<u32>,
    pub ports: Option<Vec<String>>,
    pub volumes: Option<Vec<String>>,
    pub environment: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub depends_on: Option<Vec<String>>,
    #[serde(default)]
    pub networks: Vec<String>,
}

impl VyomaCompose {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        Self::from_str(&content)
    }

    pub fn from_str(content: &str) -> Result<Self> {
        let compose: VyomaCompose = serde_yaml::from_str(content)?;

        if !Self::is_supported_version(&compose.version) {
            return Err(anyhow::anyhow!(
                "Unsupported compose version: {}. Supported: 1.0, 3.x (3.0-3.9)",
                compose.version
            ));
        }

        Ok(compose)
    }

    fn is_supported_version(version: &str) -> bool {
        version == "1" || version == "1.0" || version.starts_with("3.")
    }

    pub fn start_order(&self) -> Result<Vec<(String, Service)>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        let mut keys: Vec<_> = self.services.keys().collect();
        keys.sort();

        for name in keys {
            self.visit(name, &mut visited, &mut visiting, &mut order)?;
        }

        Ok(order)
    }

    fn visit(
        &self,
        name: &String,
        visited: &mut HashSet<String>,
        visiting: &mut HashSet<String>,
        order: &mut Vec<(String, Service)>,
    ) -> Result<()> {
        if visited.contains(name) {
            return Ok(());
        }
        if visiting.contains(name) {
            return Err(anyhow::anyhow!(
                "Circular dependency detected involving {}",
                name
            ));
        }

        visiting.insert(name.clone());

        if let Some(service) = self.services.get(name) {
            if let Some(deps) = &service.depends_on {
                for dep in deps {
                    if !self.services.contains_key(dep) {
                        return Err(anyhow::anyhow!(
                            "Service '{}' depends on undefined service '{}'",
                            name,
                            dep
                        ));
                    }
                    self.visit(dep, visited, visiting, order)?;
                }
            }
            visiting.remove(name);
            visited.insert(name.clone());
            order.push((name.clone(), service.clone()));
        }

        Ok(())
    }

    pub fn get_network_names(&self) -> Vec<String> {
        self.networks.keys().cloned().collect()
    }

    pub fn get_volume_names(&self) -> Vec<String> {
        self.volumes.keys().cloned().collect()
    }

    pub fn is_network_external(&self, name: &str) -> bool {
        self.networks
            .get(name)
            .and_then(|n| n.external.as_ref())
            .copied()
            .unwrap_or(false)
    }

    pub fn is_volume_external(&self, name: &str) -> bool {
        self.volumes
            .get(name)
            .and_then(|v| v.external.as_ref())
            .copied()
            .unwrap_or(false)
    }

    pub fn to_v3(&self) -> Result<ComposeV3> {
        let mut services = HashMap::new();
        
        for (name, svc) in &self.services {
            let mut service_v3 = ServiceV3 {
                image: svc.image.clone(),
                ..Default::default()
            };

            if let Some(ports) = &svc.ports {
                service_v3.ports = ports.iter()
                    .map(|p| PortEntry::Short(p.clone()))
                    .collect();
            }

            if let Some(volumes) = &svc.volumes {
                service_v3.volumes = volumes.iter()
                    .map(|v| VolumeEntry::Short(v.clone()))
                    .collect();
            }

            if let Some(env) = &svc.environment {
                service_v3.environment = env.clone();
            }

            if let Some(deps) = &svc.depends_on {
                for dep in deps {
                    service_v3.depends_on.insert(dep.clone(), schema_v3::DependsOnCondition {
                        condition: "service_started".to_string(),
                    });
                }
            }

            if let Some(cpus) = svc.cpus {
                service_v3.deploy = Some(schema_v3::DeployConfig {
                    resources: Some(schema_v3::ResourceConfig {
                        limits: Some(schema_v3::ResourceConstraints {
                            cpus: Some(format!("{}", cpus as f32)),
                            memory: None,
                        }),
                        reservations: None,
                    }),
                });
            }

            if let Some(mem) = svc.memory {
                if let Some(ref mut deploy) = service_v3.deploy {
                    if let Some(ref mut resources) = deploy.resources {
                        if let Some(ref mut limits) = resources.limits {
                            limits.memory = Some(format!("{}M", mem));
                        }
                    } else {
                        deploy.resources = Some(schema_v3::ResourceConfig {
                            limits: Some(schema_v3::ResourceConstraints {
                                cpus: None,
                                memory: Some(format!("{}M", mem)),
                            }),
                            reservations: None,
                        });
                    }
                } else {
                    service_v3.deploy = Some(schema_v3::DeployConfig {
                        resources: Some(schema_v3::ResourceConfig {
                            limits: Some(schema_v3::ResourceConstraints {
                                cpus: None,
                                memory: Some(format!("{}M", mem)),
                            }),
                            reservations: None,
                        }),
                    });
                }
            }

            service_v3.networks = svc.networks.clone();

            services.insert(name.clone(), service_v3);
        }

        let mut networks = HashMap::new();
        for (name, net) in &self.networks {
            networks.insert(name.clone(), NetworkV3 {
                driver: net.driver.clone(),
                external: net.external,
            });
        }

        let mut volumes = HashMap::new();
        for (name, vol) in &self.volumes {
            volumes.insert(name.clone(), VolumeV3 {
                driver: vol.driver.clone(),
                external: vol.external,
            });
        }

        Ok(ComposeV3 {
            version: Some(self.version.clone()),
            services,
            networks,
            volumes,
        })
    }
}

impl ServiceV3 {
    fn new() -> Self {
        Self {
            image: None,
            ports: Vec::new(),
            volumes: Vec::new(),
            environment: HashMap::new(),
            depends_on: HashMap::new(),
            deploy: None,
            networks: Vec::new(),
            command: None,
        }
    }
}

impl Default for ServiceV3 {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_compose() {
        let yaml = r#"
version: "1.0"
services:
  web:
    image: nginx:latest
    ports:
      - "8080:80"
  db:
    image: postgres:13
    memory: 512
"#;
        let compose = VyomaCompose::from_str(yaml).unwrap();
        assert_eq!(compose.version, "1.0");
        assert_eq!(compose.services.len(), 2);

        let web = compose.services.get("web").unwrap();
        assert_eq!(web.image.as_ref().unwrap(), "nginx:latest");
        assert_eq!(web.ports.as_ref().unwrap()[0], "8080:80");

        let db = compose.services.get("db").unwrap();
        assert_eq!(db.memory, Some(512));
    }

    #[test]
    fn test_parse_compose_v3() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx:latest
    networks:
      - frontend
  api:
    image: node:18
    networks:
      - frontend
      - backend
networks:
  frontend:
    driver: bridge
    ipam:
      config:
        - subnet: 172.20.0.0/16
  backend:
    driver: bridge
volumes:
  db-data:
    driver: local
"#;
        let compose = VyomaCompose::from_str(yaml).unwrap();
        assert!(compose.version.starts_with("3"));
        assert_eq!(compose.networks.len(), 2);
        assert!(compose.networks.contains_key("frontend"));
        assert!(compose.networks.contains_key("backend"));
        assert_eq!(compose.volumes.len(), 1);

        let web = compose.services.get("web").unwrap();
        assert_eq!(web.networks, vec!["frontend"]);

        let api = compose.services.get("api").unwrap();
        assert_eq!(api.networks, vec!["frontend", "backend"]);
    }

    #[test]
    fn test_parse_build_compose() {
        let yaml = r#"
version: "1.0"
services:
  app:
    build: ./app
    ports:
      - "3000:3000"
  worker:
    build:
      context: ./worker
      vyomafile: CustomVyomafile
"#;
        let compose = VyomaCompose::from_str(yaml).unwrap();
        let app = compose.services.get("app").unwrap();
        match app.build.as_ref().unwrap() {
            BuildSource::Path(p) => assert_eq!(p, "./app"),
            _ => panic!("Expected BuildSource::Path"),
        }

        let worker = compose.services.get("worker").unwrap();
        match worker.build.as_ref().unwrap() {
            BuildSource::Config(c) => {
                assert_eq!(c.context, "./worker");
                assert_eq!(c.vyomafile.as_ref().unwrap(), "CustomVyomafile");
            }
            _ => panic!("Expected BuildSource::Config"),
        }
    }

    #[test]
    fn test_external_network() {
        let yaml = r#"
version: "3.8"
services:
  app:
    image: nginx
    networks:
      - ext-network
networks:
  ext-network:
    external: true
"#;
        let compose = VyomaCompose::from_str(yaml).unwrap();
        assert!(compose.is_network_external("ext-network"));
    }
}
