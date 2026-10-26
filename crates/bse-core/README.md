# bse-core

[![Crates.io](https://img.shields.io/crates/v/bse-core?style=flat-square)](https://crates.io/crates/bse-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Shared core domain types, provider contracts, and safety gate definitions for the **BALD Sovereign OSINT Search Engine**.

## Features

- **Domain Models**: Types for search queries, hits, documents, claims, evidence items, and stance classifications.
- **Provider Contract**: Async traits (`SearchProvider`) for plugging in search backends (DuckDuckGo, SearXNG, Brave, Wikipedia).
- **Safety Gate Interfaces**: Guard decisions (`Allow`, `Review`, `Refuse`) for deterministic query routing.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
