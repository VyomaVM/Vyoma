use anyhow::{Result, anyhow};
use serde::{Deserialize, Serialize, Deserializer, de::Error};
use std::collections::{HashMap, HashSet};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ComposeV3 {
    pub version: Option<String>,
    pub services: HashMap<String, ServiceV3>,
    #[serde(default)]
    pub networks: HashMap<String, NetworkV3>,
    #[serde(default)]
    pub volumes: HashMap<String, VolumeV3>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServiceV3 {
    pub image: Option<String>,
    #[serde(default)]
    pub ports: Vec<PortEntry>,
    #[serde(default)]
    pub volumes: Vec<VolumeEntry>,
    #[serde(default, deserialize_with = "deserialize_env")]
    pub environment: HashMap<String, String>,
    #[serde(default, deserialize_with = "deserialize_depends_on")]
    pub depends_on: HashMap<String, DependsOnCondition>,
    pub deploy: Option<DeployConfig>,
    #[serde(default)]
    pub networks: Vec<String>,
    pub command: Option<CommandEntry>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum CommandEntry {
    String(String),
    List(Vec<String>),
}

impl CommandEntry {
    pub fn to_vec(&self) -> Vec<String> {
        match self {
            Self::String(s) => s.split_whitespace().map(|s| s.to_string()).collect(),
            Self::List(l) => l.clone(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum PortEntry {
    Short(String), // e.g. "8080:80"
    Long(PortLong),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PortLong {
    pub target: u16,
    pub published: Option<u16>,
    pub protocol: Option<String>,
    pub mode: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(untagged)]
pub enum VolumeEntry {
    Short(String), // e.g. "/host:/container:ro"
    Long(VolumeLong),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeLong {
    pub r#type: String, // "bind", "volume", "tmpfs"
    pub source: Option<String>,
    pub target: String,
    pub read_only: Option<bool>,
}

fn deserialize_env<'de, D>(deserializer: D) -> Result<HashMap<String, String>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum EnvFmt {
        List(Vec<String>),
        Map(HashMap<String, Option<String>>),
    }

    let env = Option::<EnvFmt>::deserialize(deserializer)?;
    let mut map = HashMap::new();
    match env {
        Some(EnvFmt::List(list)) => {
            for item in list {
                if let Some((k, v)) = item.split_once('=') {
                    map.insert(k.to_string(), v.to_string());
                } else {
                    map.insert(item, "".to_string());
                }
            }
        }
        Some(EnvFmt::Map(m)) => {
            for (k, v) in m {
                map.insert(k, v.unwrap_or_default());
            }
        }
        None => {}
    }
    Ok(map)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DependsOnCondition {
    pub condition: String, // "service_started", "service_healthy", "service_completed_successfully"
}

fn deserialize_depends_on<'de, D>(deserializer: D) -> Result<HashMap<String, DependsOnCondition>, D::Error>
where
    D: Deserializer<'de>,
{
    #[derive(Deserialize)]
    #[serde(untagged)]
    enum DependsFmt {
        List(Vec<String>),
        Map(HashMap<String, DependsOnCondition>),
    }

    let deps = Option::<DependsFmt>::deserialize(deserializer)?;
    let mut map = HashMap::new();
    match deps {
        Some(DependsFmt::List(list)) => {
            for item in list {
                map.insert(item, DependsOnCondition { condition: "service_started".to_string() });
            }
        }
        Some(DependsFmt::Map(m)) => {
            map = m;
        }
        None => {}
    }
    Ok(map)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeployConfig {
    pub resources: Option<ResourceConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConfig {
    pub limits: Option<ResourceConstraints>,
    pub reservations: Option<ResourceConstraints>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResourceConstraints {
    pub cpus: Option<String>,
    pub memory: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NetworkV3 {
    pub driver: Option<String>,
    pub external: Option<bool>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct VolumeV3 {
    pub driver: Option<String>,
    pub external: Option<bool>,
}

// Validation & Translation

impl ComposeV3 {
    pub fn validate(&self) -> Result<()> {
        let mut errors = Vec::new();

        let mut published_ports = HashSet::new();

        for (name, service) in &self.services {
            // Check depends_on
            for dep in service.depends_on.keys() {
                if !self.services.contains_key(dep) {
                    errors.push(format!("Service '{}' depends on undefined service '{}'", name, dep));
                }
            }

            // Check networks
            for net in &service.networks {
                if net != "default" && !self.networks.contains_key(net) {
                    errors.push(format!("Service '{}' references undefined network '{}'", name, net));
                }
            }

            // Check ports
            for port in &service.ports {
                let published = match port {
                    PortEntry::Short(s) => {
                        let parts: Vec<&str> = s.split(':').collect();
                        if parts.len() == 2 {
                            parts[0].parse::<u16>().ok()
                        } else if parts.len() == 3 {
                            parts[1].parse::<u16>().ok()
                        } else {
                            None
                        }
                    }
                    PortEntry::Long(l) => l.published,
                };
                
                if let Some(p) = published {
                    if !published_ports.insert(p) {
                        errors.push(format!("Port conflict: Host port {} is published multiple times", p));
                    }
                }
            }
        }

        if errors.is_empty() {
            Ok(())
        } else {
            Err(anyhow!("Validation failed:\n{}", errors.join("\n")))
        }
    }

    pub fn translate(&self) -> Result<Vec<vyoma_proto::v1::CreateVmRequest>> {
        self.validate()?;
        let mut requests = Vec::new();

        // For proper sorting, we could reuse start_order logic, but for translation,
        // order doesn't strictly matter if the daemon handles dispatching correctly,
        // or we sort them here. Let's sort alphabetically for determinism.
        let mut keys: Vec<_> = self.services.keys().collect();
        keys.sort();

        for name in keys {
            let service = self.services.get(name).unwrap();
            let (vcpus, memory_mib) = ResourceTranslator::translate(service.deploy.as_ref());
            
            let mut proto_ports = Vec::new();
            for p in &service.ports {
                match p {
                    PortEntry::Short(s) => {
                        let parts: Vec<&str> = s.split(':').collect();
                        if parts.len() == 2 {
                            if let (Ok(h), Ok(v)) = (parts[0].parse(), parts[1].parse()) {
                                proto_ports.push(vyoma_proto::v1::PortMapping { host: h, vm: v });
                            }
                        } else if parts.len() == 3 {
                            if let (Ok(h), Ok(v)) = (parts[1].parse(), parts[2].parse()) {
                                proto_ports.push(vyoma_proto::v1::PortMapping { host: h, vm: v });
                            }
                        }
                    }
                    PortEntry::Long(l) => {
                        if let Some(published) = l.published {
                            proto_ports.push(vyoma_proto::v1::PortMapping {
                                host: published as u32,
                                vm: l.target as u32,
                            });
                        }
                    }
                }
            }

            let mut proto_volumes = Vec::new();
            for v in &service.volumes {
                match v {
                    VolumeEntry::Short(s) => {
                        let parts: Vec<&str> = s.split(':').collect();
                        if parts.len() >= 2 {
                            proto_volumes.push(vyoma_proto::v1::VolumeMapping {
                                host_path: parts[0].to_string(),
                                vm_path: parts[1].to_string(),
                            });
                        }
                    }
                    VolumeEntry::Long(l) => {
                        if let Some(src) = &l.source {
                            proto_volumes.push(vyoma_proto::v1::VolumeMapping {
                                host_path: src.clone(),
                                vm_path: l.target.clone(),
                            });
                        }
                    }
                }
            }

            requests.push(vyoma_proto::v1::CreateVmRequest {
                image: service.image.clone().unwrap_or_else(|| "scratch".to_string()),
                vcpus,
                memory_mb: memory_mib as u64,
                name: name.clone(),
                ports: proto_ports,
                volumes: proto_volumes,
                networks: service.networks.clone(),
            });
        }

        Ok(requests)
    }
}

pub struct ResourceTranslator;

impl ResourceTranslator {
    pub fn translate(deploy: Option<&DeployConfig>) -> (u32, u32) {
        let cpu = deploy
            .and_then(|d| d.resources.as_ref())
            .and_then(|r| r.limits.as_ref())
            .and_then(|l| l.cpus.as_ref())
            .and_then(|c| c.parse::<f64>().ok())
            .map(|c| (c * 1000.0).ceil() as u32 / 1000)
            .unwrap_or(1);
            
        let mem = deploy
            .and_then(|d| d.resources.as_ref())
            .and_then(|r| r.limits.as_ref())
            .and_then(|l| l.memory.as_ref())
            .map(|m| Self::parse_mem_to_mib(m))
            .unwrap_or(512);
            
        (if cpu == 0 { 1 } else { cpu }, mem)
    }

    fn parse_mem_to_mib(mem: &str) -> u32 {
        let mem = mem.trim().to_uppercase();
        let chars: String = mem.chars().take_while(|c| c.is_digit(10) || *c == '.').collect();
        let unit: String = mem.chars().skip_while(|c| c.is_digit(10) || *c == '.').collect();
        
        let val = chars.parse::<f64>().unwrap_or(512.0);
        
        let multiplier = match unit.as_str() {
            "B" => 1.0 / (1024.0 * 1024.0),
            "K" | "KB" => 1.0 / 1024.0,
            "M" | "MB" => 1.0,
            "G" | "GB" => 1024.0,
            "T" | "TB" => 1024.0 * 1024.0,
            _ => 1.0, // Default to MB
        };
        
        (val * multiplier).ceil() as u32
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_compose_v3() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx:latest
    ports:
      - "8080:80"
  db:
    image: postgres:13
    deploy:
      resources:
        limits:
          cpus: "0.5"
          memory: "512M"
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        assert_eq!(compose.services.len(), 2);

        let web = compose.services.get("web").unwrap();
        assert_eq!(web.image.as_ref().unwrap(), "nginx:latest");
        assert_eq!(web.ports.len(), 1);

        let db = compose.services.get("db").unwrap();
        assert!(db.deploy.is_some());
        if let Some(deploy) = &db.deploy {
            if let Some(resources) = &deploy.resources {
                if let Some(limits) = &resources.limits {
                    assert_eq!(limits.cpus.as_ref().unwrap(), "0.5");
                    assert_eq!(limits.memory.as_ref().unwrap(), "512M");
                }
            }
        }
    }

    #[test]
    fn test_parse_long_port_format() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    ports:
      - target: 80
        published: 8080
        protocol: tcp
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.ports.len(), 1);

        match &web.ports[0] {
            PortEntry::Long(port) => {
                assert_eq!(port.target, 80);
                assert_eq!(port.published, Some(8080));
                assert_eq!(port.protocol.as_ref().unwrap(), "tcp");
            }
            PortEntry::Short(_) => panic!("Expected long format"),
        }
    }

    #[test]
    fn test_parse_long_volume_format() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    volumes:
      - type: bind
        source: /host/data
        target: /container/data
        read_only: true
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.volumes.len(), 1);

        match &web.volumes[0] {
            VolumeEntry::Long(vol) => {
                assert_eq!(vol.r#type, "bind");
                assert_eq!(vol.source.as_ref().unwrap(), "/host/data");
                assert_eq!(vol.target, "/container/data");
                assert_eq!(vol.read_only, Some(true));
            }
            VolumeEntry::Short(_) => panic!("Expected long format"),
        }
    }

    #[test]
    fn test_parse_env_as_list() {
        let yaml = r#"
version: "3.8"
services:
  app:
    image: alpine
    environment:
      - DB_HOST=localhost
      - DB_PORT=5432
      - DEBUG
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let app = compose.services.get("app").unwrap();
        assert_eq!(app.environment.get("DB_HOST").unwrap(), "localhost");
        assert_eq!(app.environment.get("DB_PORT").unwrap(), "5432");
        assert_eq!(app.environment.get("DEBUG").unwrap(), "");
    }

    #[test]
    fn test_parse_depends_on_list_format() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    depends_on:
      - db
      - redis
  db:
    image: postgres
  redis:
    image: redis
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.depends_on.len(), 2);
        assert!(web.depends_on.contains_key("db"));
        assert!(web.depends_on.contains_key("redis"));
        assert_eq!(web.depends_on.get("db").unwrap().condition, "service_started");
    }

    #[test]
    fn test_parse_depends_on_map_format() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    depends_on:
      db:
        condition: service_healthy
      redis:
        condition: service_started
  db:
    image: postgres
    healthcheck:
      test: ["CMD", "pg_isready"]
      interval: 10s
      timeout: 5s
      retries: 5
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.depends_on.len(), 2);
        assert_eq!(web.depends_on.get("db").unwrap().condition, "service_healthy");
        assert_eq!(web.depends_on.get("redis").unwrap().condition, "service_started");
    }

    #[test]
    fn test_validate_depends_on_undefined() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    depends_on:
      - undefined_service
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let result = compose.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("undefined_service"));
    }

    #[test]
    fn test_validate_network_undefined() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    networks:
      - undefined_net
networks:
  defined_net:
    driver: bridge
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let result = compose.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("undefined_net"));
    }

    #[test]
    fn test_validate_port_conflict() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    ports:
      - "8080:80"
      - "8080:443"
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let result = compose.validate();
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Port conflict"));
    }

    #[test]
    fn test_resource_translator_memory() {
        assert_eq!(ResourceTranslator::parse_mem_to_mib("512M"), 512);
        assert_eq!(ResourceTranslator::parse_mem_to_mib("1G"), 1024);
        assert_eq!(ResourceTranslator::parse_mem_to_mib("2048M"), 2048);
        assert_eq!(ResourceTranslator::parse_mem_to_mib("2G"), 2048);
        assert_eq!(ResourceTranslator::parse_mem_to_mib("1K"), 1); // 1KB = ~0.001MB -> ceil = 1
        assert_eq!(ResourceTranslator::parse_mem_to_mib("1024K"), 1); // 1024KB = 1MB
        assert_eq!(ResourceTranslator::parse_mem_to_mib("512"), 512); // Default to MB
    }

    #[test]
    fn test_resource_translator_cpu() {
        let deploy = DeployConfig {
            resources: Some(ResourceConfig {
                limits: Some(ResourceConstraints {
                    cpus: Some("0.5".to_string()),
                    memory: Some("512M".to_string()),
                }),
                reservations: None,
            }),
        };

        let (vcpus, mem) = ResourceTranslator::translate(Some(&deploy));
        assert_eq!(vcpus, 1); // 0.5 -> ceil(500) / 1000 = 1
        assert_eq!(mem, 512);
    }

    #[test]
    fn test_command_entry_string() {
        let yaml = r#"
version: "3.8"
services:
  app:
    image: nginx
    command: nginx -g 'daemon off;'
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let app = compose.services.get("app").unwrap();
        match app.command.as_ref().unwrap() {
            CommandEntry::String(s) => {
                assert_eq!(s, "nginx -g 'daemon off;'");
            }
            CommandEntry::List(_) => panic!("Expected String variant"),
        }
    }

    #[test]
    fn test_command_entry_list() {
        let yaml = r#"
version: "3.8"
services:
  app:
    image: nginx
    command:
      - nginx
      - -g
      - daemon off;
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let app = compose.services.get("app").unwrap();
        match app.command.as_ref().unwrap() {
            CommandEntry::List(l) => {
                assert_eq!(l.len(), 3);
            }
            CommandEntry::String(_) => panic!("Expected List variant"),
        }
    }

    #[test]
    fn test_command_to_vec() {
        assert_eq!(CommandEntry::String("nginx -g daemon".to_string()).to_vec(), vec!["nginx", "-g", "daemon"]);
        assert_eq!(CommandEntry::List(vec!["nginx".to_string(), "-g".to_string()]).to_vec(), vec!["nginx", "-g"]);
    }

    #[test]
    fn test_networks_field() {
        let yaml = r#"
version: "3.8"
services:
  web:
    image: nginx
    networks:
      - frontend
      - backend
networks:
  frontend:
    driver: bridge
  backend:
    driver: bridge
"#;
        let compose: ComposeV3 = serde_yaml::from_str(yaml).unwrap();
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.networks.len(), 2);
        assert!(web.networks.contains(&"frontend".to_string()));
        assert!(web.networks.contains(&"backend".to_string()));
    }
}
