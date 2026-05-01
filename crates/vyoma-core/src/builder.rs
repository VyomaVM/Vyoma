use serde::{Deserialize, Serialize};
use anyhow::{Result, anyhow};
use std::path::Path;
use std::fs::File;
use std::io::{BufRead, BufReader};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Instruction {
    From(String),
    Run(String),
    Copy { src: String, dest: String },
    Cmd(Vec<String>),
    Entrypoint(Vec<String>),
    Env { key: String, value: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vyomafile {
    pub instructions: Vec<Instruction>,
}

impl Vyomafile {
    pub fn parse(path: &Path) -> Result<Self> {
        let file = File::open(path)?;
        let reader = BufReader::new(file);
        let mut instructions = Vec::new();

        for line in reader.lines() {
            let line = line?;
            let trimmed = line.trim();
            if trimmed.is_empty() || trimmed.starts_with('#') {
                continue;
            }

            let parts: Vec<&str> = trimmed.splitn(2, ' ').collect();
            if parts.is_empty() {
                continue;
            }

            match parts[0].to_uppercase().as_str() {
                "FROM" => {
                    if parts.len() < 2 { return Err(anyhow!("FROM requires an argument")); }
                    instructions.push(Instruction::From(parts[1].trim().to_string()));
                },
                "RUN" => {
                    if parts.len() < 2 { return Err(anyhow!("RUN requires an argument")); }
                    instructions.push(Instruction::Run(parts[1].trim().to_string()));
                },
                "COPY" => {
                     if parts.len() < 2 { return Err(anyhow!("COPY requires arguments")); }
                     let args = parts[1].trim();
                     // Naive split by space (doesn't handle quotes yet)
                     let copy_parts: Vec<&str> = args.split_whitespace().collect();
                     if copy_parts.len() < 2 { return Err(anyhow!("COPY requires src and dest")); }
                     instructions.push(Instruction::Copy {
                         src: copy_parts[0].to_string(),
                         dest: copy_parts[1].to_string(),
                     });
                },
                "CMD" => {
                    if parts.len() < 2 { return Err(anyhow!("CMD requires arguments")); }
                    let value = parts[1].trim();
                    let args = if value.starts_with('[') && value.ends_with(']') {
                        serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| vec![value.to_string()])
                    } else {
                        vec![value.to_string()]
                    };
                    instructions.push(Instruction::Cmd(args));
                },
                "ENTRYPOINT" => {
                    if parts.len() < 2 { return Err(anyhow!("ENTRYPOINT requires arguments")); }
                    let value = parts[1].trim();
                    let args = if value.starts_with('[') && value.ends_with(']') {
                        serde_json::from_str::<Vec<String>>(value).unwrap_or_else(|_| vec![value.to_string()])
                    } else {
                        vec![value.to_string()]
                    };
                    instructions.push(Instruction::Entrypoint(args));
                },
                "ENV" => {
                    if parts.len() < 2 { return Err(anyhow!("ENV requires arguments")); }
                    let value = parts[1].trim();
                    if let Some((k, v)) = value.split_once('=') {
                        instructions.push(Instruction::Env {
                            key: k.trim().to_string(),
                            value: v.trim().to_string()
                        });
                    } else {
                        let env_parts: Vec<&str> = value.split_whitespace().collect();
                        if env_parts.len() >= 2 {
                             instructions.push(Instruction::Env {
                                 key: env_parts[0].to_string(),
                                 value: env_parts[1..].join(" "),
                             });
                        }
                    }
                },
                _ => {
                    return Err(anyhow!("Unknown instruction: {}", parts[0]));
                }
            }
        }

        Ok(Self { instructions })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_vyomafile() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "FROM alpine:latest").unwrap();
        writeln!(file, "RUN apk add curl").unwrap();
        writeln!(file, "COPY . /app").unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "").unwrap(); // Empty line

        let vyoma_file = Vyomafile::parse(file.path()).unwrap();
        
        assert_eq!(vyoma_file.instructions.len(), 3);
        
        match &vyoma_file.instructions[0] {
            Instruction::From(img) => assert_eq!(img, "alpine:latest"),
            _ => panic!("Expected FROM"),
        }
        
        match &vyoma_file.instructions[1] {
            Instruction::Run(cmd) => assert_eq!(cmd, "apk add curl"),
            _ => panic!("Expected RUN"),
        }
        
        match &vyoma_file.instructions[2] {
            Instruction::Copy { src, dest } => {
                assert_eq!(src, ".");
                assert_eq!(dest, "/app");
            },
            _ => panic!("Expected COPY"),
        }
    }
}
