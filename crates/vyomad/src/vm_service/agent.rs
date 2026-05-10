use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::fs::{self, File};
use std::io::Write;
use std::os::unix::fs::PermissionsExt;
use tracing::info;

use super::types::AgentConfig;
use crate::state::AppState;

pub async fn prepare_agent(
    _state: &AppState,
    _dm_path: &str,
    vm_dir: &Path,
    _config: &vyoma_core::oci::OciImageConfig,
) -> Result<AgentConfig> {
    let initramfs_path = generate_initramfs_pure(vm_dir)?;
    
    let temp_init_path = vm_dir.join("vyoma-init.sh");
    fs::write(&temp_init_path, "#!/bin/sh\nset -e\n")?;

    info!(
        "Agent prepared with initramfs at {:?} and init script at {:?}",
        initramfs_path, temp_init_path
    );

    Ok(AgentConfig {
        initramfs_path: Some(initramfs_path),
        init_script_path: temp_init_path,
        cmd: vec!["/sbin/init".to_string()],
        workdir: "/".to_string(),
        envs: vec![],
    })
}

fn generate_initramfs_pure(vm_dir: &Path) -> Result<PathBuf> {
    let temp_dir = vm_dir.join("initramfs_temp");
    fs::create_dir_all(&temp_dir)?;
    
    let init_script = generate_init_script();
    let init_path = temp_dir.join("init");
    fs::write(&init_path, &init_script)?;
    fs::set_permissions(&init_path, PermissionsExt::from_mode(0o755))?;

    let sbin_dir = temp_dir.join("sbin");
    fs::create_dir_all(&sbin_dir)?;
    
    let agent_binary = PathBuf::from("/usr/bin/vyoma-agent-vm");
    if agent_binary.exists() {
        fs::copy(&agent_binary, sbin_dir.join("vyoma-agent-vm"))
            .context("Failed to copy agent binary")?;
        fs::set_permissions(sbin_dir.join("vyoma-agent-vm"), PermissionsExt::from_mode(0o755))?;
    }

    let dev_dir = temp_dir.join("dev");
    fs::create_dir_all(&dev_dir)?;
    create_device_nodes(&dev_dir)?;

    let initramfs_path = vm_dir.join("initramfs.cpio");
    let mut cpio_writer = CpioWriter::new(&initramfs_path)?;

    collect_and_write_entries(&mut cpio_writer, &temp_dir)?;

    cpio_writer.finish()?;

    fs::remove_dir_all(&temp_dir).ok();

    info!("Generated initramfs (pure Rust): {} bytes", fs::metadata(&initramfs_path)?.len());
    Ok(initramfs_path)
}

fn create_device_nodes(dev_dir: &Path) -> Result<()> {
    let devices = [
        ("null", &[0u8; 0]),
        ("zero", &[0u8; 0]),
        ("console", &[0u8; 0]),
    ];
    
    for (name, _) in devices {
        fs::write(dev_dir.join(name), b"").ok();
    }
    Ok(())
}

fn collect_and_write_entries(cpio: &mut CpioWriter, dir: &Path) -> Result<()> {
    collect_entries_recursive(cpio, dir, "")?;
    Ok(())
}

fn collect_entries_recursive(cpio: &mut CpioWriter, dir: &Path, prefix: &str) -> Result<()> {
    for entry in fs::read_dir(dir)? {
        let entry = entry?;
        let name = if prefix.is_empty() {
            entry.file_name().to_string_lossy().to_string()
        } else {
            format!("{}/{}", prefix, entry.file_name().to_string_lossy())
        };
        
        let metadata = entry.metadata()?;
        
        if metadata.is_dir() {
            add_dir_entry(cpio, &name)?;
            collect_entries_recursive(cpio, &entry.path(), &name)?;
        } else if metadata.is_file() {
            let content = fs::read(entry.path())?;
            add_file_entry(cpio, &name, &content)?;
        }
    }
    Ok(())
}

struct CpioWriter {
    file: File,
}

impl CpioWriter {
    fn new(path: &Path) -> Result<Self> {
        let file = File::create(path).context("Failed to create cpio file")?;
        Ok(Self { file })
    }

    fn add_entry(&mut self, pathname: &str, mode: u32, content: &[u8]) -> Result<()> {
        let mut header = [0u8; CPIO_HEADER_SIZE];
        
        write_cpio_field(&mut header, 0, 6, "070701");
        write_cpio_hex(&mut header, 6, 8, 0u64);
        write_cpio_hex(&mut header, 14, 8, 0u64);
        write_cpio_hex(&mut header, 22, 8, 0u64);
        write_cpio_hex(&mut header, 30, 8, 0u64);
        write_cpio_hex(&mut header, 38, 8, 0u64);
        write_cpio_hex(&mut header, 46, 8, 0u64);
        write_cpio_hex(&mut header, 54, 8, content.len() as u64);
        write_cpio_hex(&mut header, 62, 8, 0u64);
        write_cpio_hex(&mut header, 70, 8, 0u64);
        write_cpio_hex(&mut header, 78, 4, (pathname.len() + 1) as u64);
        write_cpio_field(&mut header, 82, 2, "1");
        write_cpio_string(&mut header, 84, pathname);

        self.file.write_all(&header)?;
        self.file.write_all(content)?;
        
        let padding = (4 - (content.len() % 4)) % 4;
        if padding > 0 {
            self.file.write_all(&[0u8; 4][..padding])?;
        }

        Ok(())
    }

    fn finish(mut self) -> Result<()> {
        let trailer = format!("070701{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}{:08x}TRAILER!!!",
            0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 0u64, 1u64, 1u64);
        
        self.file.write_all(trailer.as_bytes())?;
        
        let padding = (4 - (trailer.len() % 4)) % 4;
        if padding > 0 {
            self.file.write_all(&[0u8; 4][..padding])?;
        }

        self.file.flush()?;
        Ok(())
    }
}

const CPIO_HEADER_SIZE: usize = 110;

fn write_cpio_field(buf: &mut [u8], offset: usize, len: usize, value: &str) {
    for (i, byte) in value.bytes().enumerate().take(len) {
        if offset + i < buf.len() {
            buf[offset + i] = byte;
        }
    }
}

fn write_cpio_hex(buf: &mut [u8], offset: usize, len: usize, value: u64) {
    let s = format!("{:08x}", value);
    write_cpio_field(buf, offset, len, &s);
}

fn write_cpio_string(buf: &mut [u8], offset: usize, value: &str) {
    for (i, byte) in value.bytes().enumerate() {
        if offset + i < buf.len() {
            buf[offset + i] = byte;
        }
    }
    if offset + value.len() < buf.len() {
        buf[offset + value.len()] = 0;
    }
}

const DIR_MODE: u32 = 0o040755;
const FILE_MODE: u32 = 0o100644;

fn add_dir_entry(cpio: &mut CpioWriter, path: &str) -> Result<()> {
    let name = if path.is_empty() { "." } else { path };
    cpio.add_entry(name, DIR_MODE, &[])?;
    Ok(())
}

fn add_file_entry(cpio: &mut CpioWriter, path: &str, content: &[u8]) -> Result<()> {
    cpio.add_entry(path, FILE_MODE, content)?;
    Ok(())
}

fn generate_init_script() -> String {
    r#"#!/bin/sh
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sys /sys 2>/dev/null || true
mount -t devtmpfs dev /dev 2>/dev/null || true
ip link set lo up 2>/dev/null || true
/sbin/vyoma-agent-vm &
sleep 1
exec /sbin/init
"#.to_string()
}

pub async fn cleanup_agent(_agent_config: &AgentConfig) -> Result<()> {
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::TempDir;

    #[test]
    fn test_generate_init_script() {
        let script = generate_init_script();
        assert!(script.contains("#!/bin/sh"));
        assert!(script.contains("vyoma-agent-vm"));
        assert!(script.contains("mount"));
    }

    #[test]
    fn test_cpio_writer_creation() {
        let temp_dir = TempDir::new().unwrap();
        let cpio_path = temp_dir.path().join("test.cpio");
        
        let mut writer = CpioWriter::new(&cpio_path).unwrap();
        add_dir_entry(&mut writer, "dev").unwrap();
        add_file_entry(&mut writer, "test.txt", b"hello world").unwrap();
        writer.finish().unwrap();
        
        assert!(cpio_path.exists());
        assert!(cpio_path.metadata().unwrap().len() > 0);
    }
}