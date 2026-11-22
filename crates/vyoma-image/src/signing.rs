use crate::vmif::VmifManifest;
use ed25519_dalek::{Signature, Signer, SigningKey, Verifier, VerifyingKey};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashSet;
use std::path::{PathBuf, Path};
use thiserror::Error;
use tracing::info;
use rand::rngs::OsRng;

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
        let signing_key = SigningKey::generate(&mut OsRng);
        let verifying_key = signing_key.verifying_key();

        Self {
            signing_key,
            verifying_key,
        }
    }

    /// Load from raw bytes: expects seed (32 bytes) + public_key (32 bytes) = 64 bytes.
    pub fn from_seed_and_public(seed: &[u8], public: &[u8]) -> Result<Self, SigningError> {
        let seed: [u8; 32] = seed
            .try_into()
            .map_err(|_| SigningError::KeyError("Invalid seed length (expected 32 bytes)".to_string()))?;
        let public: [u8; 32] = public
            .try_into()
            .map_err(|_| SigningError::KeyError("Invalid public key length (expected 32 bytes)".to_string()))?;

        let signing_key = SigningKey::from_bytes(&seed);
        let verifying_key = VerifyingKey::from_bytes(&public)
            .map_err(|_| SigningError::KeyError("Invalid public key".to_string()))?;

        // Verify consistency
        let derived_pub = signing_key.verifying_key();
        if derived_pub.as_bytes() != &public {
            return Err(SigningError::KeyError("Seed does not match public key".to_string()));
        }

        Ok(Self {
            signing_key,
            verifying_key,
        })
    }

    /// Serialize keypair as seed (32 bytes) || public_key (32 bytes) = 64 bytes.
    pub fn to_seed_and_public(&self) -> (Vec<u8>, Vec<u8>) {
        (self.signing_key.to_bytes().to_vec(), self.verifying_key.as_bytes().to_vec())
    }

    pub fn public_key_bytes(&self) -> Vec<u8> {
        self.verifying_key.as_bytes().to_vec()
    }

    pub fn sign_manifest(&self, manifest: &VmifManifest) -> Result<SignedManifest, SigningError> {
        let manifest_bytes = crate::canonical::to_canonical_json(manifest)
            .map_err(|e| SigningError::SignError(e.to_string()))?;

        let signature = self.signing_key.sign(&manifest_bytes);

        Ok(SignedManifest {
            manifest: manifest.clone(),
            signature: signature.to_bytes().to_vec(),
            public_key: self.public_key_bytes(),
        })
    }

    pub fn verify_manifest(&self, signed: &SignedManifest) -> Result<(), SigningError> {
        let canonical_bytes = crate::canonical::to_canonical_json(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        let signature = Signature::from_slice(&signed.signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        // Primary: verify against RFC 8785 canonical JSON bytes
        if self.verifying_key.verify(&canonical_bytes, &signature).is_ok() {
            return Ok(());
        }

        // Backward compatibility fallback: verify against legacy serde_json::to_vec
        let legacy_bytes = serde_json::to_vec(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        self.verifying_key
            .verify(&legacy_bytes, &signature)
            .map_err(|e| SigningError::VerifyError(format!("Signature verification failed: {}", e)))?;

        Ok(())
    }

    pub fn sign_binary(&self, data: &[u8]) -> Result<Vec<u8>, SigningError> {
        let signature = self.signing_key.sign(data);
        Ok(signature.to_bytes().to_vec())
    }

    pub fn sign_file(&self, path: &Path) -> Result<Vec<u8>, SigningError> {
        let data = std::fs::read(path)?;
        self.sign_binary(&data)
    }

    pub fn verify_binary(&self, data: &[u8], signature: &[u8]) -> Result<(), SigningError> {
        let sig = Signature::from_slice(signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;
        self.verifying_key
            .verify(data, &sig)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;
        Ok(())
    }

    pub fn verify_file(&self, path: &Path, signature: &[u8]) -> Result<(), SigningError> {
        let data = std::fs::read(path)?;
        self.verify_binary(&data, signature)
    }

    /// Saves the keypair into a private key file (`<key_base_name>.key`) and public key file (`<key_base_name>.pub`).
    pub fn save_keypair_to_files(&self, keys_dir: &Path, key_base_name: &str) -> Result<(PathBuf, PathBuf), SigningError> {
        std::fs::create_dir_all(keys_dir)?;

        let priv_path = keys_dir.join(format!("{}.key", key_base_name));
        let pub_path = keys_dir.join(format!("{}.pub", key_base_name));

        let (seed, pub_bytes) = self.to_seed_and_public();
        let mut key_file_data = seed;
        key_file_data.extend_from_slice(&pub_bytes);

        std::fs::write(&priv_path, key_file_data)?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&priv_path, std::fs::Permissions::from_mode(0o600));
        }

        std::fs::write(&pub_path, pub_bytes)?;

        Ok((priv_path, pub_path))
    }

    /// Loads a keypair from a private key file (`.key`).
    pub fn load_keypair_from_file(key_path: &Path) -> Result<Self, SigningError> {
        let data = std::fs::read(key_path)?;
        if data.len() == 64 {
            Self::from_seed_and_public(&data[..32], &data[32..])
        } else if data.len() == 32 {
            let signing_key = SigningKey::from_bytes(
                data.as_slice()
                    .try_into()
                    .map_err(|_| SigningError::KeyError("Invalid seed length".to_string()))?,
            );
            let verifying_key = signing_key.verifying_key();
            Ok(Self {
                signing_key,
                verifying_key,
            })
        } else {
            Err(SigningError::KeyError(format!(
                "Invalid private key file length: {} bytes (expected 32 or 64)",
                data.len()
            )))
        }
    }
}

pub fn compute_hash(data: &[u8]) -> Vec<u8> {
    let mut hasher = Sha256::new();
    hasher.update(data);
    hasher.finalize().to_vec()
}

pub fn compute_file_hash(path: &PathBuf) -> Result<Vec<u8>, SigningError> {
    let data = std::fs::read(path)?;
    Ok(compute_hash(&data))
}

pub struct BinarySignature {
    pub signature: Vec<u8>,
    public_key: Vec<u8>,
}

impl BinarySignature {
    pub fn new(signature: Vec<u8>, public_key: Vec<u8>) -> Self {
        Self {
            signature,
            public_key,
        }
    }

    pub fn to_bytes(&self) -> Vec<u8> {
        let mut bytes = self.signature.clone();
        bytes.extend(&self.public_key);
        bytes
    }

    pub fn from_bytes(data: &[u8]) -> Result<Self, SigningError> {
        if data.len() < 64 {
            return Err(SigningError::VerifyError(
                "Invalid binary signature data".to_string(),
            ));
        }
        let signature = data[..64].to_vec();
        let public_key = data[64..].to_vec();
        Ok(Self {
            signature,
            public_key,
        })
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

    /// Loads trusted public keys from standard user (`~/.vyoma/trusted_keys/`) and system (`/etc/vyoma/trusted_keys/`) paths.
    pub fn load_standard_trusted_dirs(&mut self) -> Result<(), SigningError> {
        if let Ok(home) = std::env::var("HOME") {
            let user_dir = Path::new(&home).join(".vyoma").join("trusted_keys");
            self.load_trusted_keys_from_dir(user_dir)?;
        }

        let sys_dir = PathBuf::from("/etc/vyoma/trusted_keys");
        self.load_trusted_keys_from_dir(sys_dir)?;

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

        let canonical_bytes = crate::canonical::to_canonical_json(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        let signature = Signature::from_slice(&signed.signature)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        // Primary: verify against RFC 8785 canonical bytes
        if verifying_key.verify(&canonical_bytes, &signature).is_ok() {
            return Ok(());
        }

        // Backward compatibility fallback: verify against legacy serde_json::to_vec
        let legacy_bytes = serde_json::to_vec(&signed.manifest)
            .map_err(|e| SigningError::VerifyError(e.to_string()))?;

        verifying_key
            .verify(&legacy_bytes, &signature)
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

    pub fn load_from_file(path: &Path) -> Result<Self, SigningError> {
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
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        );

        let signed = keypair.sign_manifest(&manifest).unwrap();

        let bytes = signed.to_bytes().unwrap();
        let loaded = SignedManifest::from_bytes(&bytes).unwrap();

        assert_eq!(loaded.manifest.arch, signed.manifest.arch);
        assert_eq!(loaded.manifest.rootfs, signed.manifest.rootfs);
        assert_eq!(loaded.manifest.kernel, signed.manifest.kernel);
        assert_eq!(loaded.signature, signed.signature);
    }

    #[test]
    fn test_canonical_signing_determinism() {
        let keypair = SigningKeyPair::generate();
        let mut labels = std::collections::HashMap::new();
        labels.insert("z-key".to_string(), "z-value".to_string());
        labels.insert("a-key".to_string(), "a-value".to_string());

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:abc123".to_string(),
            config,
            1024000,
        ).with_labels(labels);

        let signed = keypair.sign_manifest(&manifest).unwrap();
        assert!(keypair.verify_manifest(&signed).is_ok());

        // Verify that canonical JSON serializer orders keys lexicographically
        let canonical_bytes = crate::canonical::to_canonical_json(&manifest).unwrap();
        let json_str = String::from_utf8(canonical_bytes).unwrap();
        assert!(json_str.find("a-key").unwrap() < json_str.find("z-key").unwrap());
    }

    #[test]
    fn test_legacy_signature_backward_compat_fallback() {
        use ed25519_dalek::Signer;
        let keypair = SigningKeyPair::generate();
        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:legacy123".to_string(),
            config,
            1024000,
        );

        // Sign using legacy non-canonical serde_json::to_vec
        let legacy_bytes = serde_json::to_vec(&manifest).unwrap();
        let sig = keypair.signing_key.sign(&legacy_bytes);

        let legacy_signed = SignedManifest {
            manifest: manifest.clone(),
            signature: sig.to_bytes().to_vec(),
            public_key: keypair.public_key_bytes(),
        };

        // Verification must succeed via legacy fallback
        assert!(keypair.verify_manifest(&legacy_signed).is_ok());

        let mut policy = TrustPolicy::new(true);
        policy.add_trusted_key(keypair.public_key_bytes());
        assert!(policy.verify(&legacy_signed).is_ok());
    }

    #[test]
    fn test_save_and_load_developer_keypair() {
        let keypair = SigningKeyPair::generate();
        let temp_dir = tempfile::tempdir().unwrap();

        let (priv_path, pub_path) = keypair.save_keypair_to_files(temp_dir.path(), "developer").unwrap();
        assert!(priv_path.exists());
        assert!(pub_path.exists());

        let loaded = SigningKeyPair::load_keypair_from_file(&priv_path).unwrap();
        assert_eq!(loaded.public_key_bytes(), keypair.public_key_bytes());

        // Test trust policy loading from directory
        let mut policy = TrustPolicy::new(true);
        policy.load_trusted_keys_from_dir(temp_dir.path().to_path_buf()).unwrap();

        let config = crate::vmif::OciImageConfig::default();
        let manifest = VmifManifest::new(
            "amd64".to_string(),
            None,
            None,
            "sha256:test123".to_string(),
            config,
            1024,
        );
        let signed = loaded.sign_manifest(&manifest).unwrap();
        assert!(policy.verify(&signed).is_ok());
    }
}
