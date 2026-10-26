# bse-store

[![Crates.io](https://img.shields.io/crates/v/bse-store?style=flat-square)](https://crates.io/crates/bse-store)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

SQLite-backed evidence ledger, search history, and sovereign credit wallet for the **BALD Engine**.

## Features

- **Evidence Ledger**: Cryptographic storage of verified claims, source documents, entities, and citations.
- **High Concurrency**: SQLite with Write-Ahead Logging (WAL) and automatic migrations.
- **Sovereign Credit Wallet**: Rate-limiting tokenomics (Bald$) and cryptographic SHA-256 VIP voucher redemption.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
