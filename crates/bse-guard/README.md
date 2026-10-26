# bse-guard

[![Crates.io](https://img.shields.io/crates/v/bse-guard?style=flat-square)](https://crates.io/crates/bse-guard)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Deterministic query safety, anti-doxxing guard, and cryptographic audit log for the **BALD Engine**.

## Features

- **Person Guard**: Fail-closed heuristic filter distinguishing investigative topics from private individual targeting.
- **Leetspeak De-obfuscation**: Normalizes character substitutions to prevent filter bypass.
- **Cryptographic Audit Trail**: SHA-256 hash chains verifying search query ledger integrity without storing raw PII.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
