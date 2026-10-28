# Language Profile & Third-Party Licenses

## 1. SmartBlink Source Code & Language Profiles (`MIT License`)

All SmartBlink source code, binary assets, and the 20 built-in language n-gram profile binaries (`assets/profiles/*.bin` for `en`, `ru`, `uk`, `be`, `kk`, `de`, `fr`, `es`, `pt`, `it`, `tr`, `pl`, `nl`, `sv`, `cs`, `el`, `he`, `ar`, `hy`, `ka`) are licensed under the **MIT License** (`Copyright (c) 2026 VKV Studio`, see [`../../LICENSE`](../../LICENSE)).

- The binary files `assets/profiles/*.bin` store compact quantized character bigram (`u16 -> i8`) and trigram (`u32 -> i8`) log-odds tables generated offline by `tools/profilegen`.
- The training word lists in `tools/profilegen/src/corpora.rs` consist solely of uncopyrightable core vocabulary (common pronouns, prepositions, conjunctions, and everyday base words in each of the 20 languages) compiled specifically for SmartBlink and released under the **MIT License**. No copyleft (GPL / CC-BY-SA) datasets are included.

## 2. Third-Party Rust Dependencies

SmartBlink uses a single runtime dependency crate:

- **`windows` (`v0.58`)** — Microsoft Corporation  
  Licensed under **MIT OR Apache-2.0** ([https://github.com/microsoft/windows-rs](https://github.com/microsoft/windows-rs)).

