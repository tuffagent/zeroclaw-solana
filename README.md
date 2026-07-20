# ZeroClaw Solana Plugins

Two crates submitted together against the Superteam Brasil "Build Solana-native
plugins for Zeroclaw" bounty:

- [`crates/solana-core`](./crates/solana-core/README.md) — a dependency-free,
  `wasm32-wasip2`-friendly Solana JSON-RPC client, base58 codec, SPL
  Token/Token-2022 mint parser, and risk-scoring model. Track E submission.
- [`plugins/token-risk-check`](./plugins/token-risk-check/README.md) — a
  ZeroClaw WIT tool plugin built on `solana-core`: assesses rug/custody risk
  for a Solana mint. Track D submission. Custody tier **T0 (Read)**.

See each crate's own README for details.
