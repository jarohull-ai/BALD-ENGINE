<p align="center">
  <img src="assets/logo.png" alt="BALD Search Engine Logo" width="180" />
</p>

# BALD — Sovereign OSINT Search Engine & Evidence Ledger

<p align="center">
  <a href="https://github.com/jarohull-ai/BALD-ENGINE/releases"><img src="https://img.shields.io/github/v/release/jarohull-ai/BALD-ENGINE?color=f59e0b&label=Release&logo=github&style=for-the-badge" alt="GitHub Release"></a>
  <a href="https://crates.io/crates/bald-engine"><img src="https://img.shields.io/badge/Crates.io-bald--engine-eab308?logo=rust&style=for-the-badge" alt="Crates.io"></a>
  <img src="https://img.shields.io/badge/Rust-2021%20%7C%201.88-ea580c?logo=rust&style=for-the-badge" alt="Rust Version">
  <img src="https://img.shields.io/badge/Tauri-2.0-24c8db?logo=tauri&style=for-the-badge" alt="Tauri 2">
  <img src="https://img.shields.io/badge/Platform-Windows%20%7C%20macOS%20%7C%20Linux-3b82f6?style=for-the-badge" alt="Supported Platforms">
  <a href="LICENSE"><img src="https://img.shields.io/badge/License-MIT-10b981?style=for-the-badge" alt="License MIT"></a>
  <img src="https://img.shields.io/badge/Privacy-100%25%20Offline%20First-8b5cf6?style=for-the-badge" alt="Privacy First">
</p>

---

## Overview

**BALD** is a sovereign, offline-first OSINT search engine and cryptographic evidence ledger built in **Rust** and **Tauri 2**.

Unlike conventional commercial search engines, BALD does not index or track individual private persons, nor does it subject queries to corporate SEO manipulation, deplatforming, or algorithmic censorship. 

Instead of a generic list of sponsored links, BALD provides a structured **Evidence Ledger**:
* **Source Tier & Reputation Score**
* **Verbatim Citation Gate Verification** (exact matching with normalized Jaccard distance $\ge 0.92$)
* **Editorial Stance Analysis** (`reports`, `endorses`, `debunks`, `mentions`)
* **Cryptographic Tamper-Evident Audit Trail** (SHA-256 hash chains)
* **Localized Credibility Judge** with dual-source corroboration metrics
* **Sovereign In-App Tokenomics (Bald$)** with VIP voucher activation

> *„Investigate the claim and the narrative, never dox the private individual.”*

---

## Key Features

### 1. Multi-Provider Sovereign Crawling
* **Keyless Live Web Crawler:** Native DuckDuckGo live extractor without API keys or accounts.
* **Aggregated Federated Search:** SearXNG JSON API, Brave Web Search API, and Wikipedia (PL & EN).
* **Safe Content Extraction:** Built-in `bse-fetch` engine with strict private IP/SSRF filtering, canonical URL normalization, tracker parameter stripping, and robots.txt compliance.

### 2. Evidence Pipeline & Citation Gate
* **Strict Citation Verification:** An AI model cannot hallucinate facts into your database. A claim is only recorded in the ledger if accompanied by a verbatim quote ($\ge 24$ characters) proven to exist in the source document.
* **Near-Duplicate Suppression:** MinHash / Locality-Sensitive Hashing (LSH) merges mirrors and syndicated copies while preserving unique investigative angles.
* **Balanced Information Diversity:** Shannon entropy metric ensures query results do not collapse into single-domain echo chambers.

### 3. Privacy & Person Guard
* **Fail-Closed Guard:** Deterministic rules in Polish and English, leetspeak de-obfuscation, and PII filters.
* **Public Figure Protection vs. Private Individuals:** Respects `EntityKind::PublicActor` (historians, politicians, authors, whistleblowers) while strictly rejecting private doxxing (national IDs, private telephone numbers, residential addresses).
* **100% Local Storage:** SQLite with WAL mode stored exclusively on your machine (`~/.local/share/bald/bald.db`). No telemetry, no cloud relays, no tracking.

### 4. Interactive Web & Desktop Shell
* **Single Self-Contained Binary:** The React / Tailwind UI is compiled directly into the native Rust binary via `include_dir`.
* **Zero Dependency Web Server:** Run `bald serve` to launch the local web interface on `http://127.0.0.1:7333` protected by anti-CSRF session tokens.
* **Cross-Platform Native Desktop Shell:** Built with Tauri 2 for Windows (`.exe` / `.msi`), macOS (`.dmg`), and Linux (`.AppImage` / `.deb`).

---

## Architecture

```
                          ┌────────────────────────┐
                          │   BALD UI (React/TS)   │
                          └───────────┬────────────┘
                                      │ IPC / REST
┌─────────────────────────────────────▼─────────────────────────────────────┐
│                          BALD Core (Rust Workspace)                       │
│                                                                           │
│  ┌───────────────┐     ┌────────────────┐     ┌────────────────────────┐  │
│  │   bse-guard   │ ──> │ bse-providers  │ ──> │       bse-fetch        │  │
│  │ (Safety/PII)  │     │(DDG/SearX/Wiki)│     │    (SSRF & Parser)     │  │
│  └───────────────┘     └────────────────┘     └───────────┬────────────┘  │
│                                                           │               │
│  ┌───────────────┐     ┌────────────────┐                 ▼               │
│  │   bse-store   │ <── │  bse-pipeline  │ <─── ┌──────────────────────┐   │
│  │ (SQLite & WAL)│     │(Citation Gate) │      │     bse-classify     │   │
│  └───────────────┘     └────────────────┘      │ (Credibility Judge)  │   │
│                                                └──────────────────────┘   │
└───────────────────────────────────────────────────────────────────────────┘
```

---

## Installation

### Option 1: Native Desktop Installers (Recommended)

Download ready-to-run signed installers from the official [GitHub Releases](https://github.com/jarohull-ai/BALD-ENGINE/releases):

| OS | Format | Download |
|---|---|---|
| **Windows** | Setup `.exe` (NSIS) | [BALD_0.1.0_x64-setup.exe](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD_0.1.0_x64-setup.exe) |
| **Windows** | System `.msi` | [BALD_0.1.0_x64_en-US.msi](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD_0.1.0_x64_en-US.msi) |
| **macOS** | Apple Silicon `.dmg` | [BALD_0.1.0_aarch64.dmg](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD_0.1.0_aarch64.dmg) |
| **Linux** | Universal `.AppImage` | [BALD_0.1.0_amd64.AppImage](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD_0.1.0_amd64.AppImage) |
| **Linux** | Debian / Ubuntu `.deb` | [BALD_0.1.0_amd64.deb](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD_0.1.0_amd64.deb) |
| **Linux** | Fedora / RPM `.rpm` | [BALD-0.1.0-1.x86_64.rpm](https://github.com/jarohull-ai/BALD-ENGINE/releases/download/v0.1.0/BALD-0.1.0-1.x86_64.rpm) |

### Option 2: Cargo (From Source or Crates.io)

Install with the Rust toolchain directly from GitHub:
```bash
cargo install --git https://github.com/jarohull-ai/BALD-ENGINE bse-cli
```
Or via crates.io:
```bash
cargo install bald-engine
```

---

## Quickstart

### Launch Web Interface
```bash
bald serve
```
Open `http://127.0.0.1:7333` in your browser. The embedded UI loads instantly without external node services.

### CLI Search
Perform an interactive query with live web deduplication and evidence parsing:
```bash
bald search "evidence of corporate deplatforming" --limit 10
```

### Inspect Audit Trail
Verify the integrity of the cryptographic hash chain:
```bash
bald audit verify
```

---

## Sovereign Credit Wallet (Bald$)

Each fresh installation starts with **1,000.00 Bald$** (~4,000 deep research queries at 0.25 Bald$ each). 

The platform supports offline single-use VIP voucher redemption powered by cryptographic SHA-256 hashes, granting researchers and partner organizations dedicated query allotments without third-party billing servers.

---

## Repository Structure

* `crates/bse-core`: Shared domain types, evidence models, and gate contracts.
* `crates/bse-fetch`: Safe HTTP fetching engine with SSRF guard and robots.txt caching.
* `crates/bse-guard`: Person guard, query sanitizer, and PII detector.
* `crates/bse-providers`: DuckDuckGo, SearXNG, Brave, and Wikipedia search adapters.
* `crates/bse-pipeline`: Reciprocal Rank Fusion, MinHash near-duplicate detector, and citation gate.
* `crates/bse-classify`: AI claim extraction, stance determination, and credibility judge heuristics.
* `crates/bse-store`: SQLite evidence ledger, migration engine, and wallet verification.
* `crates/bse-llm`: Local SLM inference (Ollama, LM Studio, OpenAI-compatible APIs).
* `crates/bse-api`: HTTP REST API, SSE endpoints, and model scout.
* `crates/bse-cli`: Unified `bald` command-line application.
* `apps/ui`: Modern React + TypeScript + Tailwind frontend.
* `apps/desktop`: Native Tauri 2 desktop shell.

---

## Test Suite

BALD maintains a 100% passing test suite across all 12 modules:
```bash
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

---

## License

Distributed under the **MIT License**. See [`LICENSE`](LICENSE) for details.

Developed with precision by **Jarosław Kuchta** / **Viper Team**.
