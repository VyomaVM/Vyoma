//! dm-verity Merkle Tree Root Hash Generator and Verifier.
//!
//! Implements the Linux kernel dm-verity block-integrity Merkle tree calculation,
//! generating cryptographic root hashes for read-only Squashfs rootfs images.
//! This ensures block-level tamper-proofing and runtime integrity enforcement.

use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::fs::File;
use std::io::Read;
use std::path::Path;
use thiserror::Error;

pub const DEFAULT_VERITY_BLOCK_SIZE: u32 = 4096;
pub const HASH_SIZE: usize = 32; // SHA-256 digest size

#[derive(Debug, Error)]
pub enum VerityError {
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
    #[error("Invalid block size: {0} (must be a power of 2, minimum 512)")]
    InvalidBlockSize(u32),
    #[error("Hex decoding error: {0}")]
    Hex(#[from] hex::FromHexError),
    #[error("Root hash mismatch: expected {expected}, calculated {calculated}")]
    RootHashMismatch {
        expected: String,
        calculated: String,
    },
}

/// Metadata describing a dm-verity integrity tree and its root hash.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VeritySpec {
    pub root_hash: String,
    pub algorithm: String,
    pub data_blocks: u64,
    pub hash_blocks: u64,
    pub block_size: u32,
    pub salt: String,
}

/// Result of generating a verity tree, including the metadata spec and serialized tree bytes.
pub struct VerityTreeResult {
    pub spec: VeritySpec,
    pub tree_bytes: Vec<u8>,
}

/// Computes the dm-verity Merkle tree and root hash for in-memory byte slice.
pub fn generate_verity_from_bytes(
    data: &[u8],
    salt: Option<&[u8]>,
    block_size: u32,
) -> Result<VerityTreeResult, VerityError> {
    validate_block_size(block_size)?;

    let bs = block_size as usize;
    let salt_bytes = salt.unwrap_or(&[]);
    let salt_hex = hex::encode(salt_bytes);

    let data_blocks = if data.is_empty() {
        1
    } else {
        (data.len() + bs - 1) / bs
    };

    // Level 0: Hash each data block
    let mut current_hashes = Vec::with_capacity(data_blocks);
    for i in 0..data_blocks {
        let start = i * bs;
        let mut block = vec![0u8; bs];
        if start < data.len() {
            let end = (start + bs).min(data.len());
            block[..(end - start)].copy_from_slice(&data[start..end]);
        }

        let mut hasher = Sha256::new();
        if !salt_bytes.is_empty() {
            hasher.update(salt_bytes);
        }
        hasher.update(&block);
        let hash: [u8; 32] = hasher.finalize().into();
        current_hashes.push(hash);
    }

    // Build the Merkle tree levels until a single root hash remains
    let mut tree_bytes = Vec::new();
    let hashes_per_block = bs / HASH_SIZE;

    while current_hashes.len() > 1 {
        let num_hash_blocks = (current_hashes.len() + hashes_per_block - 1) / hashes_per_block;
        let mut next_level_hashes = Vec::with_capacity(num_hash_blocks);

        for block_idx in 0..num_hash_blocks {
            let mut hash_block = vec![0u8; bs];
            let start_h = block_idx * hashes_per_block;
            let end_h = (start_h + hashes_per_block).min(current_hashes.len());

            for (h_offset, h) in current_hashes[start_h..end_h].iter().enumerate() {
                let dest = h_offset * HASH_SIZE;
                hash_block[dest..dest + HASH_SIZE].copy_from_slice(h);
            }

            // Save hash block to the output tree device stream
            tree_bytes.extend_from_slice(&hash_block);

            // Compute parent hash for the next level
            let mut hasher = Sha256::new();
            if !salt_bytes.is_empty() {
                hasher.update(salt_bytes);
            }
            hasher.update(&hash_block);
            let parent_hash: [u8; 32] = hasher.finalize().into();
            next_level_hashes.push(parent_hash);
        }

        current_hashes = next_level_hashes;
    }

    let root_hash = hex::encode(current_hashes[0]);
    let hash_blocks = if tree_bytes.is_empty() {
        0
    } else {
        (tree_bytes.len() / bs) as u64
    };

    Ok(VerityTreeResult {
        spec: VeritySpec {
            root_hash,
            algorithm: "sha256".to_string(),
            data_blocks: data_blocks as u64,
            hash_blocks,
            block_size,
            salt: salt_hex,
        },
        tree_bytes,
    })
}

/// Computes the dm-verity Merkle tree and root hash directly from a file stream.
pub fn generate_verity_from_file(
    path: &Path,
    salt: Option<&[u8]>,
    block_size: u32,
) -> Result<VerityTreeResult, VerityError> {
    validate_block_size(block_size)?;

    let mut file = File::open(path)?;
    let file_len = file.metadata()?.len();
    let bs = block_size as usize;
    let salt_bytes = salt.unwrap_or(&[]);

    let data_blocks = if file_len == 0 {
        1
    } else {
        ((file_len + block_size as u64 - 1) / block_size as u64) as usize
    };

    let mut current_hashes = Vec::with_capacity(data_blocks);
    let mut buffer = vec![0u8; bs];

    for _ in 0..data_blocks {
        let bytes_read = file.read(&mut buffer)?;
        if bytes_read < bs {
            buffer[bytes_read..].fill(0);
        }

        let mut hasher = Sha256::new();
        if !salt_bytes.is_empty() {
            hasher.update(salt_bytes);
        }
        hasher.update(&buffer);
        let hash: [u8; 32] = hasher.finalize().into();
        current_hashes.push(hash);
    }

    // Build upper levels in memory
    let hashes_per_block = bs / HASH_SIZE;
    let mut tree_bytes = Vec::new();

    while current_hashes.len() > 1 {
        let num_hash_blocks = (current_hashes.len() + hashes_per_block - 1) / hashes_per_block;
        let mut next_level_hashes = Vec::with_capacity(num_hash_blocks);

        for block_idx in 0..num_hash_blocks {
            let mut hash_block = vec![0u8; bs];
            let start_h = block_idx * hashes_per_block;
            let end_h = (start_h + hashes_per_block).min(current_hashes.len());

            for (h_offset, h) in current_hashes[start_h..end_h].iter().enumerate() {
                let dest = h_offset * HASH_SIZE;
                hash_block[dest..dest + HASH_SIZE].copy_from_slice(h);
            }

            tree_bytes.extend_from_slice(&hash_block);

            let mut hasher = Sha256::new();
            if !salt_bytes.is_empty() {
                hasher.update(salt_bytes);
            }
            hasher.update(&hash_block);
            let parent_hash: [u8; 32] = hasher.finalize().into();
            next_level_hashes.push(parent_hash);
        }

        current_hashes = next_level_hashes;
    }

    let root_hash = hex::encode(current_hashes[0]);
    let hash_blocks = (tree_bytes.len() / bs) as u64;

    Ok(VerityTreeResult {
        spec: VeritySpec {
            root_hash,
            algorithm: "sha256".to_string(),
            data_blocks: data_blocks as u64,
            hash_blocks,
            block_size,
            salt: hex::encode(salt_bytes),
        },
        tree_bytes,
    })
}

/// Verifies that data in a file matches the provided `VeritySpec` root hash.
pub fn verify_verity_file(path: &Path, spec: &VeritySpec) -> Result<bool, VerityError> {
    let salt = hex::decode(&spec.salt)?;
    let result = generate_verity_from_file(path, Some(&salt), spec.block_size)?;

    if result.spec.root_hash.eq_ignore_ascii_case(&spec.root_hash) {
        Ok(true)
    } else {
        Err(VerityError::RootHashMismatch {
            expected: spec.root_hash.clone(),
            calculated: result.spec.root_hash,
        })
    }
}

fn validate_block_size(block_size: u32) -> Result<(), VerityError> {
    if block_size < 512 || !block_size.is_power_of_two() {
        return Err(VerityError::InvalidBlockSize(block_size));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::{Seek, SeekFrom, Write};
    use tempfile::NamedTempFile;

    #[test]
    fn test_verity_single_block() {
        let data = b"Hello, dm-verity on Vyoma microVM!";
        let res = generate_verity_from_bytes(data, None, 4096).unwrap();

        assert_eq!(res.spec.data_blocks, 1);
        assert_eq!(res.spec.hash_blocks, 0);
        assert_eq!(res.spec.block_size, 4096);
        assert_eq!(res.spec.root_hash.len(), 64);
        assert!(res.tree_bytes.is_empty()); // Single block needs no intermediate tree device
    }

    #[test]
    fn test_verity_multi_block_and_tamper_detection() {
        // Create 2 blocks of data (8192 bytes)
        let mut data = vec![0x41u8; 8192];
        let res = generate_verity_from_bytes(&data, None, 4096).unwrap();

        assert_eq!(res.spec.data_blocks, 2);
        assert_eq!(res.spec.hash_blocks, 1);
        let orig_root = res.spec.root_hash.clone();

        // Tamper with one byte in block 1
        data[5000] = 0x42;
        let tampered_res = generate_verity_from_bytes(&data, None, 4096).unwrap();
        assert_ne!(orig_root, tampered_res.spec.root_hash);
    }

    #[test]
    fn test_verity_file_verification_roundtrip() {
        let mut tmp = NamedTempFile::new().unwrap();
        // Write 16 KiB of reproducible data
        let content = vec![0x55u8; 16384];
        tmp.write_all(&content).unwrap();
        tmp.flush().unwrap();

        let salt = b"vyoma-salt-123";
        let res = generate_verity_from_file(tmp.path(), Some(salt), 4096).unwrap();

        assert_eq!(res.spec.data_blocks, 4);
        assert!(verify_verity_file(tmp.path(), &res.spec).unwrap());

        // Tamper with the file
        tmp.seek(SeekFrom::Start(100)).unwrap();
        tmp.write_all(b"corrupted").unwrap();
        tmp.flush().unwrap();

        assert!(verify_verity_file(tmp.path(), &res.spec).is_err());
    }

    #[test]
    fn test_verity_multi_level_tree() {
        // Each block holds 4096 / 32 = 128 hashes.
        // If data has 200 blocks (800 KiB), level 0 needs 200 hashes -> 2 hash blocks.
        // Level 1 hashes those 2 blocks -> 1 root hash!
        let data = vec![0x77u8; 200 * 4096];
        let res = generate_verity_from_bytes(&data, None, 4096).unwrap();

        assert_eq!(res.spec.data_blocks, 200);
        // Level 1 produces 2 hash blocks, Level 2 produces 1 hash block = 3 total hash blocks
        assert_eq!(res.spec.hash_blocks, 3);
        assert_eq!(res.spec.root_hash.len(), 64);
    }
}
