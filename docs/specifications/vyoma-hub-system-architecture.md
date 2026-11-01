# Vyoma Hub — System Architecture & Product Blueprint

This document defines the complete product and engineering specification for **Vyoma Hub** (`hub.vyoma.dev` / `vyoma.io`), the standalone SaaS platform and Official App Store for hardware-isolated micro-OS appliances and certified Linux kernels.

---

## 1. High-Level Vision & Separation of Concerns

Just as the **Docker Engine** is separate from **Docker Hub**, Vyoma is structured into discrete layers:

```
┌────────────────────────────────────────────────────────────────────────┐
│                        Vyoma Ecosystem Topology                        │
├──────────────────────┬─────────────────────────┬───────────────────────┤
│ 1. Vyoma Engine      │ 2. Vyoma Marketing Site │ 3. Vyoma Hub Platform │
│    (This Repository) │    (Vyom-Site / Netlify)│    (Separate SaaS App)│
│                      │                         │                       │
│ • Local MicroVM VMM  │ • Product Landing Page  │ • OCI & VMIF Registry │
│ • KVM & TAP Net      │ • Feature Showcases     │ • Kernel Catalog Store│
│ • CLI (`vyoma run`)  │ • Quickstart & Docs     │ • Publisher Identity  │
│ • Web UI (Dashboard) │ • Community & Pricing   │ • Notary Ed25519 Sign │
│ • Daemon (`vyomad`)  │                         │ • Webhooks & Scanning │
└──────────────────────┴─────────────────────────┴───────────────────────┘
```

---

## 2. Platform Architecture

Vyoma Hub operates as a distributed, multi-tenant cloud service:

```
                                  [ Cloudflare Edge CDN ]
                                             │
                   ┌─────────────────────────┴─────────────────────────┐
                   ▼                                                   ▼
        [ Web UI Frontend ]                                [ API Gateway / Ingress ]
        (Next.js / React)                                  (Envoy / Traefik)
                   │                                                   │
                   │               ┌───────────────────────────────────┼───────────────────────────────────┐
                   ▼               ▼                                   ▼                                   ▼
        ┌───────────────────────────────┐               ┌───────────────────────────────┐   ┌───────────────────────────────┐
        │   Web API & Dashboard Svc     │               │   OCI/VMIF Registry Service   │   │     Token & Auth Service      │
        │ • Org / Team Management       │               │ • OCI Distribution v2 Spec    │   │ • /v2/token (Docker CLI auth) │
        │ • RBAC & Access Tokens        │               │ • VMIF Layer & Manifest Store │   │ • OAuth2 (GitHub / SSO)       │
        │ • Webhooks & Notifications    │               │ • Range Requests & Resumes    │   │ • Scoped JWT Minting          │
        └──────────────┬────────────────┘               └──────────────┬────────────────┘   └──────────────┬────────────────┘
                       │                                               │                                   │
                       └───────────────────────┬───────────────────────┴───────────────────────────────────┘
                                               ▼
                              ┌───────────────────────────────────┐
                              │     PostgreSQL 16 Primary DB      │
                              │ • Users, Orgs, Teams, Repos       │
                              │ • Tags, Downloads, Star Metrics   │
                              │ • Audit Logs, Permissions         │
                              └───────────────────────────────────┘
                                               │
                       ┌───────────────────────┴───────────────────────┐
                       ▼                                               ▼
        ┌───────────────────────────────┐               ┌───────────────────────────────┐
        │      Redis Cluster Cache      │               │   Object Storage (S3 / R2)    │
        │ • Rate limiting (Token Bucket)│               │ • rootfs.sqfs chunks          │
        │ • Session & Token Blacklist   │               │ • kernel.vmlinuz binaries     │
        │ • Async Task Queues           │               │ • VMIF archives (.vmif)       │
        └───────────────────────────────┘               └───────────────────────────────┘
                       │                                               │
                       └───────────────────────┬───────────────────────┘
                                               ▼
                              ┌───────────────────────────────────┐
                              │     Async Background Workers      │
                              ├───────────────────────────────────┤
                              │ 1. Notary & Ed25519 Signer        │
                              │ 2. OCI-to-VMIF JIT Pre-Converter  │
                              │ 3. CVE & Rootfs Vulnerability Scan│
                              │ 4. Kernel Attestation Verifier    │
                              └───────────────────────────────────┘
```

---

## 3. Core Subsystems & Responsibilities

### 3.1 Registry Service (Dual-Engine: OCI + VMIF)
- **OCI Distribution Spec v2 Compatibility**:
  - `GET /v2/`: Version check.
  - `GET /v2/<name>/manifests/<tag>`: Pull manifest.
  - `GET /v2/<name>/blobs/<digest>`: Stream layer blobs.
  - Allows standard Docker/Podman clients to pull from `hub.vyoma.dev` with zero changes.
- **Native VMIF Appliance Protocol**:
  - `GET /v2/vyoma/appliances/<name>/manifest`: Returns `VmifManifest` with kernel profile recommendations, boot flags, and hardware requirements.
  - `GET /v2/vyoma/kernels/<profile>/<version>`: Downloads certified guest Linux kernel binaries (`vmlinux`).
  - Supports content-addressed deduplication using SHA-256 digests.

### 3.2 The Kernel Marketplace & Publisher Verification
- **Verified Publisher Program**:
  - Official badges for database companies (PostgreSQL, MongoDB, ScyllaDB), networking vendors (NGINX, Envoy), and AI platforms (PyTorch, vLLM).
  - Publishers register organization Ed25519 public keys on the Hub.
- **Kernel Catalog Directory**:
  - Public browser for all maintained and community-contributed kernels.
  - Detailed metadata pages: compilation flags, supported CPU features (AVX-512, Intel TDX, AMD SEV-SNP), benchmark throughputs, and vTPM PCR baselines.

### 3.3 Token & Identity Service
- Implements Docker-compatible token authentication:
  ```
  CLI: vyoma login hub.vyoma.dev
    │
    ├─► POST https://hub.vyoma.dev/v2/token?service=hub.vyoma.dev&scope=repository:user/app:pull,push
    │   (Basic Auth or PAT)
    │
    └─◄ Returns Bearer JWT (RS256/Ed25519 signed)
  ```
- Granular Personal Access Tokens (PATs):
  - Read-only tokens (for CI/CD deployments).
  - Read/Write tokens (for release pipelines).
  - Organization administrative tokens.

### 3.4 Notary & Cryptographic Supply Chain (The Gatekeeper)
- Every image pushed to Vyoma Hub is verified:
  1. The publisher signs the `VmifManifest` using their private Ed25519 key.
  2. The Hub Notary verifies the publisher's key and countersigns the manifest using the **Vyoma Hub Root Authority**.
  3. The signature and manifest hash are recorded in an append-only transparency log (Merkle tree).
  4. When a user runs `vyoma run hub.vyoma.dev/org/app`, the local daemon verifies both the Publisher Key and the Vyoma Hub CA.

### 3.5 Automated OCI $\rightarrow$ VMIF Mirroring & Pre-conversion
- Background Celery/Temporal workers continuously mirror the top 1,000 Docker Hub images (Ubuntu, Alpine, Debian, Node, Python, Golang, Postgres, NGINX).
- Automatically converts them to Squashfs, extracts entrypoints, tags them with certified kernel profiles from `KernelCatalog`, and stores them in S3.
- **Result**: Developers experience instantaneous `<250ms` cold boots without waiting for local OCI layer unpacking.

---

## 4. Web Application Frontend (The Hub UI)

A modern, responsive web application (Next.js / TypeScript / Vanilla CSS):

1. **Appliance Explorer**:
   - Filter by workload category: **Databases**, **Web & Ingress**, **AI & ML**, **Security & Vaults**, **Operating Systems**.
   - Search by tags, software versions, and kernel requirements.
2. **Appliance Profile Page**:
   - Readme markdown rendered with copy-paste CLI commands (`vyoma run hub.vyoma.dev/postgres:16`).
   - Hardware requirements panel (vCPU, RAM, Disk).
   - Recommended Kernel Profile with direct link to the Kernel Explorer.
   - Security scorecard: Ed25519 signature status, CVE scan report, and PCR measurement values.
3. **Kernel Explorer**:
   - Interactive comparison table of all available kernels (`vyoma/kernel-db`, `vyoma/kernel-edge`, `vyoma/kernel-ai`, etc.).
   - Interactive command-line generator for custom boot arguments.
4. **Organization & Team Management**:
   - Role-based permissions: Owner, Maintainer, Contributor, Read-Only.
   - Private and public repository toggles.
   - Webhook configurations (trigger builds on push, Slack/Discord notifications).

---

## 5. Technology Stack Recommendation

| Component | Recommended Technology | Rationale |
|---|---|---|
| **Frontend** | Next.js 15 (React 19, TypeScript) | Server-side rendering (SSR) for SEO, fast performance, rich ecosystem. |
| **Styling** | Vanilla CSS / CSS Modules | Matches Vyoma's high-performance design system with zero bloated runtime. |
| **Backend API** | Go (`gin` / `chi`) or Rust (`axum`) | Extremely high throughput, low latency for streaming gigabyte blobs. |
| **Primary Database**| PostgreSQL 16 | Relational consistency, JSONB support for manifests, ACID transactions. |
| **Cache & Queues** | Redis + Dragonfly | Ultra-low latency rate limiting and task queues. |
| **Blob Storage** | Cloudflare R2 / AWS S3 | Zero egress fees on R2; high availability for large layer and kernel files. |
| **Edge & CDN** | Cloudflare Enterprise / Workers | Global caching of read-only Squashfs layers and kernel images. |
| **Search Engine** | Meilisearch or PostgreSQL Full-Text | Instant typo-tolerant search across appliances, tags, and kernels. |

---

## 6. Implementation Phases

```
┌────────────────────────────────────────────────────────────────────────┐
│                      Vyoma Hub Development Phases                      │
├────────────────────────────────────────────────────────────────────────┤
│ Phase 1: MVP Registry & Auth (Weeks 1–6)                               │
│ • Docker-compatible /v2/ OCI endpoints                                 │
│ • User registration, OAuth login, and Personal Access Tokens           │
│ • S3-backed blob storage with chunked streaming                        │
│ • CLI integration: `vyoma login`, `vyoma pull`, `vyoma push`           │
├────────────────────────────────────────────────────────────────────────┤
│ Phase 2: VMIF Appliance & Kernel Catalog Engine (Weeks 7–12)           │
│ • Native `.vmif` appliance upload and manifest parsing                 │
│ • Workload-Tuned Kernel Catalog publishing and distribution            │
│ • Automated OCI-to-VMIF conversion worker for popular Docker images    │
│ • Hub UI: Appliance Explorer & Kernel Inspector                        │
├────────────────────────────────────────────────────────────────────────┤
│ Phase 3: Cryptographic Notary & Security Gatekeeper (Weeks 13–18)       │
│ • Ed25519 signature verification on push                               │
│ • Verified Publisher badges & organization key management              │
│ • Automated CVE and rootfs vulnerability scanning                      │
│ • Merkle-tree transparency log for image provenance                    │
├────────────────────────────────────────────────────────────────────────┤
│ Phase 4: Enterprise & Marketplace (Weeks 19–24)                        │
│ • Private registries for enterprise self-hosting (Vyoma Hub Enterprise)│
│ • Paid commercial kernels & appliances with license verification       │
│ • Team RBAC, SAML/SSO integration, and audit compliance logging        │
└────────────────────────────────────────────────────────────────────────┘
```
