# bse-config

[![Crates.io](https://img.shields.io/crates/v/bse-config?style=flat-square)](https://crates.io/crates/bse-config)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Configuration management for the **BALD Sovereign OSINT Search Engine**.

## Features

- **Hierarchical Loading**: Reads from `~/.config/bald/config.toml` with graceful fallback defaults.
- **Environment Overrides**: Override settings via `BALD_*` environment variables.
- **Path Expansion**: Safe home directory (`~`) resolution across Windows, macOS, and Linux.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
