use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgniteCompose {
    pub version: String,
    pub services: HashMap<String, Service>,
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
    pub ignitefile: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub image: Option<String>,
    pub build: Option<BuildSource>,
    pub cpus: Option<u32>,
    pub memory: Option<u32>, // MB
    pub ports: Option<Vec<String>>, // "8080:80"
    pub volumes: Option<Vec<String>>, // "/host:/vm"
    pub environment: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub depends_on: Option<Vec<String>>,
}

// ...

impl IgniteCompose {
    pub fn from_file<P: AsRef<Path>>(path: P) -> Result<Self> {
        let content = fs::read_to_string(path)?;
        let compose: IgniteCompose = serde_yaml::from_str(&content)?;
        Ok(compose)
    }

    pub fn from_str(content: &str) -> Result<Self> {
        let compose: IgniteCompose = serde_yaml::from_str(content)?;
        Ok(compose)
    }

    pub fn start_order(&self) -> Result<Vec<(String, Service)>> {
        let mut order = Vec::new();
        let mut visited = HashSet::new();
        let mut visiting = HashSet::new();

        // Sort keys for deterministic output on independent nodes
        let mut keys: Vec<_> = self.services.keys().collect();
        keys.sort();

        for name in keys {
            self.visit(name, &mut visited, &mut visiting, &mut order)?;
        }
        
        Ok(order)
    }

    fn visit(&self, name: &String, visited: &mut HashSet<String>, visiting: &mut HashSet<String>, order: &mut Vec<(String, Service)>) -> Result<()> {
        if visited.contains(name) { return Ok(()); }
        if visiting.contains(name) { return Err(anyhow::anyhow!("Circular dependency detected involving {}", name)); }

        visiting.insert(name.clone());

        if let Some(service) = self.services.get(name) {
            if let Some(deps) = &service.depends_on {
                for dep in deps {
                    if !self.services.contains_key(dep) {
                         return Err(anyhow::anyhow!("Service '{}' depends on undefined service '{}'", name, dep));
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
        let compose = IgniteCompose::from_str(yaml).unwrap();
        assert_eq!(compose.version, "1.0");
        assert_eq!(compose.services.len(), 2);
        
        let web = compose.services.get("web").unwrap();
        assert_eq!(web.image.as_ref().unwrap(), "nginx:latest");
        assert_eq!(web.ports.as_ref().unwrap()[0], "8080:80");

        let db = compose.services.get("db").unwrap();
        assert_eq!(db.memory, Some(512));
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
      ignitefile: CustomIgnitefile
"#;
        let compose = IgniteCompose::from_str(yaml).unwrap();
        let app = compose.services.get("app").unwrap();
        match app.build.as_ref().unwrap() {
            BuildSource::Path(p) => assert_eq!(p, "./app"),
            _ => panic!("Expected BuildSource::Path"),
        }

        let worker = compose.services.get("worker").unwrap();
        match worker.build.as_ref().unwrap() {
            BuildSource::Config(c) => {
                assert_eq!(c.context, "./worker");
                assert_eq!(c.ignitefile.as_ref().unwrap(), "CustomIgnitefile");
            }
            _ => panic!("Expected BuildSource::Config"),
        }
    }
}
