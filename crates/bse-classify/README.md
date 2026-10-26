# bse-classify

[![Crates.io](https://img.shields.io/crates/v/bse-classify?style=flat-square)](https://crates.io/crates/bse-classify)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Claim extraction, entity recognition, and source stance classification for the **BALD Engine**.

## Features

- **Structured Claim Extraction**: Identifies factual claims, verbatim quotes, and publishing stance (`reports`, `endorses`, `debunks`, `mentions`).
- **Entity Classification**: Preserves `EntityKind::PublicActor` (politicians, researchers, authors) while strictly scrubbing private PII.
- **Deterministic Fallback**: Built-in heuristic extraction engine (`heuristic-v1`) when LLMs are unavailable or unresponsive.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
