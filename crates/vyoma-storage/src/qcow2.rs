use std::fs::OpenOptions;
use std::io::{Read, Seek, SeekFrom, Write};
use std::path::Path;
use std::process::Command;
use tracing::{info, error};
use serde::{Deserialize, Serialize};

use crate::error::{StorageError, Result};

const QCOW2_MAGIC: u32 = 0x5146_49fb;
const QCOW2_VERSION_3: u32 = 3;
const CLUSTER_BITS: u32 = 16;
const CLUSTER_SIZE: u64 = 1 << CLUSTER_BITS; // 64 KiB
const REFCOUNT_ORDER: u32 = 4; // 16-bit refcounts
const HEADER_LEN_V3: u32 = 104;
const QCOW2_EXT_END: u32 = 0x0000_0000;
const QCOW2_EXT_BACKING_FORMAT: u32 = 0xe279_2aca;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Qcow2Info {
    #[serde(rename = "virtual-size")]
    pub virtual_size: u64,
    pub filename: String,
    pub format: String,
    #[serde(rename = "actual-size")]
    pub actual_size: Option<u64>,
    #[serde(rename = "backing-filename")]
    pub backing_filename: Option<String>,
    #[serde(rename = "backing-filename-format")]
    pub backing_filename_format: Option<String>,
}

pub struct Qcow2Manager;

impl Qcow2Manager {
    /// Write a minimal, spec-compliant QCOW2 v3 image or backing-file overlay purely in Rust.
    fn write_qcow2_v3(
        path: &Path,
        virtual_size_bytes: u64,
        backing_file: Option<&str>,
        backing_format: Option<&str>,
    ) -> Result<()> {
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(StorageError::Io)?;
        }

        let virtual_size = virtual_size_bytes.max(512);
        let l2_entries_per_table = CLUSTER_SIZE / 8; // 8192 entries
        let bytes_per_l1_entry = l2_entries_per_table * CLUSTER_SIZE; // 512 MiB
        let l1_size = ((virtual_size + bytes_per_l1_entry - 1) / bytes_per_l1_entry).max(1);
        let l1_bytes = l1_size * 8;
        let l1_clusters = (l1_bytes + CLUSTER_SIZE - 1) / CLUSTER_SIZE;

        let refcount_table_offset = CLUSTER_SIZE;
        let refcount_block_offset = 2 * CLUSTER_SIZE;
        let l1_table_offset = 3 * CLUSTER_SIZE;
        let total_meta_clusters = 3 + l1_clusters;

        // Build extensions block starting immediately after the 104-byte v3 header
        let mut ext_buf = Vec::new();
        if let Some(fmt) = backing_format {
            let fmt_bytes = fmt.as_bytes();
            ext_buf.extend_from_slice(&QCOW2_EXT_BACKING_FORMAT.to_be_bytes());
            ext_buf.extend_from_slice(&(fmt_bytes.len() as u32).to_be_bytes());
            ext_buf.extend_from_slice(fmt_bytes);
            let pad = (8 - (fmt_bytes.len() % 8)) % 8;
            ext_buf.resize(ext_buf.len() + pad, 0);
        }
        // End-of-extensions marker
        ext_buf.extend_from_slice(&QCOW2_EXT_END.to_be_bytes());
        ext_buf.extend_from_slice(&0u32.to_be_bytes());

        let (backing_offset, backing_size, backing_bytes) = if let Some(bf) = backing_file {
            let bytes = bf.as_bytes();
            let offset = HEADER_LEN_V3 as usize + ext_buf.len();
            if offset + bytes.len() > CLUSTER_SIZE as usize {
                return Err(StorageError::Path(format!(
                    "Backing file path exceeds QCOW2 header cluster: {}",
                    bf
                )));
            }
            (offset as u64, bytes.len() as u32, bytes)
        } else {
            (0u64, 0u32, &[][..])
        };

        // Construct cluster 0 (header + extensions + backing filename)
        let mut cluster0 = vec![0u8; CLUSTER_SIZE as usize];
        cluster0[0..4].copy_from_slice(&QCOW2_MAGIC.to_be_bytes());
        cluster0[4..8].copy_from_slice(&QCOW2_VERSION_3.to_be_bytes());
        cluster0[8..16].copy_from_slice(&backing_offset.to_be_bytes());
        cluster0[16..20].copy_from_slice(&backing_size.to_be_bytes());
        cluster0[20..24].copy_from_slice(&CLUSTER_BITS.to_be_bytes());
        cluster0[24..32].copy_from_slice(&virtual_size.to_be_bytes());
        cluster0[32..36].copy_from_slice(&0u32.to_be_bytes()); // crypt_method = none
        cluster0[36..40].copy_from_slice(&(l1_size as u32).to_be_bytes());
        cluster0[40..48].copy_from_slice(&l1_table_offset.to_be_bytes());
        cluster0[48..56].copy_from_slice(&refcount_table_offset.to_be_bytes());
        cluster0[56..60].copy_from_slice(&1u32.to_be_bytes()); // refcount_table_clusters
        cluster0[60..64].copy_from_slice(&0u32.to_be_bytes()); // nb_snapshots
        cluster0[64..72].copy_from_slice(&0u64.to_be_bytes()); // snapshots_offset
        cluster0[72..80].copy_from_slice(&0u64.to_be_bytes()); // incompatible_features
        cluster0[80..88].copy_from_slice(&0u64.to_be_bytes()); // compatible_features
        cluster0[88..96].copy_from_slice(&0u64.to_be_bytes()); // autoclear_features
        cluster0[96..100].copy_from_slice(&REFCOUNT_ORDER.to_be_bytes());
        cluster0[100..104].copy_from_slice(&HEADER_LEN_V3.to_be_bytes());

        let ext_start = HEADER_LEN_V3 as usize;
        cluster0[ext_start..ext_start + ext_buf.len()].copy_from_slice(&ext_buf);
        if !backing_bytes.is_empty() {
            let bo = backing_offset as usize;
            cluster0[bo..bo + backing_bytes.len()].copy_from_slice(backing_bytes);
        }

        // Cluster 1: Refcount table pointing entry 0 -> cluster 2 (refcount block)
        let mut cluster1 = vec![0u8; CLUSTER_SIZE as usize];
        cluster1[0..8].copy_from_slice(&refcount_block_offset.to_be_bytes());

        // Cluster 2: Refcount block marking clusters 0..total_meta_clusters as refcount = 1
        let mut cluster2 = vec![0u8; CLUSTER_SIZE as usize];
        let max_entries = (CLUSTER_SIZE / 2) as u64;
        if total_meta_clusters > max_entries {
            return Err(StorageError::Other(
                "Requested QCOW2 virtual size exceeds single refcount block capacity".to_string(),
            ));
        }
        for i in 0..total_meta_clusters as usize {
            cluster2[i * 2..i * 2 + 2].copy_from_slice(&1u16.to_be_bytes());
        }

        let mut file = OpenOptions::new()
            .create(true)
            .write(true)
            .truncate(true)
            .open(path)
            .map_err(StorageError::Io)?;
        file.write_all(&cluster0).map_err(StorageError::Io)?;
        file.write_all(&cluster1).map_err(StorageError::Io)?;
        file.write_all(&cluster2).map_err(StorageError::Io)?;
        // Clusters 3..3+l1_clusters: zeroed L1 table
        let l1_zeros = vec![0u8; (l1_clusters * CLUSTER_SIZE) as usize];
        file.write_all(&l1_zeros).map_err(StorageError::Io)?;
        file.sync_all().map_err(StorageError::Io)?;

        Ok(())
    }

    /// Read a QCOW2 file's header metadata natively in Rust (fallback when qemu-img is absent).
    fn parse_qcow2_header(path: &Path) -> Result<Qcow2Info> {
        let mut file = OpenOptions::new().read(true).open(path).map_err(StorageError::Io)?;
        let actual_size = file.metadata().map(|m| m.len()).ok();
        let mut hdr = [0u8; 104];
        file.read_exact(&mut hdr).map_err(StorageError::Io)?;

        let magic = u32::from_be_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        if magic != QCOW2_MAGIC {
            return Err(StorageError::Other(format!(
                "Not a valid QCOW2 file (magic=0x{:08x})",
                magic
            )));
        }

        let version = u32::from_be_bytes([hdr[4], hdr[5], hdr[6], hdr[7]]);
        let backing_offset = u64::from_be_bytes([
            hdr[8], hdr[9], hdr[10], hdr[11], hdr[12], hdr[13], hdr[14], hdr[15],
        ]);
        let backing_size = u32::from_be_bytes([hdr[16], hdr[17], hdr[18], hdr[19]]);
        let virtual_size = u64::from_be_bytes([
            hdr[24], hdr[25], hdr[26], hdr[27], hdr[28], hdr[29], hdr[30], hdr[31],
        ]);
        let header_length = if version >= 3 {
            u32::from_be_bytes([hdr[100], hdr[101], hdr[102], hdr[103]]) as usize
        } else {
            72usize
        };

        let backing_filename = if backing_offset > 0 && backing_size > 0 {
            file.seek(SeekFrom::Start(backing_offset)).map_err(StorageError::Io)?;
            let mut bbuf = vec![0u8; backing_size as usize];
            file.read_exact(&mut bbuf).map_err(StorageError::Io)?;
            Some(String::from_utf8_lossy(&bbuf).to_string())
        } else {
            None
        };

        let mut backing_filename_format = None;
        let ext_end_limit = if backing_offset > 0 {
            backing_offset as usize
        } else {
            CLUSTER_SIZE as usize
        };
        if header_length >= 104 && header_length < ext_end_limit {
            file.seek(SeekFrom::Start(header_length as u64)).map_err(StorageError::Io)?;
            let mut pos = header_length;
            while pos + 8 <= ext_end_limit {
                let mut ext_hdr = [0u8; 8];
                if file.read_exact(&mut ext_hdr).is_err() {
                    break;
                }
                let ext_type = u32::from_be_bytes([ext_hdr[0], ext_hdr[1], ext_hdr[2], ext_hdr[3]]);
                let ext_len = u32::from_be_bytes([ext_hdr[4], ext_hdr[5], ext_hdr[6], ext_hdr[7]]) as usize;
                pos += 8;
                if ext_type == QCOW2_EXT_END {
                    break;
                }
                let padded_len = (ext_len + 7) & !7;
                if pos + padded_len > ext_end_limit {
                    break;
                }
                let mut data = vec![0u8; padded_len];
                if file.read_exact(&mut data).is_err() {
                    break;
                }
                if ext_type == QCOW2_EXT_BACKING_FORMAT {
                    backing_filename_format = Some(String::from_utf8_lossy(&data[..ext_len]).to_string());
                }
                pos += padded_len;
            }
        }

        Ok(Qcow2Info {
            virtual_size,
            filename: path.to_string_lossy().to_string(),
            format: "qcow2".to_string(),
            actual_size,
            backing_filename,
            backing_filename_format,
        })
    }

    /// Determine virtual size of a raw or QCOW2 backing file in bytes.
    fn detect_backing_virtual_size(backing_file: &Path) -> Result<u64> {
        if let Ok(info) = Self::parse_qcow2_header(backing_file) {
            return Ok(info.virtual_size);
        }
        let len = std::fs::metadata(backing_file).map_err(StorageError::Io)?.len();
        Ok(len.max(512))
    }

    /// Create a new independent QCOW2 image of specified size in megabytes.
    pub fn create_image(path: &Path, size_mb: u64) -> Result<()> {
        info!("Creating QCOW2 disk image at {:?} ({}MB)", path, size_mb);
        Self::write_qcow2_v3(path, size_mb * 1024 * 1024, None, None)?;
        info!("Successfully created QCOW2 image at {:?}", path);
        Ok(())
    }

    /// Create a copy-on-write overlay image pointing to a backing file.
    /// Cloud Hypervisor v54.0 natively supports QCOW2 backing files for instant VM branching.
    pub fn create_overlay(overlay_path: &Path, backing_file: &Path) -> Result<()> {
        let backing_fmt = if backing_file.extension().and_then(|e| e.to_str()) == Some("qcow2") {
            "qcow2"
        } else {
            "raw"
        };
        Self::create_overlay_with_format(overlay_path, backing_file, backing_fmt, None)
    }

    /// Create a QCOW2 overlay image with an explicit backing format (`"raw"` or `"qcow2"`)
    /// and optional virtual size in megabytes (defaults to backing file virtual size).
    pub fn create_overlay_with_format(
        overlay_path: &Path,
        backing_file: &Path,
        backing_fmt: &str,
        virtual_size_mb: Option<u64>,
    ) -> Result<()> {
        info!(
            "Creating QCOW2 overlay {:?} backed by {:?} (format={})",
            overlay_path, backing_file, backing_fmt
        );
        if !backing_file.exists() {
            return Err(StorageError::NotFound(format!(
                "Backing file does not exist: {:?}",
                backing_file
            )));
        }
        let backing_path_str = backing_file.to_str().ok_or_else(|| {
            StorageError::Path(format!("Invalid backing file path: {:?}", backing_file))
        })?;

        let virtual_size_bytes = match virtual_size_mb {
            Some(mb) => mb * 1024 * 1024,
            None => Self::detect_backing_virtual_size(backing_file)?,
        };

        Self::write_qcow2_v3(
            overlay_path,
            virtual_size_bytes,
            Some(backing_path_str),
            Some(backing_fmt),
        )?;

        info!("Successfully created QCOW2 overlay at {:?}", overlay_path);
        Ok(())
    }

    /// Resize an existing QCOW2 image.
    /// Delegates to `qemu-img resize` if available; otherwise updates the QCOW2 v3 header
    /// when growth stays within the allocated L1 table clusters.
    pub fn resize_image(path: &Path, new_size_mb: u64) -> Result<()> {
        info!("Resizing QCOW2 image at {:?} to {}MB", path, new_size_mb);
        if !path.exists() {
            return Err(StorageError::NotFound(format!("File not found: {:?}", path)));
        }

        if let Ok(output) = Command::new("qemu-img")
            .arg("resize")
            .arg(path)
            .arg(format!("{}M", new_size_mb))
            .output()
        {
            if output.status.success() {
                info!("Successfully resized QCOW2 image via qemu-img at {:?}", path);
                return Ok(());
            }
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("qemu-img resize failed: {}", stderr);
            return Err(StorageError::Io(std::io::Error::new(
                std::io::ErrorKind::Other,
                format!("qemu-img resize failed: {}", stderr),
            )));
        }

        // Pure-Rust fallback when qemu-img is not installed
        let new_size_bytes = new_size_mb * 1024 * 1024;
        let info = Self::parse_qcow2_header(path)?;
        if new_size_bytes < info.virtual_size {
            return Err(StorageError::Other(
                "Shrinking QCOW2 images requires qemu-img to be installed".to_string(),
            ));
        }

        let mut file = OpenOptions::new().read(true).write(true).open(path).map_err(StorageError::Io)?;
        let mut hdr = [0u8; 104];
        file.read_exact(&mut hdr).map_err(StorageError::Io)?;
        let old_l1_size = u32::from_be_bytes([hdr[36], hdr[37], hdr[38], hdr[39]]) as u64;
        let l2_entries_per_table = CLUSTER_SIZE / 8;
        let bytes_per_l1_entry = l2_entries_per_table * CLUSTER_SIZE;
        let new_l1_size = ((new_size_bytes + bytes_per_l1_entry - 1) / bytes_per_l1_entry).max(1);

        let old_l1_clusters = (old_l1_size * 8 + CLUSTER_SIZE - 1) / CLUSTER_SIZE;
        let new_l1_clusters = (new_l1_size * 8 + CLUSTER_SIZE - 1) / CLUSTER_SIZE;
        if new_l1_clusters > old_l1_clusters {
            return Err(StorageError::Other(
                "Growing QCOW2 beyond existing L1 table capacity requires qemu-img".to_string(),
            ));
        }

        file.seek(SeekFrom::Start(24)).map_err(StorageError::Io)?;
        file.write_all(&new_size_bytes.to_be_bytes()).map_err(StorageError::Io)?;
        file.seek(SeekFrom::Start(36)).map_err(StorageError::Io)?;
        file.write_all(&(new_l1_size as u32).to_be_bytes()).map_err(StorageError::Io)?;
        file.sync_all().map_err(StorageError::Io)?;

        info!("Successfully resized QCOW2 image via native header update at {:?}", path);
        Ok(())
    }

    /// Query metadata and inspect a QCOW2 image.
    /// Uses `qemu-img info --output=json` when installed, or parses the QCOW2 header natively.
    pub fn get_image_info(path: &Path) -> Result<Qcow2Info> {
        if !path.exists() {
            return Err(StorageError::NotFound(format!("File not found: {:?}", path)));
        }

        if let Ok(output) = Command::new("qemu-img")
            .arg("info")
            .arg("--output=json")
            .arg(path)
            .output()
        {
            if output.status.success() {
                let info: Qcow2Info = serde_json::from_slice(&output.stdout).map_err(|e| {
                    StorageError::Io(std::io::Error::new(
                        std::io::ErrorKind::InvalidData,
                        format!("Failed to parse qemu-img JSON output: {}", e),
                    ))
                })?;
                return Ok(info);
            }
        }

        Self::parse_qcow2_header(path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    #[test]
    fn test_qcow2_lifecycle() {
        let dir = tempdir().unwrap();
        let base_img = dir.path().join("base.qcow2");
        let overlay_img = dir.path().join("overlay.qcow2");

        // 1. Create base 100MB image
        Qcow2Manager::create_image(&base_img, 100).expect("create base image failed");
        assert!(base_img.exists());

        // 2. Query info
        let base_info = Qcow2Manager::get_image_info(&base_img).expect("get base info failed");
        assert_eq!(base_info.format, "qcow2");
        assert_eq!(base_info.virtual_size, 100 * 1024 * 1024);
        assert!(base_info.backing_filename.is_none());

        // 3. Create overlay backed by base
        Qcow2Manager::create_overlay(&overlay_img, &base_img).expect("create overlay failed");
        assert!(overlay_img.exists());

        let overlay_info = Qcow2Manager::get_image_info(&overlay_img).expect("get overlay info failed");
        assert_eq!(overlay_info.format, "qcow2");
        assert!(overlay_info.backing_filename.is_some());

        // 4. Resize overlay
        Qcow2Manager::resize_image(&overlay_img, 200).expect("resize overlay failed");
        let resized_info = Qcow2Manager::get_image_info(&overlay_img).expect("get resized info failed");
        assert_eq!(resized_info.virtual_size, 200 * 1024 * 1024);

        // 5. Overlay backed by raw ext4 image
        let raw_img = dir.path().join("rootfs.ext4");
        std::fs::write(&raw_img, vec![0u8; 4 * 1024 * 1024]).unwrap();
        let raw_overlay = dir.path().join("raw-overlay.qcow2");
        Qcow2Manager::create_overlay(&raw_overlay, &raw_img).expect("create raw overlay failed");
        let raw_info = Qcow2Manager::get_image_info(&raw_overlay).expect("get raw overlay info failed");
        assert_eq!(raw_info.format, "qcow2");
        assert_eq!(raw_info.virtual_size, 4 * 1024 * 1024);
        assert_eq!(raw_info.backing_filename_format.as_deref(), Some("raw"));
    }
}
