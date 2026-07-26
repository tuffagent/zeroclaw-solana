# solana-core

A dependency-free, `wasm32-wasip2`-friendly Solana toolkit for ZeroClaw WIT
plugins: base58, a minimal JSON-RPC client behind a mockable `RpcTransport`
trait, SPL Token / Token-2022 mint parsing, and a noisy-OR risk-scoring
model. Deliberately not `solana-sdk`/`solana-client`/`bs58` — those don't
build cleanly for this target; see the bounty listing's own warning on this.

## Why a transport trait

`RpcTransport` is the only seam between this crate's logic and actual
network I/O. Inject `transport::MockTransport` for host tests (`cargo test`,
no wasm toolchain, no live network — this crate's own tests do exactly
that); inject `transport::WakiTransport` (only compiled for
`wasm32-wasip2`) inside an actual plugin component. This is what lets a
plugin author literally `solana_core::rpc::get_account_info(&WakiTransport, ...)`
and get a working Solana RPC call inside a WIT component, which is the
concrete thing this crate is trying to hand the next plugin author.

## Modules

- `b58` — base58 encode/decode.
- `transport` — the `RpcTransport` trait, `MockTransport`/`FailingTransport`
  (behind `test-support`), and `WakiTransport` (wasm-only).
- `rpc` — `get_account_info`, `get_token_largest_accounts`, and a minimal
  base64 decoder.
- `mint` — SPL Token / Token-2022 mint account parsing: authorities and the
  six Token-2022 extensions the risk model cares about.
- `risk` — the noisy-OR scoring model. `score` takes its concentration
  argument as an `Option`, because `getTokenLargestAccounts` is an
  expensive scan that public endpoints refuse outright: passing `None`
  omits that factor rather than substituting an invented probability, and
  floors the verdict at amber so a partial reading can never return green.
  See `docs/superpowers/specs/2026-07-20-zeroclaw-solana-plugin-design.md`
  in the parent workspace for the full derivation.

## Using it from another plugin

```toml
[dependencies]
solana-core = { path = "../../crates/solana-core" }

[dev-dependencies]
solana-core = { path = "../../crates/solana-core", features = ["test-support"] }
```

## Testing

```bash
cargo test --lib                        # pure logic, mocked RPC, no network
cargo build --target wasm32-wasip2 --release   # confirms the wasm-gated WakiTransport compiles
```
