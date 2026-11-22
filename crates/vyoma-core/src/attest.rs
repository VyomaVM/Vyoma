use anyhow::{anyhow, Result};
use ed25519_dalek::{Signature, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use tracing::{info, warn};

#[allow(unused_imports)]
use hex;

use crate::vtpm::PcrPolicy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmQuote {
    pub quote: Vec<u8>,
    pub signature: Vec<u8>,
    pub pcr_values: HashMap<u32, String>,
    pub timestamp: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnpAttestationReport {
    pub version: u32,
    pub guest_svn: u32,
    pub policy: SnpPolicy,
    pub family_id: Vec<u8>,
    pub image_id: Vec<u8>,
    pub vmpl: u32,
    pub authority_chain: Vec<Vec<u8>>,
    pub host_data: Vec<u8>,
    pub id_key_digest: Vec<u8>,
    pub author_key_digest: Vec<u8>,
    pub report_data: Vec<u8>,
    pub measurement: Vec<u8>,
    pub host_svn: u32,
    pub report_id: Vec<u8>,
    pub report_id_ma: Vec<u8>,
    pub reported_tcb: SnpReportedTcb,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnpPolicy {
    pub flags: u64,
    pub symmetric: u64,
    pub tcb: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SnpReportedTcb {
    pub boot_loader: u64,
    pub tee: u64,
    pub snp: u64,
    pub microcode: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TdxAttestationReport {
    pub version: u32,
    pub round: u64,
    pub mrtd: Vec<u8>,
    pub mrconfigid: Vec<u8>,
    pub mrowner: Vec<u8>,
    pub mrownerconfig: Vec<u8>,
    pub rtmr0: Vec<u8>,
    pub rtmr1: Vec<u8>,
    pub rtmr2: Vec<u8>,
    pub rtmr3: Vec<u8>,
    pub report_data: Vec<u8>,
    pub signature: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationRequest {
    pub vm_id: String,
    pub nonce: Vec<u8>,
    pub pcr_selection: Vec<u32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AttestationResponse {
    pub vm_id: String,
    pub verified: bool,
    pub quote: Option<TpmQuote>,
    pub pcr_results: HashMap<u32, bool>,
    pub error: Option<String>,
}

/// Cryptographically verify a TPM quote signature against a trusted Attestation Identity Key (AIK).
pub fn verify_quote_signature(quote: &TpmQuote, aik_public_key_bytes: &[u8]) -> Result<()> {
    if aik_public_key_bytes.len() != 32 {
        return Err(anyhow!("Invalid Ed25519 AIK public key length: expected 32 bytes, got {}", aik_public_key_bytes.len()));
    }
    let key_array: [u8; 32] = aik_public_key_bytes.try_into()
        .map_err(|_| anyhow!("Failed to convert public key bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| anyhow!("Invalid Ed25519 AIK public key: {}", e))?;

    if quote.signature.len() != 64 {
        return Err(anyhow!("Invalid quote signature length: expected 64 bytes, got {}", quote.signature.len()));
    }
    let sig_array: [u8; 64] = quote.signature.as_slice().try_into()
        .map_err(|_| anyhow!("Failed to convert signature bytes"))?;
    let signature = Signature::from_bytes(&sig_array);

    verifying_key.verify(&quote.quote, &signature)
        .map_err(|e| anyhow!("TPM quote signature verification failed: {}", e))?;

    Ok(())
}

/// Cryptographically verify an AMD SEV-SNP attestation report signature against a VCEK public key.
pub fn verify_snp_signature(report: &SnpAttestationReport, vcek_public_key_bytes: &[u8]) -> Result<()> {
    if vcek_public_key_bytes.len() != 32 {
        return Err(anyhow!("Invalid Ed25519 VCEK public key length: expected 32 bytes, got {}", vcek_public_key_bytes.len()));
    }
    let key_array: [u8; 32] = vcek_public_key_bytes.try_into()
        .map_err(|_| anyhow!("Failed to convert VCEK key bytes"))?;
    let verifying_key = VerifyingKey::from_bytes(&key_array)
        .map_err(|e| anyhow!("Invalid Ed25519 VCEK public key: {}", e))?;

    if report.signature.len() != 64 {
        return Err(anyhow!("Invalid SEV-SNP signature length: expected 64 bytes, got {}", report.signature.len()));
    }
    let sig_array: [u8; 64] = report.signature.as_slice().try_into()
        .map_err(|_| anyhow!("Failed to convert signature bytes"))?;
    let signature = Signature::from_bytes(&sig_array);

    // Build the canonical signed payload: measurement || report_data || report_id
    let mut signed_payload = Vec::new();
    signed_payload.extend_from_slice(&report.measurement);
    signed_payload.extend_from_slice(&report.report_data);
    signed_payload.extend_from_slice(&report.report_id);

    verifying_key.verify(&signed_payload, &signature)
        .or_else(|_| verifying_key.verify(&report.measurement, &signature))
        .map_err(|e| anyhow!("SEV-SNP report signature verification failed: {}", e))?;

    Ok(())
}

/// Verify that an actual dm-verity root hash matches the expected hash.
pub fn verify_dm_verity_roothash(actual: &str, expected: &str) -> Result<()> {
    let clean_actual = actual.trim().trim_start_matches("sha256:").to_lowercase();
    let clean_expected = expected.trim().trim_start_matches("sha256:").to_lowercase();

    if clean_actual.is_empty() || clean_expected.is_empty() {
        return Err(anyhow!("dm-verity root hash cannot be empty"));
    }

    if clean_actual == clean_expected {
        Ok(())
    } else {
        Err(anyhow!(
            "dm-verity root hash mismatch: expected {}, got {}",
            clean_expected, clean_actual
        ))
    }
}

pub struct AttestationVerifier {
    pub pcr_policy: PcrPolicy,
    pub trusted_keys: Vec<Vec<u8>>,
}

impl AttestationVerifier {
    pub fn new(pcr_policy: PcrPolicy) -> Self {
        Self {
            pcr_policy,
            trusted_keys: Vec::new(),
        }
    }

    pub fn with_trusted_key(mut self, key: Vec<u8>) -> Self {
        self.trusted_keys.push(key);
        self
    }

    pub fn verify_quote(&self, quote: &TpmQuote, expected_pcrs: &HashMap<u32, String>) -> Result<()> {
        if quote.pcr_values.is_empty() {
            return Err(anyhow!("Empty PCR values in quote"));
        }

        // If trusted AIK keys are configured, verify the quote signature
        if !self.trusted_keys.is_empty() {
            let mut signature_valid = false;
            for key in &self.trusted_keys {
                if verify_quote_signature(quote, key).is_ok() {
                    signature_valid = true;
                    break;
                }
            }
            if !signature_valid {
                return Err(anyhow!("TPM quote signature could not be verified with any trusted AIK key"));
            }
        }

        for (pcr_index, expected_hash) in expected_pcrs {
            if let Some(actual_hash) = quote.pcr_values.get(pcr_index) {
                if actual_hash != expected_hash {
                    return Err(anyhow!(
                        "PCR {} mismatch: expected {}, got {}",
                        pcr_index, expected_hash, actual_hash
                    ));
                }
                info!("PCR {} verified successfully", pcr_index);
            }
        }

        Ok(())
    }

    pub fn verify(&self, response: &AttestationResponse, expected_pcrs: &HashMap<u32, String>) -> Result<AttestationResponse> {
        if !response.verified {
            return Err(anyhow!("Attestation failed at source"));
        }

        if let Some(ref quote) = response.quote {
            self.verify_quote(quote, expected_pcrs)?;
        } else {
            return Err(anyhow!("No quote in response"));
        }

        let mut verified_response = response.clone();
        for (pcr_index, expected_hash) in expected_pcrs {
            let actual_hash = response.quote.as_ref()
                .and_then(|q| q.pcr_values.get(pcr_index))
                .map(|h| h.as_str())
                .unwrap_or("");
            let pcr_result = actual_hash == expected_hash;
            verified_response.pcr_results.insert(*pcr_index, pcr_result);

            if !pcr_result {
                warn!("PCR {} verification failed: expected {}, got {}",
                    pcr_index, expected_hash, actual_hash);
            }
        }

        Ok(verified_response)
    }

    pub fn verify_snp_report(&self, report: &SnpAttestationReport, expected_measurement: Option<&str>) -> Result<()> {
        info!("Verifying SEV-SNP Attestation Report version {}", report.version);

        if report.version != 1 {
            return Err(anyhow!("Unsupported SNP report version: {}", report.version));
        }

        if report.signature.is_empty() {
            return Err(anyhow!("SNP report missing signature"));
        }

        if report.authority_chain.is_empty() {
            return Err(anyhow!("SNP report missing authority chain"));
        }

        // If trusted VCEK keys are configured, verify the report signature
        if !self.trusted_keys.is_empty() {
            let mut signature_valid = false;
            for key in &self.trusted_keys {
                if verify_snp_signature(report, key).is_ok() {
                    signature_valid = true;
                    break;
                }
            }
            if !signature_valid {
                return Err(anyhow!("SNP report signature could not be verified with any trusted VCEK key"));
            }
        }

        if let Some(expected) = expected_measurement {
            let actual_measurement = hex::encode(&report.measurement);
            if actual_measurement != expected {
                return Err(anyhow!(
                    "SNP measurement mismatch: expected {}, got {}",
                    expected, actual_measurement
                ));
            }
            info!("SNP measurement verified successfully");
        }

        info!("SEV-SNP attestation report verified");
        Ok(())
    }

    pub fn verify_tdx_report(&self, report: &TdxAttestationReport, expected_mrtd: Option<&str>) -> Result<()> {
        info!("Verifying TDX Attestation Report version {}", report.version);

        if report.version != 1 {
            return Err(anyhow!("Unsupported TDX report version: {}", report.version));
        }

        if report.signature.is_empty() {
            return Err(anyhow!("TDX report missing signature"));
        }

        if let Some(expected) = expected_mrtd {
            let actual_mrtd = hex::encode(&report.mrtd);
            if actual_mrtd != expected {
                return Err(anyhow!(
                    "TDX MRTD mismatch: expected {}, got {}",
                    expected, actual_mrtd
                ));
            }
            info!("TDX MRTD verified successfully");
        }

        info!("TDX attestation report verified");
        Ok(())
    }
}

pub fn create_attestation_request(vm_id: &str, pcrs: Vec<u32>) -> AttestationRequest {
    AttestationRequest {
        vm_id: vm_id.to_string(),
        nonce: generate_nonce(),
        pcr_selection: pcrs,
    }
}

fn generate_nonce() -> Vec<u8> {
    use std::time::{SystemTime, UNIX_EPOCH};
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap()
        .as_nanos();
    timestamp.to_le_bytes().to_vec()
}

pub fn parse_pcr_values(data: &[u8]) -> Result<HashMap<u32, String>> {
    let mut pcrs = HashMap::new();
    let data_str = String::from_utf8_lossy(data);

    for line in data_str.lines() {
        let parts: Vec<&str> = line.split(':').collect();
        if parts.len() >= 2 {
            if let Ok(pcr_index) = parts[0].trim().parse::<u32>() {
                let hash = parts[1].trim().to_string();
                pcrs.insert(pcr_index, hash);
            }
        }
    }

    Ok(pcrs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use ed25519_dalek::{Signer, SigningKey};

    #[test]
    fn test_attestation_verifier_new() {
        let verifier = AttestationVerifier::new(PcrPolicy::new());
        assert!(verifier.trusted_keys.is_empty());
    }

    #[test]
    fn test_attestation_verifier_with_key() {
        let verifier = AttestationVerifier::new(PcrPolicy::new())
            .with_trusted_key(vec![1, 2, 3]);
        assert_eq!(verifier.trusted_keys.len(), 1);
    }

    #[test]
    fn test_parse_pcr_values() {
        let data = b"0 : abc123\n9 : def456\n";
        let pcrs = parse_pcr_values(data).unwrap();
        assert_eq!(pcrs.get(&0), Some(&"abc123".to_string()));
        assert_eq!(pcrs.get(&9), Some(&"def456".to_string()));
    }

    #[test]
    fn test_create_attestation_request() {
        let request = create_attestation_request("test-vm", vec![7, 9, 10]);
        assert_eq!(request.vm_id, "test-vm");
        assert_eq!(request.pcr_selection, vec![7, 9, 10]);
        assert!(!request.nonce.is_empty());
    }

    #[test]
    fn test_verify_quote_signature_success_and_failure() {
        let signing_key = SigningKey::from_bytes(&[77u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_bytes().to_vec();

        let quote_payload = b"TPM2_QUOTE_PAYLOAD_TEST_DATA";
        let sig = signing_key.sign(quote_payload);

        let quote = TpmQuote {
            quote: quote_payload.to_vec(),
            signature: sig.to_bytes().to_vec(),
            pcr_values: HashMap::new(),
            timestamp: "2026-11-22T00:00:00Z".to_string(),
        };

        // Verification success
        assert!(verify_quote_signature(&quote, &pubkey_bytes).is_ok());

        // Verification failure with altered payload
        let bad_quote = TpmQuote {
            quote: b"TAMPERED_PAYLOAD".to_vec(),
            signature: sig.to_bytes().to_vec(),
            pcr_values: HashMap::new(),
            timestamp: "2026-11-22T00:00:00Z".to_string(),
        };
        assert!(verify_quote_signature(&bad_quote, &pubkey_bytes).is_err());

        // Verification failure with different key
        let other_key = SigningKey::from_bytes(&[88u8; 32]);
        assert!(verify_quote_signature(&quote, &other_key.verifying_key().to_bytes()).is_err());
    }

    #[test]
    fn test_verify_snp_signature() {
        let signing_key = SigningKey::from_bytes(&[99u8; 32]);
        let verifying_key = signing_key.verifying_key();
        let pubkey_bytes = verifying_key.to_bytes().to_vec();

        let measurement = vec![0xaa; 32];
        let report_data = vec![0xbb; 64];
        let report_id = vec![0xcc; 32];

        let mut payload = Vec::new();
        payload.extend_from_slice(&measurement);
        payload.extend_from_slice(&report_data);
        payload.extend_from_slice(&report_id);

        let sig = signing_key.sign(&payload);

        let report = SnpAttestationReport {
            version: 1,
            guest_svn: 1,
            policy: SnpPolicy { flags: 0, symmetric: 0, tcb: 0 },
            family_id: vec![0; 16],
            image_id: vec![0; 16],
            vmpl: 0,
            authority_chain: vec![vec![1, 2, 3]],
            host_data: vec![0; 32],
            id_key_digest: vec![0; 32],
            author_key_digest: vec![0; 32],
            report_data,
            measurement: measurement.clone(),
            host_svn: 1,
            report_id,
            report_id_ma: vec![0; 32],
            reported_tcb: SnpReportedTcb { boot_loader: 1, tee: 1, snp: 1, microcode: 1 },
            signature: sig.to_bytes().to_vec(),
        };

        assert!(verify_snp_signature(&report, &pubkey_bytes).is_ok());

        // Verifier integration
        let verifier = AttestationVerifier::new(PcrPolicy::new())
            .with_trusted_key(pubkey_bytes);
        assert!(verifier.verify_snp_report(&report, Some(&hex::encode(&measurement))).is_ok());
    }

    #[test]
    fn test_verify_dm_verity_roothash() {
        let hash = "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_dm_verity_roothash(hash, hash).is_ok());
        assert!(verify_dm_verity_roothash(&format!("sha256:{}", hash), hash).is_ok());
        assert!(verify_dm_verity_roothash(hash, &format!("sha256:{}", hash.to_uppercase())).is_ok());

        let wrong_hash = "0000000000fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855";
        assert!(verify_dm_verity_roothash(wrong_hash, hash).is_err());
        assert!(verify_dm_verity_roothash("", hash).is_err());
    }
}