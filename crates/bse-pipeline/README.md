# bse-pipeline

[![Crates.io](https://img.shields.io/crates/v/bse-pipeline?style=flat-square)](https://crates.io/crates/bse-pipeline)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Search fusion, deduplication, and evidence validation pipeline for the **BALD Engine**.

## Features

- **Reciprocal Rank Fusion (RRF)**: Merges multi-provider search results using $k=60$ harmonic scoring.
- **Near-Duplicate Suppression**: MinHash / LSH (64 hashes, 16 bands, 0.90 similarity threshold) to merge mirrors.
- **Citation Gate**: Verifies that extracted claims have verbatim textual grounding in the source document.
- **Information Diversity**: Shannon entropy measurement across result domains.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
