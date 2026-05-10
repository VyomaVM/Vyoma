#[cfg(test)]
mod tests {
    use std::path::Path;
    use tempfile::TempDir;
    use vyoma_core::initramfs;

    #[test]
    fn test_initramfs_roundtrip_extract() {
        let temp_dir = TempDir::new().unwrap();
        let initramfs_path = temp_dir.path().join("test.cpio.gz");

        let init_script = r#"#!/bin/sh
mount -t proc proc /proc 2>/dev/null || true
mount -t sysfs sys /sys 2>/dev/null || true
ip link set lo up 2>/dev/null || true
/sbin/vyoma-agent-vm &
exec /sbin/init
"#;

        let result = initramfs::create_initramfs(init_script, None, &initramfs_path);
        assert!(result.is_ok());
        assert!(initramfs_path.exists());

        let metadata = std::fs::metadata(&initramfs_path).unwrap();
        assert!(metadata.len() > 0, "Initramfs should not be empty");

        use flate2::read::GzDecoder;
        use std::io::Read;
        let file = std::fs::File::open(&initramfs_path).unwrap();
        let mut decoder = GzDecoder::new(file);
        let mut bytes = Vec::new();
        decoder.read_to_end(&mut bytes).unwrap();
        assert!(bytes.len() > 0, "Should be able to decompress initramfs");

        assert!(bytes.windows(6).any(|w| w == b"070701" || w == b"070702"), 
            "Should contain cpio newc magic bytes");
    }

    #[test]
    fn test_initramfs_with_agent_binary() {
        let temp_dir = TempDir::new().unwrap();
        let initramfs_path = temp_dir.path().join("with_agent.cpio.gz");

        let fake_agent = temp_dir.path().join("vyoma-agent-vm");
        std::fs::write(&fake_agent, b"\x7fELF\x01\x01\x01fake").unwrap();

        let init_script = "#!/bin/sh\n/sbin/vyoma-agent-vm\n";

        let result = initramfs::create_initramfs(
            init_script,
            Some(&fake_agent as &Path),
            &initramfs_path,
        );

        assert!(result.is_ok());
        assert!(initramfs_path.exists());

        let metadata = std::fs::metadata(&initramfs_path).unwrap();
        assert!(metadata.len() > 100, "Initramfs with agent should be larger");
    }

    #[test]
    fn test_initramfs_missing_agent_graceful() {
        let temp_dir = TempDir::new().unwrap();
        let initramfs_path = temp_dir.path().join("no_agent.cpio.gz");

        let nonexistent_agent = temp_dir.path().join("nonexistent_agent");
        let init_script = "#!/bin/sh\n";

        let result = initramfs::create_initramfs(
            init_script,
            Some(&nonexistent_agent as &Path),
            &initramfs_path,
        );

        assert!(result.is_ok(), "Should succeed even when agent doesn't exist");
        assert!(initramfs_path.exists(), "Initramfs should be created");
    }

    #[test]
    fn test_initramfs_contains_required_files() {
        let temp_dir = TempDir::new().unwrap();
        let initramfs_path = temp_dir.path().join("check_content.cpio.gz");

        let init_script = "#!/bin/sh\necho Hello";
        initramfs::create_initramfs(init_script, None, &initramfs_path).unwrap();

        use flate2::read::GzDecoder;
        use std::io::Read;

        let file = std::fs::File::open(&initramfs_path).unwrap();
        let mut decoder = GzDecoder::new(file);
        let mut decompressed = Vec::new();
        decoder.read_to_end(&mut decompressed).unwrap();

        assert!(decompressed.len() > 100, "Decompressed content should be substantial");

        let content = String::from_utf8_lossy(&decompressed);
        assert!(content.contains("070701") || content.contains("070702"), 
            "Should contain cpio newc magic bytes (070701 or 070702)");
        
        assert!(content.contains("init"), "Decompressed content should contain 'init'");
        assert!(content.contains("vyoma-init"), "Decompressed content should contain 'vyoma-init'");
    }
}