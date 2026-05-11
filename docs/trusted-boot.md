# Trusted Boot Guide

This guide explains how to set up, build, and run VMs with Vyoma's measured boot and attestation system.

## Overview

Vyoma's measured boot system provides cryptographic verification that your VM's firmware, kernel, initrd, and rootfs have not been tampered with. The flow works as follows:

1. **Build Phase**: An ephemeral VM is launched with a virtual TPM (vTPM). The VM boots the final rootfs with OVMF firmware, which measures each component into the TPM's Platform Configuration Registers (PCRs). These expected PCR values are then embedded into a signed VMIF manifest.

2. **Runtime Phase**: When a VM starts with `require-measured-boot` policy enabled, the daemon obtains a fresh TPM quote from the running VM's vTPM, compares it against the signed expected values, and only allows the VM to transition to `Running` state if they match.

## Prerequisites

- `swtpm`: Software TPM emulator (`apt install swtpm` or equivalent)
- `tpm2-tools`: TPM utilities (`apt install tpm2-tools`)
- OVMF firmware: UEFI firmware for the hypervisor
- A signing key for manifest signing

## Quick Start

### 1. Enable Measured Boot Policy

```bash
# Configure the daemon to require measured boot
vyomad --require-measured-boot
```

Or configure via policy file (`~/.vyoma/policy.json`):

```json
{
  "measured_boot": {
    "enabled": true,
    "required": true,
    "pcr_selection": [7, 9, 10],
    "verification_timeout_secs": 30,
    "block_on_failure": true,
    "build_signing_key_path": "/path/to/keys"
  }
}
```

### 2. Build a Measured Image

```bash
vyoma build --measured ./context
```

This will:
- Launch an ephemeral VM with a vTPM
- Boot the VM to capture PCR measurements
- Embed the PCR values in the VMIF manifest
- Sign the manifest with the configured build key
- Store `vyoma.toml` and `vyoma.toml.sig` alongside the image

### 3. Run with Attestation

```bash
vyoma run my-measured-image:latest
```

If the policy requires measured boot:
1. The VM boots with a vTPM
2. The daemon obtains a TPM quote
3. PCRs are compared against the signed manifest
4. VM transitions to `Running` only if attestation succeeds

### 4. Verify Attestation Manually

```bash
vyoma attest <vm-id>
```

Output:
```
Attestation Result: VERIFIED
VM: test-vm-12345
PCR Values:
  PCR0 (firmware):      verified
  PCR7 (secure boot):   verified
  PCR9 (kernel):        verified
  PCR10 (initrd):       verified
  PCR14 (rootfs):       verified
```

## PCR Indices

The following PCRs are typically measured during boot:

| PCR | Description |
|-----|-------------|
| 0 | Firmware (BIOS/UEFI) |
| 1 | Firmware Configuration |
| 4 | Boot Manager |
| 5 | Boot Manager Configuration |
| 7 | Secure Boot State |
| 9 | Kernel Image |
| 10 | Initrd/Initramfs |
| 14 | Rootfs |

## Policy Configuration

### MeasuredBootPolicy Options

| Option | Type | Default | Description |
|--------|------|---------|-------------|
| `enabled` | bool | false | Enable measured boot checks |
| `required` | bool | false | Require measured boot (fail if not available) |
| `pcr_selection` | Vec<u32> | [7, 9, 10] | Which PCRs to verify |
| `verification_timeout_secs` | u64 | 30 | Timeout for attestation |
| `block_on_failure` | bool | true | Kill VM on attestation failure |
| `build_signing_key_path` | Option<String> | None | Path to signing keys |

## VM Lifecycle States

When measured boot is enabled, VMs transition through these states:

1. **PendingAttestation**: VM booted, awaiting attestation verification
2. **Running**: Attestation passed, VM operational
3. **Error**: Attestation failed (if `block_on_failure` is true, VM is killed)

## Troubleshooting

### Attestation Fails - PCR Mismatch

**Symptom**: VM transitions to Error state with "PCR X mismatch" message.

**Cause**: The VM's PCR values don't match the expected values in the signed manifest.

**Solutions**:
- The rootfs or kernel may have been modified after measurement
- Rebuild the image with `vyoma build --measured`
- Ensure the image hasn't been altered at runtime

### Attestation Fails - Timeout

**Symptom**: VM transitions to Error state with "Attestation timed out" message.

**Cause**: The vTPM didn't respond within the configured timeout.

**Solutions**:
- Increase `verification_timeout_secs` in policy
- Check that the vTPM socket is accessible
- Verify swtpm is running correctly

### No Signed Manifest

**Symptom**: Error "Image has no signed manifest (vyoma.toml.sig)".

**Cause**: The image was built without the `--measured` flag.

**Solutions**:
- Rebuild the image: `vyoma build --measured <context>`
- Disable `require_signed_manifest` in policy (not recommended for production)

### Manifest Signature Verification Fails

**Symptom**: Error "Manifest signature verification failed".

**Cause**: The manifest was signed with a different key than what's trusted.

**Solutions**:
- Ensure trusted keys are in `~/.vyoma/keys/trusted/`
- Add the correct public key to the trusted keys directory
- Verify the build was done with the correct signing key

## Security Considerations

1. **Protect Signing Keys**: The build signing key must be kept secure. If compromised, attackers could create valid manifests for tampered images.

2. **Key Rotation**: Establish a process for rotating signing keys while maintaining trust.

3. **Trusted Key Management**: Regularly audit the trusted keys directory to ensure only authorized keys are present.

4. **No Rootfs Modifications**: Once measured, the rootfs must not be modified. Any changes require rebuilding with `--measured`.

5. **Secure Boot**: For full chain verification, enable Secure Boot in OVMF and ensure PK/KEK/DB are properly configured.

## Advanced: Manual Attestation

For advanced use cases, you can perform attestation manually:

```rust
use vyoma_core::unified_attest::UnifiedAttestationManager;
use vyoma_image::signing::SignedManifest;

let signed = SignedManifest::load_from_file("vyoma.toml.sig")?;
let expected_pcrs = signed.manifest.measured_boot.pcr_policy?;

let manager = UnifiedAttestationManager::new();
let response = get_tpm_quote_from_vm()?; // Your implementation

let result = manager.verify_tpm_attestation(&response, &expected_pcrs)?;
if result.verified {
    println!("Attestation PASSED");
} else {
    println!("Attestation FAILED: {:?}", result.error);
}
```

## References

- [TPM 2.0 Specification](https://trustedcomputinggroup.org/tpm-library-specification/)
- [OVMF Documentation](https://github.com/tianocore/tianocore.github.io/wiki/OVMF)
- [swtpm Project](https://github.com/stefanberger/swtpm)
