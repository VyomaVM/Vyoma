use anyhow::{anyhow, Result};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::Path;
use tracing::{info, warn};

use crate::vtpm::PcrPolicy;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TpmQuote {
    pub quote: Vec<u8>,
    pub signature: Vec<u8>,
    pub pcr_values: HashMap<u32, String>,
    pub timestamp: String,
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

pub struct AttestationVerifier {
    pcr_policy: PcrPolicy,
    trusted_keys: Vec<Vec<u8>>,
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
}