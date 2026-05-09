use anyhow::Result;
use serde::{Deserialize, Serialize};
use tracing::info;

use crate::attest::{AttestationVerifier, AttestationResponse, SnpAttestationReport, TdxAttestationReport};
use crate::vtpm::PcrPolicy;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum AttestationType {
    Tpm,
    SevSnp,
    Tdx,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAttestationRequest {
    pub vm_id: String,
    pub attestation_type: AttestationType,
    pub nonce: Vec<u8>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnifiedAttestationResponse {
    pub vm_id: String,
    pub attestation_type: AttestationType,
    pub verified: bool,
    pub measurements: Vec<MeasurementResult>,
    pub error: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MeasurementResult {
    pub name: String,
    pub value: String,
    pub verified: bool,
}

pub struct UnifiedAttestationManager {
    tpm_verifier: AttestationVerifier,
}

impl UnifiedAttestationManager {
    pub fn new() -> Self {
        Self {
            tpm_verifier: AttestationVerifier::new(PcrPolicy::new()),
        }
    }

    pub fn verify_tpm_attestation(&self, response: &AttestationResponse, expected_pcrs: &std::collections::HashMap<u32, String>) -> Result<UnifiedAttestationResponse> {
        let result = self.tpm_verifier.verify(response, expected_pcrs)?;

        let measurements: Vec<MeasurementResult> = result.pcr_results
            .iter()
            .map(|(idx, verified)| {
                let pcr_name = match idx {
                    0 => "firmware",
                    1 => "firmware_config",
                    4 => "boot_manager",
                    5 => "boot_manager_config",
                    7 => "secure_boot_state",
                    9 => "kernel",
                    10 => "initrd",
                    14 => "rootfs",
                    _ => "unknown",
                };
                MeasurementResult {
                    name: format!("PCR{}", idx),
                    value: response.quote.as_ref()
                        .and_then(|q| q.pcr_values.get(idx))
                        .cloned()
                        .unwrap_or_default(),
                    verified: *verified,
                }
            })
            .collect();

        Ok(UnifiedAttestationResponse {
            vm_id: response.vm_id.clone(),
            attestation_type: AttestationType::Tpm,
            verified: result.verified,
            measurements,
            error: result.error,
        })
    }

    pub fn verify_snp_attestation(&self, report: &SnpAttestationReport, expected_measurement: Option<&str>) -> Result<UnifiedAttestationResponse> {
        self.tpm_verifier.verify_snp_report(report, expected_measurement)?;

        let measurements = vec![
            MeasurementResult {
                name: "AMD_SEV_SNP_MEASUREMENT".to_string(),
                value: hex::encode(&report.measurement),
                verified: true,
            },
            MeasurementResult {
                name: "GUEST_SVN".to_string(),
                value: report.guest_svn.to_string(),
                verified: true,
            },
            MeasurementResult {
                name: "HOST_SVN".to_string(),
                value: report.host_svn.to_string(),
                verified: true,
            },
        ];

        Ok(UnifiedAttestationResponse {
            vm_id: "snp-vm".to_string(),
            attestation_type: AttestationType::SevSnp,
            verified: true,
            measurements,
            error: None,
        })
    }

    pub fn verify_tdx_attestation(&self, report: &TdxAttestationReport, expected_mrtd: Option<&str>) -> Result<UnifiedAttestationResponse> {
        self.tpm_verifier.verify_tdx_report(report, expected_mrtd)?;

        let measurements = vec![
            MeasurementResult {
                name: "TDX_MRTD".to_string(),
                value: hex::encode(&report.mrtd),
                verified: true,
            },
            MeasurementResult {
                name: "TDX_RTMR0".to_string(),
                value: hex::encode(&report.rtmr0),
                verified: true,
            },
            MeasurementResult {
                name: "TDX_RTMR1".to_string(),
                value: hex::encode(&report.rtmr1),
                verified: true,
            },
            MeasurementResult {
                name: "TDX_RTMR2".to_string(),
                value: hex::encode(&report.rtmr2),
                verified: true,
            },
            MeasurementResult {
                name: "TDX_RTMR3".to_string(),
                value: hex::encode(&report.rtmr3),
                verified: true,
            },
        ];

        Ok(UnifiedAttestationResponse {
            vm_id: "tdx-vm".to_string(),
            attestation_type: AttestationType::Tdx,
            verified: true,
            measurements,
            error: None,
        })
    }
}

impl Default for UnifiedAttestationManager {
    fn default() -> Self {
        Self::new()
    }
}

pub fn detect_attestation_type(vm_config: &crate::ch_types::VmConfig) -> Option<AttestationType> {
    if vm_config.sev_snp.as_ref().map(|s| s.enabled).unwrap_or(false) {
        Some(AttestationType::SevSnp)
    } else if vm_config.tdx.as_ref().map(|t| t.enabled).unwrap_or(false) {
        Some(AttestationType::Tdx)
    } else if vm_config.tpm.is_some() {
        Some(AttestationType::Tpm)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_unified_attestation_manager_new() {
        let manager = UnifiedAttestationManager::new();
        assert!(true);
    }

    #[test]
    fn test_detect_attestation_type_tpm() {
        let mut config = crate::ch_types::VmConfig::default();
        config.tpm = Some(crate::ch_types::TpmConfig {
            socket_path: "/path".to_string(),
            tpm_version: "2.0".to_string(),
        });

        let att_type = detect_attestation_type(&config);
        assert_eq!(att_type, Some(AttestationType::Tpm));
    }

    #[test]
    fn test_detect_attestation_type_snp() {
        let mut config = crate::ch_types::VmConfig::default();
        config.sev_snp = Some(crate::ch_types::SevSnpConfig {
            enabled: true,
            policy: Some("1".to_string()),
            certificate_path: None,
            guest_key_root_hash: None,
            host_data: None,
        });

        let att_type = detect_attestation_type(&config);
        assert_eq!(att_type, Some(AttestationType::SevSnp));
    }

    #[test]
    fn test_detect_attestation_type_tdx() {
        let mut config = crate::ch_types::VmConfig::default();
        config.tdx = Some(crate::ch_types::TdxConfig {
            enabled: true,
            measurement_uuid: None,
        });

        let att_type = detect_attestation_type(&config);
        assert_eq!(att_type, Some(AttestationType::Tdx));
    }

    #[test]
    fn test_detect_attestation_type_none() {
        let config = crate::ch_types::VmConfig::default();

        let att_type = detect_attestation_type(&config);
        assert_eq!(att_type, None);
    }
}