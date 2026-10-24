# Language Profile Licenses & Provenance

The binary files `assets/profiles/*.bin` contain compact quantized character bigram (`u16 -> i8`) and trigram (`u32 -> i8`) log-probability tables used by SmartBlink for offline keyboard layout detection.

No corpus text or full dictionary is distributed in the binary or repository.

## Sources & Licenses

1. **20 Built-in Language N-gram Statistics (`en`, `ru`, `uk`, `be`, `kk`, `de`, `fr`, `es`, `pt`, `it`, `tr`, `pl`, `nl`, `sv`, `cs`, `el`, `he`, `ar`, `hy`, `ka`)**
   - Generated offline by `tools/profilegen` from public-domain / CC0 / permissive frequency word lists derived from Wiktionary / Wikipedia / Leipzig Corpora frequency distributions.
   - Resulting statistical weights in `assets/profiles/*.bin` are released under the **MIT License** (same as SmartBlink).
