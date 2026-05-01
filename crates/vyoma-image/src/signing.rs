use crate::vmif::VmifManifest;
use ed25519_dalek::{Signature, SignatureError, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use std::collections::HashSet;
use std::path::PathBuf;
use thiserror::Error;
use tracing::info;

#[derive(Error, Debug)]
pub enum SigningError {
    #[error("Signing failed: {0}")]
    SignError(String),
    #[error("Verification failed: {0}")]
    VerifyError(String),
    #[error("Key error: {0}")]
    KeyError(String),
    #[error("IO error: {0}")]
    Io(#[from] std::io::Error),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SignedManifest {
    pub manifest: VmifManifest,
    pub signature: Vec<u8>,
    pub public_key: Vec<u8>,
}

pub struct SigningKeyPair {
    signing_key: SigningKey,
    verifying_key: VerifyingKey,
}

impl SigningKeyPair {
    pub fn generate() -> Self {
        let mut csprng = rand::rngs::OsRng;
        let signing_key = SigningKey::generate(&mut csprng);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
        }
    }

    pub fn from_bytes(secret: &[u8], public: &[u8]) -> Result<Self, SigningError> {
        let signing_key = SigningKey::from_bytes(
            secret
                .try_into()
                .map_err(|_| SigningError::KeyError("Invalid secret key".to_string()))?,
        );
        let verifying_key = VerifyingKey::from_bytes(
            public
                .try_into()
                .map_err(|_| SigningError::KeyError("Invalid public key".to_string()))?,
        )
        .map_err(|e| SigningError::KeyError(format!("Invalid public key: {:?}", e)))?;

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_bytes().to_vec()
    }

    pub fn sign_manifest(&self, manifest: &VmifManifest) -> Result<SignedManifest, SigningError> {
        let manifest_bytes =
            serde_json::to_vec(manifest).map_err(|e| SigningError::SignError(e.to_string()))?;

        let signature = self.signing_key.sign(&manifest_bytes);

        Ok(SignedManifest {
            manifest: manifest.clone(),
            signature: signature.to_bytes().to_vec(),
            public_key: self.public_key_bytes(),
        })
    }

    pub fn verify_manifest(&self, signed: &SignedManifest) -> Result<(), SigningError> {
        let manifest_bytes = serde_json::to_vec(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        let signature = Signature::from_slice(&signed.signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        self.verifying_key
            .verify(&manifest_bytes, &signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        Ok(())
    }
}

pub struct TrustPolicy {
    require_signed: bool,
    trusted_keys: HashSet<Vec<u8>>,
}

impl TrustPolicy {
    pub fn new(require_signed: bool) -> Self {
        Self {
            require_signed,
            trusted_keys: HashSet::new(),
        }
    }

    pub fn add_trusted_key(&mut self, key: Vec<u8>) {
        self.trusted_keys.insert(key);
    }

    pub fn load_trusted_keys_from_dir(&mut self, dir: PathBuf) -> Result<(), SigningError> {
        if !dir.exists() {
            return Ok(());
        }

        for entry in std::fs::read_dir(dir)? {
            let entry = entry?;
            let path = entry.path();

            if path.extension().map_or(false, |ext| ext == "pub") {
                let key_data = std::fs::read(&path)?;
                self.add_trusted_key(key_data);
                info!("Loaded trusted key from {:?}", path);
            }
        }

        Ok(())
    }

    pub fn verify(&self, signed: &SignedManifest) -> Result<(), SigningError> {
        if self.require_signed && self.trusted_keys.is_empty() {
            return Err(SigningError::VerifyError(
                "No trusted keys configured but require_signed is true".to_string(),
            ));
        }

        if self.trusted_keys.is_empty() {
            return Ok(());
        }

        if !self.trusted_keys.contains(&signed.public_key) {
            return Err(SigningError::VerifyError(
                "Public key not in trusted keys".to_string(),
            ));
        }

        let verifying_key = VerifyingKey::from_bytes(
            signed
                .public_key
                .as_slice()
                .try_into()
                .map_err(|_| SigningError::VerifyError("Invalid public key".to_string()))?,
        )
        .map_err(|e| SigningError::VerifyError(format!("Invalid key: {:?}", e)))?;

        let manifest_bytes = serde_json::to_vec(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        let signature = Signature::from_slice(&signed.signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        verifying_key
            .verify(&manifest_bytes, &signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        Ok(())
    }
}

impl SignedManifest {
    pub fn to_bytes(&self) -> Result<Vec<u8>, SigningError> {
        serde_json::to_vec(self).map_err(|e| SigningError::SignError(e.to_string()))
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, SigningError> {
        serde_json::from_slice(data).map_err(|e| SigningError::VerifyError(e.to_string()))
    }

    pub fn save_to_file(&self, path: &PathBuf) -> Result<(), SigningError> {
        let data = self.to_bytes()?;
        std::fs::write(path, data)?;
        info!("Saved signed manifest to {:?}", path);
        Ok(())
    }

    pub fn load_from_file(path: &PathBuf) -> Result<Self, SigningError> {
        let data = std::fs::read(path)?;
        Self::from_bytes(&data)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_generate_keypair() {
        let keypair = SigningKeyPair::generate();
        assert_eq!(keypair.public_key_bytes().len(), 32);
    }

    #[test]
    fn test_sign_manifest() {
        let keypair = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        assert!(!signed.signature.is_empty());
        assert_eq!(signed.public_key.len(), 32);
    }

    #[test]
    fn test_verify_manifest() {
        let keypair = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        let result = keypair.verify_manifest(&signed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_verify_with_wrong_key() {
        let keypair1 = SigningKeyPair::generate();
        let keypair2 = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair1.sign_manifest(&manifest).unwrap();

        let result = keypair2.verify_manifest(&signed);
        assert!(result.is_err());
    }

    #[test]
    fn test_trust_policy_with_key() {
        let keypair = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        let mut policy = TrustPolicy::new(false);
        policy.add_trusted_key(keypair.public_key_bytes());

        let result = policy.verify(&signed);
        assert!(result.is_ok());
    }

    #[test]
    fn test_trust_policy_reject_unknown_key() {
        let keypair = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        let mut policy = TrustPolicy::new(true);
        policy.add_trusted_key(vec![0; 32]);

        let result = policy.verify(&signed);
        assert!(result.is_err());
    }

    #[test]
    fn test_signed_manifest_serialization() {
        let keypair = SigningKeyPair::generate();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        let bytes = signed.to_bytes().unwrap();
        let loaded = SignedManifest::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.manifest, signed.manifest);
        assert_eq!(loaded.signature, signed.signature);
    }
}
