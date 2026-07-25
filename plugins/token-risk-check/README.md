# token-risk-check

A ZeroClaw tool plugin: assesses rug/custody risk for a Solana SPL Token or
Token-2022 mint. **Custody tier: T0 (Read).** No signing, no transaction
building, no private key anywhere in this plugin — the worst case of a bug
here is a misleading verdict, never fund loss.

## What it checks

- Whether `mint_authority`/`freeze_authority` are present or renounced.
- Dangerous Token-2022 extensions: `permanent_delegate`, `transfer_hook`,
  `transfer_fee_config`, `default_account_state_frozen`,
  `non_transferable`, `confidential_transfer`.
- Holder concentration from `getTokenLargestAccounts` (top 1/5/10/20 as a
  percentage of total supply). This RPC method returns at most the top 20
  holders, so a **low** reading means little (holders 21+ are invisible to
  this tool); a **high** reading is a hard floor on true concentration.
- Optionally, whether the mint appears in Jupiter's public token list
  (informational only, never fed into the score).

These combine into a single 0-100 score via a noisy-OR / competing-risks
model (one catastrophic signal, e.g. `permanent_delegate`, dominates the
score regardless of otherwise-clean signals; several independently
moderate signals still compound). Full derivation in the design spec.

**Deliberate design bias:** this tool flags toward false positives. A
legitimately locked-LP address can trigger high concentration; a
legitimate compliance token can trigger `freeze_authority`. Both should
still land amber/red for a human or the calling agent to look at, because
this is a read-only advisory a human reviews, not a gate that blocks
funds.

## Threat model

1. No private key ever enters this plugin.
2. `http_client` is granted at the sandbox level as all-or-nothing; this
   plugin's own code only ever calls the configured Solana RPC, the
   Jupiter token-list endpoint, and (only if `ollama_enabled`) a
   local/configured Ollama server. That is a code-level promise, stated
   here, not a sandbox-enforced allowlist.
3. **This plugin never fetches token metadata (name/symbol).** That field
   is the one place an attacker fully controls the content, and leaving it
   out of the data path removes the only realistic prompt-injection
   channel into either the score or the Ollama narration prompt,
   structurally, not by filtering. See `prompt-injection-test.md` for the
   concrete test and transcript.
4. Ollama narration (`summary` field) is a side channel: any failure or
   unparseable response falls back to a deterministic templated sentence.
   It never alters `score`/`verdict`/`factors`, which are computed first
   and independently.
5. A broken/unreachable RPC returns `success: false` with a message the
   calling agent can react to (retry, try another RPC) — never a hard
   `Err`, since that's an infrastructure condition, not a plugin fault.

## Config keys (`__config`, all optional)

| Key | Default | Purpose |
|---|---|---|
| `rpc_url` | `https://api.mainnet-beta.solana.com` | Solana JSON-RPC endpoint — supply your own; the default has strict rate limits |
| `amber_threshold` | `25` | Score at/above which the verdict is amber |
| `red_threshold` | `60` | Score at/above which the verdict is red |
| `check_liquidity` | `true` | One extra call to Jupiter's token list |
| `ollama_enabled` | `false` | Gate for local-LLM narration (see below) |
| `ollama_endpoint` | `http://localhost:11434` | Local Ollama server |
| `ollama_model` | `qwen2.5:0.5b` | Kept deliberately small — this is a one-paragraph summary, not a reasoning task |

## Worked example

See `demo-transcripts.md` for real mainnet and devnet Token-2022
invocations, and `prompt-injection-test.md` for the mandatory
fail-closed proof.

## Testing

```bash
cargo test --lib                              # pure logic, mocked RPC, no network
cargo build --target wasm32-wasip2 --release  # produces token_risk_check.wasm
```
