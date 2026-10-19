# Vyoma Measured Boot Architecture

## Overview

This document describes the Trusted Boot architecture for Vyoma VMs using UEFI Secure Boot, virtual TPM (vTPM), and remote attestation.

## Architecture Components

### 1. UEFI Firmware with Secure Boot

- **OVMF**: Open Virtual Machine Firmware with Secure Boot enabled
- **Firmware Location**: `/var/lib/vyoma/firmware/OVMF_CODE.secboot.fd`
- **UEFI Variables**: Per-VM copy-on-write UEFI variable store

### 2. Virtual TPM (vTPM)

- **Implementation**: swtpm (Software TPM Emulator)
- **TPM Version**: TPM 2.0
- **Per-VM Socket**: `{vm_dir}/tpm/swtpm.sock`

### 3. Boot Chain Measurement

| PCR Index | Measurement Description |
|-----------|------------------------|
| 0 | Firmware code (OVMF) |
| 1 | Firmware configuration |
| 4 | Boot manager code (shim/systemd-boot) |
| 5 | Boot manager configuration |
| 7 | Secure Boot state |
| 9 | Kernel image |
| 10 | Initrd |
| 14 | Root filesystem (via dm-verity) |

### 4. Signing Infrastructure

- **Algorithm**: Ed25519
- **Keys**: CI signing key embedded in VMIF manifest
- **Signed Components**:
  - VMIF manifest
  - Kernel binary
  - Initrd

### 5. Attestation Flow

```
┌─────────────┐     ┌─────────────┐     ┌─────────────┐
│   VM (VTPM) │────▶│  Attestation │────▶│   Verifier  │
│   Running   │     │   Service    │     │   Service   │
└─────────────┘     └─────────────┘     └─────────────┘
      │                    │                    │
      │ TPM Quote          │ TPM Quote +       │ Verify PCR
      │ (PCR values)       │ VMIF manifest     │ against expected
      ▼                    ▼                    ▼
  PCR 7,9,10           Validate sig         Return "Verified" / "Failed"
```

## Policy Engine

### `require-measured-boot` Policy

When enabled:
1. VM startup triggers automatic attestation
2. Attestation verifier compares PCR values against signed VMIF manifest
3. If verification fails:
   - VM is paused
   - Event emitted
   - Network access optionally blocked

### Configuration API

```bash
# Enable measured boot requirement
curl -X POST /policy -H "Content-Type: application/json" \
  -d '{"policy": "require-measured-boot", "enabled": true}'

# Get current policies
curl /policy
```

## Security Guarantees

### What is Prevented

1. **Modified Kernel**: Attacker replaces kernel → PCR 9 mismatch → attestation fails
2. **Tampered Initrd**: Attacker modifies initrd → PCR 10 mismatch → attestation fails
3. **Rootfs Tampering**: With dm-verity → PCR 14 mismatch → attestation fails
4. **Secure Boot Bypass**: Without valid signature → PCR 7 indicates insecure state
5. **Firmware Modification**: PCR 0/1 would reflect changed firmware

### What is NOT Prevented

1. **Host Memory Inspection**: Requires SEV-SNP (Phase 2)
2. **Cold Boot Attacks**: Hardware-dependent mitigation
3. **Insider Threat**: Host administrator with physical access
4. **Side-Channel Attacks**: Spectre/Meltdown-class vulnerabilities

## Threat Model

| Threat | Mitigation | Phase |
|--------|------------|-------|
| Modified kernel/initrd | TPM PCR measurement | 1 |
| Rootfs tampering | dm-verity + PCR 14 | 1 |
| Secure Boot bypass | OVMF + signed assets | 1 |
| VM impersonation | TPM EK certificate chain | 1 |
| Host memory read | SEV-SNP encryption | 2 |
| Runtime attack | Encrypted memory + GPU | 2 |

## Implementation Status

- ✅ TPM 2.0 via swtpm
- ✅ PCR policy definition
- ✅ Attestation verifier
- ✅ Policy API
- ⏳ OVMF binary bundling
- ⏳ End-to-end integration testing

## References

- [TPM 2.0 Specification](https://trustedcomputinggroup.org/resource/tpm-library-specification/)
- [EDK II OVMF](https://github.com/tianocore/tianocore.github.io/wiki/OVMF)
- [swtpm Documentation](https://github.com/stefanberger/swtpm)
- [Cloud Hypervisor TPM Support](https://www.cloudhypervisor.org/)