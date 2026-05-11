use std::collections::HashMap;
use std::path::PathBuf;
use vyoma_core::attest::{AttestationResponse, TpmQuote};
use vyoma_core::policy::{MeasuredBootPolicy, PolicyManager};
use vyoma_core::unified_attest::UnifiedAttestationManager;
use vyoma_image::signing::{SigningKeyPair, SignedManifest, TrustPolicy};
use vyoma_image::vmif::{VmifManifest, MeasuredBootInfo, OciImageConfig};

const STANDARD_PCRS: &[u32] = &[0, 1, 4, 5, 7, 9, 10, 14];

fn create_test_pcr_values() -> HashMap<u32, String> {
    let mut pcrs = HashMap::new();
    pcrs.insert(0, "0000000000000000000000000000000000000000".to_string());
    pcrs.insert(1, "1111111111111111111111111111111111111111".to_string());
    pcrs.insert(4, "4444444444444444444444444444444444444444".to_string());
    pcrs.insert(5, "5555555555555555555555555555555555555555".to_string());
    pcrs.insert(7, "7777777777777777777777777777777777777777".to_string());
    pcrs.insert(9, "9999999999999999999999999999999999999999".to_string());
    pcrs.insert(10, "aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa".to_string());
    pcrs.insert(14, "eeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeeee".to_string());
    pcrs
}

fn create_test_manifest() -> VmifManifest {
    let config = OciImageConfig {
        cmd: Some(vec!["/bin/sh".to_string()]),
        ..Default::default()
    };
    
    let mut manifest = VmifManifest::new(
        "amd64".to_string(),
        Some("kernel:v1".to_string()),
        None,
        "sha256:abcdef123456".to_string(),
        config,
        1024000,
    );
    
    manifest.measured_boot.pcr_policy = Some(create_test_pcr_values());
    manifest
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_pcr_policy_contains_all_standard_indices() {
        let pcrs = create_test_pcr_values();
        
        for pcr_index in STANDARD_PCRS {
            assert!(
                pcrs.contains_key(pcr_index),
                "PCR {} should be present in policy",
                pcr_index
            );
        }
        
        assert_eq!(pcrs.len(), STANDARD_PCRS.len());
    }

    #[test]
    fn test_unified_attestation_manager_verify_tpm_attestation_success() {
        let manager = UnifiedAttestationManager::new();
        let expected_pcrs = create_test_pcr_values();
        
        let quote = TpmQuote {
            quote: Vec::new(),
            signature: Vec::new(),
            pcr_values: expected_pcrs.clone(),
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        
        let response = AttestationResponse {
            vm_id: "test-vm".to_string(),
            verified: true,
            quote: Some(quote),
            pcr_results: HashMap::new(),
            error: None,
        };
        
        let result = manager.verify_tpm_attestation(&response, &expected_pcrs);
        assert!(result.is_ok(), "Attestation should succeed with matching PCRs");
        
        let verified_response = result.unwrap();
        assert!(verified_response.verified, "Response should indicate verification passed");
        
        for measurement in &verified_response.measurements {
            assert!(measurement.verified, "PCR {} should be verified", measurement.name);
        }
    }

    #[test]
    fn test_unified_attestation_manager_verify_tpm_attestation_pcr_mismatch() {
        let manager = UnifiedAttestationManager::new();
        let expected_pcrs = create_test_pcr_values();
        
        let mut tampered_pcrs = expected_pcrs.clone();
        tampered_pcrs.insert(9, "tampered_hash_tampered_hash_tampered_has".to_string());
        
        let quote = TpmQuote {
            quote: Vec::new(),
            signature: Vec::new(),
            pcr_values: tampered_pcrs,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        
        let response = AttestationResponse {
            vm_id: "test-vm".to_string(),
            verified: true,
            quote: Some(quote),
            pcr_results: HashMap::new(),
            error: None,
        };
        
        let result = manager.verify_tpm_attestation(&response, &expected_pcrs);
        
        if result.is_ok() {
            let verified_response = result.unwrap();
            assert!(!verified_response.verified, "Response should indicate verification failed");
        } else {
            assert!(result.is_err(), "Should return error on PCR mismatch");
        }
    }

    #[test]
    fn test_unified_attestation_manager_missing_pcr_in_live_quote() {
        let manager = UnifiedAttestationManager::new();
        let expected_pcrs = create_test_pcr_values();
        
        let mut incomplete_pcrs = expected_pcrs.clone();
        incomplete_pcrs.remove(&9);
        
        let quote = TpmQuote {
            quote: Vec::new(),
            signature: Vec::new(),
            pcr_values: incomplete_pcrs,
            timestamp: "2024-01-01T00:00:00Z".to_string(),
        };
        
        let response = AttestationResponse {
            vm_id: "test-vm".to_string(),
            verified: true,
            quote: Some(quote),
            pcr_results: HashMap::new(),
            error: None,
        };
        
        let result = manager.verify_tpm_attestation(&response, &expected_pcrs);
        
        let verified_response = result.unwrap();
        
        let has_missing = verified_response.measurements.iter().any(|m| m.value.is_empty());
        assert!(has_missing || !verified_response.verified, 
            "Should detect missing PCR or report verification failure");
    }

    #[test]
    fn test_signed_manifest_signing_and_verification() {
        let keypair = SigningKeyPair::generate();
        let manifest = create_test_manifest();
        
        let signed = keypair.sign_manifest(&manifest);
        assert!(signed.is_ok(), "Signing should succeed");
        
        let signed_manifest = signed.unwrap();
        assert!(!signed_manifest.signature.is_empty(), "Signature should not be empty");
        assert_eq!(signed_manifest.public_key.len(), 32, "Public key should be 32 bytes");
        
        let verification = keypair.verify_manifest(&signed_manifest);
        assert!(verification.is_ok(), "Verification should succeed with correct key");
    }

    #[test]
    fn test_signed_manifest_verification_fails_with_wrong_key() {
        let keypair1 = SigningKeyPair::generate();
        let keypair2 = SigningKeyPair::generate();
        let manifest = create_test_manifest();
        
        let signed = keypair1.sign_manifest(&manifest).unwrap();
        
        let verification = keypair2.verify_manifest(&signed);
        assert!(verification.is_err(), "Verification should fail with wrong key");
    }

    #[test]
    fn test_trust_policy_accepts_signed_manifest_with_trusted_key() {
        let keypair = SigningKeyPair::generate();
        let manifest = create_test_manifest();
        let signed = keypair.sign_manifest(&manifest).unwrap();
        
        let mut policy = TrustPolicy::new(true);
        policy.add_trusted_key(keypair.public_key_bytes());
        
        let result = policy.verify(&signed);
        assert!(result.is_ok(), "Trust policy should accept manifest with trusted key");
    }

    #[test]
    fn test_trust_policy_rejects_unsigned_manifest_when_required() {
        let mut policy = TrustPolicy::new(true);
        policy.add_trusted_key(vec![0; 32]);
        
        let unsigned_manifest = create_test_manifest();
        
        let signed = SignedManifest {
            manifest: unsigned_manifest,
            signature: Vec::new(),
            public_key: Vec::new(),
        };
        
        let result = policy.verify(&signed);
        assert!(result.is_err(), "Trust policy should reject unsigned manifest when required");
    }

    #[test]
    fn test_measured_boot_policy_configuration() {
        let mut policy = MeasuredBootPolicy::default();
        
        assert!(!policy.enabled, "Policy should be disabled by default");
        assert!(!policy.required, "Policy should not be required by default");
        
        policy.enabled = true;
        policy.required = true;
        policy.verification_timeout_secs = 60;
        policy.block_on_failure = true;
        
        assert!(policy.enabled, "Policy should be enabled");
        assert!(policy.required, "Policy should be required");
        assert_eq!(policy.verification_timeout_secs, 60, "Timeout should be 60 seconds");
        assert!(policy.block_on_failure, "Should block on failure");
    }

    #[test]
    fn test_policy_manager_must_verify_on_boot() {
        let mut manager = PolicyManager::new();
        
        assert!(!manager.should_verify_on_boot(), "Should not verify when disabled");
        assert!(!manager.must_verify_on_boot(), "Should not require verification when disabled");
        
        manager.set_require_measured_boot(true);
        
        assert!(manager.should_verify_on_boot(), "Should verify when enabled");
        assert!(manager.must_verify_on_boot(), "Should require verification when required");
    }

    #[test]
    fn test_attestation_response_with_pcr_results() {
        let mut pcr_results = HashMap::new();
        pcr_results.insert(0u32, true);
        pcr_results.insert(9, true);
        pcr_results.insert(14, false);
        
        let response = AttestationResponse {
            vm_id: "test-vm".to_string(),
            verified: false,
            quote: None,
            pcr_results,
            error: Some("PCR 14 mismatch".to_string()),
        };
        
        assert!(!response.verified);
        assert!(response.error.is_some());
        assert_eq!(response.error.unwrap(), "PCR 14 mismatch");
        assert_eq!(response.pcr_results.get(&14), Some(&false));
    }

    #[test]
    fn test_measured_boot_info_pcr_policy_storage() {
        let mut boot_info = MeasuredBootInfo::default();
        
        let pcrs = create_test_pcr_values();
        boot_info.pcr_policy = Some(pcrs.clone());
        
        assert!(boot_info.pcr_policy.is_some());
        assert_eq!(boot_info.pcr_policy.as_ref().unwrap().len(), 8);
        assert_eq!(
            boot_info.pcr_policy.as_ref().unwrap().get(&9),
            Some(&"9999999999999999999999999999999999999999".to_string())
        );
    }

    #[test]
    fn test_pcr_value_hex_format() {
        let pcrs = create_test_pcr_values();
        
        for (index, hash) in &pcrs {
            assert_eq!(hash.len(), 40, "PCR {} should be 40 hex chars (SHA-1)", index);
            assert!(
                hash.chars().all(|c| c.is_ascii_hexdigit()),
                "PCR {} should contain only hex characters",
                index
            );
        }
    }

    #[test]
    fn test_manifest_without_pcr_policy_is_unsigned() {
        let config = OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );
        
        assert!(
            manifest.measured_boot.pcr_policy.is_none(),
            "Manifest without --measured should have no PCR policy"
        );
    }

    #[test]
    fn test_manifest_with_pcr_policy_is_ready_for_attestation() {
        let manifest = create_test_manifest();
        
        assert!(
            manifest.measured_boot.pcr_policy.is_some(),
            "Manifest built with --measured should have PCR policy"
        );
        
        let pcr_policy = manifest.measured_boot.pcr_policy.unwrap();
        assert!(!pcr_policy.is_empty(), "PCR policy should not be empty");
        
        for pcr_index in STANDARD_PCRS {
            assert!(
                pcr_policy.contains_key(pcr_index),
                "Standard PCR {} should be in policy",
                pcr_index
            );
        }
    }

    #[test]
    fn test_signed_manifest_serialization_roundtrip() {
        let keypair = SigningKeyPair::generate();
        let manifest = create_test_manifest();
        let signed = keypair.sign_manifest(&manifest).unwrap();
        
        let bytes = signed.to_bytes();
        assert!(bytes.is_ok(), "Serialization should succeed");
        
        let loaded = SignedManifest::from_bytes(&bytes.unwrap());
        assert!(loaded.is_ok(), "Deserialization should succeed");
        
        let deserialized = loaded.unwrap();
        assert_eq!(deserialized.manifest, manifest, "Manifest should match");
        assert_eq!(deserialized.signature, signed.signature, "Signature should match");
        assert_eq!(deserialized.public_key, signed.public_key, "Public key should match");
    }

    #[test]
    fn test_policy_config_pcr_selection() {
        let config = MeasuredBootPolicy::default();
        
        assert_eq!(
            config.pcr_selection,
            vec![7, 9, 10],
            "Default PCR selection should be [7, 9, 10]"
        );
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;

    const TEST_VM_ID: &str = "test-vm-12345";

    fn create_test_image_dir() -> PathBuf {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let image_dir = home.join(".vyoma").join("images").join("test_alpine_latest");
        if let Err(e) = std::fs::create_dir_all(&image_dir) {
            eprintln!("Warning: Could not create test image dir: {}", e);
        }
        image_dir
    }

    fn cleanup_test_image_dir() {
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("/tmp"));
        let image_dir = home.join(".vyoma").join("images").join("test_alpine_latest");
        let _ = std::fs::remove_dir_all(&image_dir);
    }

    #[test]
    fn test_tamper_detection_flow() {
        let keypair = SigningKeyPair::generate();
        let manifest = create_test_manifest();
        let signed = keypair.sign_manifest(&manifest).unwrap();
        
        let image_dir = create_test_image_dir();
        let sig_path = image_dir.join("vyoma.toml.sig");
        signed.save_to_file(&sig_path).unwrap();
        
        let expected_pcrs = manifest.measured_boot.pcr_policy.unwrap();
        
        let mut tampered_pcrs = expected_pcrs.clone();
        tampered_pcrs.insert(14, "tampered_value_tampered_value_tampered_va".to_string());
        
        let manager = UnifiedAttestationManager::new();
        
        let quote = TpmQuote {
            quote: Vec::new(),
            signature: Vec::new(),
            pcr_values: tampered_pcrs,
            timestamp: chrono::Utc::now().to_rfc3339(),
        };
        
        let response = AttestationResponse {
            vm_id: TEST_VM_ID.to_string(),
            verified: true,
            quote: Some(quote),
            pcr_results: HashMap::new(),
            error: None,
        };
        
        let result = manager.verify_tpm_attestation(&response, &expected_pcrs);
        
        if result.is_ok() {
            let verified = result.unwrap();
            assert!(!verified.verified, "Attestation should fail after tampering");
            
            let failed: Vec<_> = verified.measurements
                .iter()
                .filter(|m| !m.verified)
                .collect();
            
            assert!(!failed.is_empty(), "At least one PCR should fail verification");
            assert!(
                failed.iter().any(|m| m.name.contains("14")),
                "PCR 14 should be among failed measurements"
            );
        } else {
            assert!(result.is_err(), "Should return error on tampered PCR");
        }
        
        let _ = std::fs::remove_dir_all(&image_dir);
    }

    #[test]
    fn test_unsigned_image_rejection() {
        let image_dir = create_test_image_dir();
        let manifest_path = image_dir.join("vyoma.toml");
        
        let config = OciImageConfig::default();
        let unsigned_manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:unsigned".to_string(),
            config,
            1024000,
        );
        
        let content = serde_json::to_string_pretty(&unsigned_manifest).unwrap();
        
        if let Err(e) = std::fs::write(&manifest_path, &content) {
            if e.kind() != std::io::ErrorKind::NotFound {
                panic!("Failed to write manifest: {}", e);
            }
            std::fs::create_dir_all(&image_dir).ok();
            std::fs::write(&manifest_path, &content).unwrap();
        }
        
        assert!(
            unsigned_manifest.measured_boot.pcr_policy.is_none(),
            "Unsigned manifest should have no PCR policy"
        );
        
        let sig_path = image_dir.join("vyoma.toml.sig");
        assert!(
            !sig_path.exists(),
            "Signed manifest should not exist for unsigned image"
        );
        
        let _ = std::fs::remove_dir_all(&image_dir);
    }

    #[test]
    fn test_attestation_timeout_configuration() {
        let policy = MeasuredBootPolicy::default();
        
        assert_eq!(policy.verification_timeout_secs, 30, "Default timeout should be 30 seconds");
        
        let mut custom_policy = MeasuredBootPolicy::default();
        custom_policy.verification_timeout_secs = 120;
        
        assert_eq!(custom_policy.verification_timeout_secs, 120, "Custom timeout should be 120 seconds");
    }

    #[test]
    fn test_block_on_failure_policy() {
        let mut policy = MeasuredBootPolicy::default();
        
        assert!(policy.block_on_failure, "Block on failure should be true by default");
        
        policy.block_on_failure = false;
        assert!(!policy.block_on_failure, "Block on failure can be disabled");
    }

    #[test]
    fn test_trust_policy_requires_signed_manifest() {
        let image_dir = create_test_image_dir();
        
        let mut policy = TrustPolicy::new(true);
        policy.add_trusted_key(vec![0; 32]);
        
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:unsigned".to_string(),
            OciImageConfig::default(),
            1024000,
        );
        
        let unsigned = SignedManifest {
            manifest,
            signature: Vec::new(),
            public_key: Vec::new(),
        };
        
        let sig_path = image_dir.join("vyoma.toml.sig");
        assert!(!sig_path.exists(), "No signed manifest should exist");
        
        let result = policy.verify(&unsigned);
        assert!(result.is_err(), "Should reject unsigned manifest when required");
        
        let _ = std::fs::remove_dir_all(&image_dir);
    }

    #[test]
    fn test_pcr_policy_keys_match_expected() {
        let pcrs = create_test_pcr_values();
        
        assert_eq!(pcrs.len(), 8, "Should have 8 PCRs");
        assert!(pcrs.contains_key(&0), "Should have PCR 0 (firmware)");
        assert!(pcrs.contains_key(&7), "Should have PCR 7 (secure boot state)");
        assert!(pcrs.contains_key(&9), "Should have PCR 9 (kernel)");
        assert!(pcrs.contains_key(&10), "Should have PCR 10 (initrd)");
        assert!(pcrs.contains_key(&14), "Should have PCR 14 (rootfs)");
    }
}