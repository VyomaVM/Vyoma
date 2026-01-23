use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;
use anyhow::Result;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IgniteCompose {
    pub version: String,
    pub services: HashMap<String, Service>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Service {
    pub image: String,
    pub cpus: Option<u32>,
    pub memory: Option<u32>, // MB
    pub ports: Option<Vec<String>>, // "8080:80"
    pub volumes: Option<Vec<String>>, // "/host:/vm"
    pub environment: Option<HashMap<String, String>>,
    pub command: Option<String>,
    pub depends_on: Option<Vec<String>>,
}

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
        assert_eq!(web.image, "nginx:latest");
        assert_eq!(web.ports.as_ref().unwrap()[0], "8080:80");

        let db = compose.services.get("db").unwrap();
        assert_eq!(db.memory, Some(512));
    }
}
