# bse-api

[![Crates.io](https://img.shields.io/crates/v/bse-api?style=flat-square)](https://crates.io/crates/bse-api)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Unified REST and Tauri IPC service layer for the **BALD Search Engine**.

## Features

- **Embedded Web Server**: High-performance Axum HTTP service bound to loopback.
- **Embedded UI**: Serves pre-compiled React/Tailwind web interface directly from binary memory via `include_dir`.
- **Anti-CSRF Security**: Enforces mandatory session tokens for all state-changing API endpoints.
- **Credibility Judge**: Calculates multi-source corroborated credibility scores and follow-up investigative angles.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
