# bse-llm

[![Crates.io](https://img.shields.io/crates/v/bse-llm?style=flat-square)](https://crates.io/crates/bse-llm)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg?style=flat-square)](LICENSE)

Local and private SLM/LLM inference client for the **BALD OSINT Engine**.

## Features

- **Local Inference**: Seamless connection to Ollama (`/api/chat`) and LM Studio / OpenAI-compatible endpoints.
- **Security Guardrails**: Enforces HTTPS for non-loopback endpoints, rejects embedded URL credentials, and redacts errors.
- **Embedded Catalogs**: Built-in model presets optimized for evidence extraction and factual synthesis.

## License

Licensed under the MIT License. Part of the [BALD Engine](https://github.com/jarohull-ai/BALD-ENGINE).
