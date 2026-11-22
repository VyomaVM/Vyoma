use std::collections::HashMap;
use std::path::Path;
use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Represents a parsed Vyomafile
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Vyomafile {
    pub instructions: Vec<Instruction>,
}

/// Instructions supported by Vyomafile
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum Instruction {
    From { image: String },
    Kernel { image: String },
    KernelCmdline { args: Vec<String> },
    Sysctl { key: String, value: String },
    Run { command: String },
    Copy { src: String, dst: String },
    Cmd { args: Vec<String> },
    Entrypoint { args: Vec<String> },
    Env { key: String, value: String },
    Workdir { path: String },
    VmMeasuredBoot,
}

impl Vyomafile {
    /// Parse a Vyomafile from disk
    pub fn parse(path: &Path) -> Result<Self> {
        let content = std::fs::read_to_string(path)
            .context("Failed to read Vyomafile")?;

        Self::parse_content(&content)
    }

    /// Parse Vyomafile content from string
    pub fn parse_content(content: &str) -> Result<Self> {
        let mut instructions = Vec::new();

        for (line_num, line) in content.lines().enumerate() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let instruction = Self::parse_line(line, line_num + 1)?;
            instructions.push(instruction);
        }

        Ok(Vyomafile { instructions })
    }

    /// Check if this Vyomafile requests measured boot
    pub fn has_measured_boot(&self) -> bool {
        self.instructions.iter().any(|inst| matches!(inst, Instruction::VmMeasuredBoot))
    }

    /// Retrieve the configured base kernel if specified via KERNEL instruction
    pub fn kernel(&self) -> Option<String> {
        self.instructions.iter().find_map(|inst| match inst {
            Instruction::Kernel { image } => Some(image.clone()),
            _ => None,
        })
    }

    /// Retrieve the configured kernel command-line arguments if specified via KERNEL_CMDLINE
    pub fn kernel_cmdline(&self) -> Option<Vec<String>> {
        self.instructions.iter().find_map(|inst| match inst {
            Instruction::KernelCmdline { args } => Some(args.clone()),
            _ => None,
        })
    }

    /// Retrieve all configured sysctl key-value pairs
    pub fn sysctls(&self) -> HashMap<String, String> {
        let mut map = HashMap::new();
        for inst in &self.instructions {
            if let Instruction::Sysctl { key, value } = inst {
                map.insert(key.clone(), value.clone());
            }
        }
        map
    }

    fn parse_line(line: &str, line_num: usize) -> Result<Instruction> {
        // Split instruction and arguments
        let parts: Vec<&str> = line.splitn(2, char::is_whitespace).collect();
        if parts.is_empty() {
            anyhow::bail!("Empty line at {}", line_num);
        }

        let instruction = parts[0].to_uppercase();
        let args = parts.get(1).unwrap_or(&"").trim();

        match instruction.as_str() {
            "FROM" => {
                if args.is_empty() {
                    anyhow::bail!("FROM requires an image name at line {}", line_num);
                }
                Ok(Instruction::From { image: args.to_string() })
            }
            "KERNEL" => {
                if args.is_empty() {
                    anyhow::bail!("KERNEL requires a kernel image name at line {}", line_num);
                }
                Ok(Instruction::Kernel { image: args.to_string() })
            }
            "KERNEL_CMDLINE" => {
                if args.is_empty() {
                    anyhow::bail!("KERNEL_CMDLINE requires arguments at line {}", line_num);
                }
                let cmd_args = Self::parse_shell_args(args)?;
                Ok(Instruction::KernelCmdline { args: cmd_args })
            }
            "SYSCTL" => {
                let sysctl_parts: Vec<&str> = args.splitn(2, '=').collect();
                if sysctl_parts.len() != 2 {
                    anyhow::bail!("SYSCTL requires KEY=VALUE format at line {}", line_num);
                }
                Ok(Instruction::Sysctl {
                    key: sysctl_parts[0].trim().to_string(),
                    value: sysctl_parts[1].trim().to_string(),
                })
            }
            "RUN" => {
                if args.is_empty() {
                    anyhow::bail!("RUN requires a command at line {}", line_num);
                }
                Ok(Instruction::Run { command: args.to_string() })
            }
            "COPY" => {
                let copy_parts: Vec<&str> = args.split_whitespace().collect();
                if copy_parts.len() != 2 {
                    anyhow::bail!("COPY requires src and dst arguments at line {}", line_num);
                }
                Ok(Instruction::Copy {
                    src: copy_parts[0].to_string(),
                    dst: copy_parts[1].to_string(),
                })
            }
            "CMD" => {
                let cmd_args = Self::parse_shell_args(args)?;
                Ok(Instruction::Cmd { args: cmd_args })
            }
            "ENTRYPOINT" => {
                let entry_args = Self::parse_shell_args(args)?;
                Ok(Instruction::Entrypoint { args: entry_args })
            }
            "ENV" => {
                let env_parts: Vec<&str> = args.splitn(2, '=').collect();
                if env_parts.len() != 2 {
                    anyhow::bail!("ENV requires KEY=VALUE format at line {}", line_num);
                }
                Ok(Instruction::Env {
                    key: env_parts[0].trim().to_string(),
                    value: env_parts[1].trim().to_string(),
                })
            }
            "WORKDIR" => {
                if args.is_empty() {
                    anyhow::bail!("WORKDIR requires a path at line {}", line_num);
                }
                Ok(Instruction::Workdir { path: args.to_string() })
            }
            "VM_MEASURED_BOOT" => {
                if args.is_empty() || args.eq_ignore_ascii_case("true") || args == "1" {
                    Ok(Instruction::VmMeasuredBoot)
                } else if args.eq_ignore_ascii_case("false") || args == "0" {
                    // Explicitly disabled measured boot: empty command
                    anyhow::bail!("VM_MEASURED_BOOT disabled explicitly at line {}", line_num);
                } else {
                    anyhow::bail!("VM_MEASURED_BOOT accepts only optional boolean flag at line {}", line_num);
                }
            }
            _ => {
                anyhow::bail!("Unknown instruction '{}' at line {}", instruction, line_num);
            }
        }
    }

    fn parse_shell_args(args: &str) -> Result<Vec<String>> {
        let trimmed = args.trim();
        if trimmed.starts_with('[') && trimmed.ends_with(']') {
            let json_str = trimmed;
            let parsed: Vec<String> = serde_json::from_str(json_str)
                .map_err(|e| anyhow::anyhow!("Failed to parse args as JSON: {}", e))?;
            Ok(parsed)
        } else {
            let mut result = Vec::new();
            let mut current = String::new();
            let mut in_quotes = false;
            let mut quote_char = '"';

            for ch in args.chars() {
                match ch {
                    '"' | '\'' if !in_quotes => {
                        in_quotes = true;
                        quote_char = ch;
                    }
                    '"' | '\'' if in_quotes && ch == quote_char => {
                        in_quotes = false;
                    }
                    ' ' if !in_quotes => {
                        if !current.is_empty() {
                            result.push(current);
                            current = String::new();
                        }
                    }
                    _ => {
                        current.push(ch);
                    }
                }
            }

            if !current.is_empty() {
                result.push(current);
            }

            Ok(result)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_vyomafile() {
        let content = r#"
FROM alpine:latest
RUN echo "hello world"
COPY app /app
CMD ["echo", "done"]
"#;

        let vyomafile = Vyomafile::parse_content(content).unwrap();
        assert_eq!(vyomafile.instructions.len(), 4);

        match &vyomafile.instructions[0] {
            Instruction::From { image } => assert_eq!(image, "alpine:latest"),
            _ => panic!("Expected FROM instruction"),
        }

        match &vyomafile.instructions[1] {
            Instruction::Run { command } => assert_eq!(command, "echo \"hello world\""),
            _ => panic!("Expected RUN instruction"),
        }

        match &vyomafile.instructions[2] {
            Instruction::Copy { src, dst } => {
                assert_eq!(src, "app");
                assert_eq!(dst, "/app");
            }
            _ => panic!("Expected COPY instruction"),
        }

        match &vyomafile.instructions[3] {
            Instruction::Cmd { args } => assert_eq!(args, &["echo", "done"]),
            _ => panic!("Expected CMD instruction"),
        }
    }

    #[test]
    fn test_parse_kernel_and_cmdline_instructions() {
        let content = r#"
FROM vyomahub.io/library/postgres:16
KERNEL vyomahub.io/kernels/db-heavy:6.16
KERNEL_CMDLINE transparent_hugepage=always elevator=deadline noswap
SYSCTL vm.dirty_ratio=15
SYSCTL vm.overcommit_memory=2
VM_MEASURED_BOOT true
"#;

        let vf = Vyomafile::parse_content(content).unwrap();
        assert_eq!(vf.kernel(), Some("vyomahub.io/kernels/db-heavy:6.16".to_string()));
        assert_eq!(
            vf.kernel_cmdline(),
            Some(vec!["transparent_hugepage=always".to_string(), "elevator=deadline".to_string(), "noswap".to_string()])
        );
        let sysctls = vf.sysctls();
        assert_eq!(sysctls.get("vm.dirty_ratio"), Some(&"15".to_string()));
        assert_eq!(sysctls.get("vm.overcommit_memory"), Some(&"2".to_string()));
        assert!(vf.has_measured_boot());
    }

    #[test]
    fn test_parse_env_instruction() {
        let vyomafile = Vyomafile::parse_content("ENV PORT=8080").unwrap();
        match &vyomafile.instructions[0] {
            Instruction::Env { key, value } => {
                assert_eq!(key, "PORT");
                assert_eq!(value, "8080");
            }
            _ => panic!("Expected ENV instruction"),
        }
    }

    #[test]
    fn test_parse_workdir_instruction() {
        let vyomafile = Vyomafile::parse_content("WORKDIR /app").unwrap();
        match &vyomafile.instructions[0] {
            Instruction::Workdir { path } => assert_eq!(path, "/app"),
            _ => panic!("Expected WORKDIR instruction"),
        }
    }

    #[test]
    fn test_parse_vm_measured_boot_instruction() {
        let vyomafile = Vyomafile::parse_content("VM_MEASURED_BOOT").unwrap();
        assert_eq!(vyomafile.instructions.len(), 1);
        assert!(vyomafile.has_measured_boot());

        let vyomafile2 = Vyomafile::parse_content("VM_MEASURED_BOOT true").unwrap();
        assert!(vyomafile2.has_measured_boot());
    }

    #[test]
    fn test_parse_vm_measured_boot_with_invalid_arg_fails() {
        let result = Vyomafile::parse_content("VM_MEASURED_BOOT invalid_arg");
        assert!(result.is_err());
    }
}
