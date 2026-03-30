# ADR-029: WSL2/KVM Fixes - Patch v1.3.1

## Status
Accepted | Patch v1.3.1

## Context
Users on WSL2 (Ubuntu 24.04) reported two issues:

1. **DNS Binding Failure**: `Cannot assign requested address (os error 99)` - Race condition where DNS server tries to bind before bridge IP is ready
2. **KVM Access Denied**: `/dev/kvm` permission denied for unprivileged `ignite` user

## Decision

### Fix 1: KVM Group Membership
Add KVM group configuration to postinstall script (per technical spec line 529):
```bash
# Add ignite to kvm group for /dev/kvm access
usermod -aG kvm ignite 2>/dev/null || true

# Fix /dev/kvm permissions
chmod 0660 /dev/kvm 2>/dev/null || true
chown root:kvm /dev/kvm 2>/dev/null || true
```

### Fix 2: DNS Race Condition
Add retry logic with bridge readiness check before DNS binding:
- Wait for bridge IP to be ready before binding
- Add exponential backoff retry

## Consequences
**Positive:**
- KVM access works for ignite user
- DNS binds reliably on WSL2

**Negative:**
- Slight delay in DNS startup (negligible)

## References
- Technical Spec Line 529: KVM group configuration
- Issue: WSL2 DNS binding failure
