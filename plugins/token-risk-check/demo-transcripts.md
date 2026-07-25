# Live verification transcripts

Everything below was produced by running the compiled `token_risk_check.wasm` through a real
`zeroclaw` host against live Solana RPC. Nothing here is hand-written, reconstructed, or
illustrative: each block is the exact output of a CI step, and each can be reproduced by
dispatching the `Verify zeroclaw host` workflow in this repository.

## Provenance

| | |
|---|---|
| Workflow run | [30178582595](https://github.com/tuffagent/zeroclaw-solana/actions/runs/30178582595) |
| Plugin commit | `68adca3` |
| Host | `zeroclaw-labs/zeroclaw` at `4ae0d75`, the commit `wit/UPSTREAM_REF` pins |
| Host build | `cargo build --release --features plugins-wasm,plugins-wasm-cranelift` |
| Plugin build | `cargo build --target wasm32-wasip2 --release` |
| RPC | Helius mainnet and devnet, keyed from a repository secret |

The RPC URLs carry an API key, so they are redacted here as `$MAINNET_RPC` and `$DEVNET_RPC`.
No key appears in this repository or in any transcript.

### Why a harness rather than the CLI

`zeroclaw` has no subcommand that invokes a single tool: `plugin` offers list, search, install,
remove, info, and migrate, and the only sanctioned way to reach a tool is a full agent
conversation in which a model decides to call it. That is not a thing to hang deterministic
verification on. `tools/live-verify` therefore drives the same host-side path the agent does,
`WasmTool::from_wasm` then `WasmTool::execute`, with nothing stubbed. Its whole source is
82 lines, in `tools/live-verify/src/main.rs`.

`from_wasm` probes the component's own `tool` export for its name, description, and parameter
schema, and the current host rejects a component it cannot instantiate rather than registering
it with synthetic metadata. So the harness printing a real description is itself evidence the
component instantiated and answered.

## Registration in a real host

```
$ zeroclaw config set plugins.enabled true
plugins.enabled updated.

$ zeroclaw plugin list
Installed plugins:
  token-risk-check v0.1.0 - Rug/custody risk assessment for a Solana SPL Token or Token-2022 mint: authorities, dangerous extensions, holder concentration. Read-only (T0).
```

## Transcript 1 - mainnet PYUSD, a full three-signal reading

PayPal USD, `2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo`, a live mainnet Token-2022 mint.
Run with `ollama_enabled=true` against `qwen2.5:0.5b` on plain loopback.

```
$ TRC_OLLAMA_ENABLED=true TRC_RPC_URL="$MAINNET_RPC" \
    live-verify 2b1kV6DkPAnxd5ixfnxCpjxmKwqjjaYmCZfHsFu24GXo
registered as: token_risk_check (Assess the rug/custody risk of a Solana SPL Token or Token-2022 mint: ...)
```

```json
{
  "context": {
    "decimals": 6,
    "total_supply": "680616050875521"
  },
  "factors": {
    "concentration": {
      "top10_pct": 83.48453804229496,
      "top1_pct": 56.65039837918263,
      "top20_pct": 92.56943591186383,
      "top5_pct": 69.87277255072067
    },
    "extensions": {
      "confidential_transfer": true,
      "default_account_state_frozen": false,
      "non_transferable": false,
      "permanent_delegate": true,
      "transfer_fee_config": true,
      "transfer_hook": true
    },
    "freeze_authority": true,
    "mint_authority": true
  },
  "liquidity": "unknown",
  "score": 99.7093,
  "summary": "This Solana token mint has a red score due to high concentrations of transfer and non-transferable tokens, indicating significant volatility in the market.",
  "verdict": "red",
  "warnings": []
}
```

Four dangerous extensions are live on this mint at once - a permanent delegate, a transfer
hook, a transfer fee, and confidential transfer - on top of both authorities still being held
and the top holder sitting on 56.65% of supply. The noisy-OR combination puts it at 99.7093,
red. The concentration figures agree to fourteen decimal places with an independent query made
from a different machine, which is the cross-check that the plugin is reading the chain rather
than reporting something of its own invention.

A red verdict on PYUSD is not the tool crying wolf, and it is worth being plain about what it
means. A regulated stablecoin holds exactly these powers deliberately, and its issuer being
able to freeze and claw back tokens is the product working as designed. The tool's job is to
say what an address can do to a holder, not to guess whether it will - and it flags toward
false positives on purpose, because a T0 advisory a human reads is the cheap place to be wrong.

The `summary` line is model prose, and it is the answer to the one open question the design
carried: `waki` can reach `localhost:11434` from inside the sandbox, through the host, over
`wasi:http`. It is also wrong, in a way worth keeping on the record. `non_transferable` is
`false` on this mint, and "volatility in the market" is invented outright, since the model was
given no price data and asked for none. The score and the verdict were both fixed before any
model saw anything. That is the separation the design assumes, here observed rather than
argued: it is why a 0.5b model is safe to let loose on the narration, and why a garbled or
adversarial summary cannot move a number.

## Transcript 2 - mainnet USDC, a holder scan that cannot be run

USDC, `EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v`, has on the order of ten million holders.
`getTokenLargestAccounts` is a scan over every token account for a mint, and at that size no
provider will run it - this is a refusal on scale, not on rate, so no API key and no amount of
backoff makes it go away.

```
$ TRC_RPC_URL="$MAINNET_RPC" live-verify EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v
```

```json
{
  "context": {
    "decimals": 6,
    "total_supply": "7695296870199498"
  },
  "factors": {
    "concentration": null,
    "extensions": {
      "confidential_transfer": false,
      "default_account_state_frozen": false,
      "non_transferable": false,
      "permanent_delegate": false,
      "transfer_fee_config": false,
      "transfer_hook": false
    },
    "freeze_authority": true,
    "mint_authority": true
  },
  "liquidity": "unknown",
  "score": 23.5,
  "summary": "Automated summary unavailable; deterministic risk score is 23.5 (amber). See the factors field for the underlying signals.",
  "verdict": "amber",
  "warnings": [
    "holder concentration unavailable: RPC error: {\"code\":-32600,\"message\":\"Too many accounts requested (10000000 pubkeys), try adding filters to narrow down results\"}"
  ]
}
```

This is the degraded path, on a real mint, refused for real reasons. Three things to note.

The authority and extension findings survive: they came from an ordinary `getAccountInfo`,
which answered perfectly well, and there is no reason to throw them away because a second call
failed. `concentration` is `null` rather than zeroes, because a caller must be able to tell an
unmeasured signal from a mint whose top holders genuinely hold nothing. And the warning carries
the cause verbatim, including the provider's own error, so an operator can tell in one glance
whether to point the plugin at a different endpoint.

Then the part that matters. The score is 23.5, and the default amber threshold is 25 - so on
the numbers alone this would have come back **green**. It did not. A reading missing one of its
three signals may not make this tool's one affirmative claim, so the verdict is floored at
amber and sent to a human. No probability was invented for the missing signal to force that
outcome; the score stays an honest reading of exactly what was measured, and the uncertainty is
carried by the verdict where it belongs. CI asserts all four of these properties on every run
rather than leaving them to be eyeballed.

## Transcript 3 - devnet, a permanent delegate

`FEBXiyu9QuByQGiMNzbkd3dkfX8pJfbYA3g1SPEYvjz2`, a devnet Token-2022 mint carrying the single
most dangerous extension in the set: a permanent delegate can move any holder's tokens at any
time, without their signature.

```
$ TRC_RPC_URL="$DEVNET_RPC" live-verify FEBXiyu9QuByQGiMNzbkd3dkfX8pJfbYA3g1SPEYvjz2
```

```json
{
  "context": {
    "decimals": 9,
    "total_supply": "0"
  },
  "factors": {
    "concentration": {
      "top10_pct": 0.0,
      "top1_pct": 0.0,
      "top20_pct": 0.0,
      "top5_pct": 0.0
    },
    "extensions": {
      "confidential_transfer": false,
      "default_account_state_frozen": false,
      "non_transferable": false,
      "permanent_delegate": true,
      "transfer_fee_config": false,
      "transfer_hook": false
    },
    "freeze_authority": false,
    "mint_authority": true
  },
  "liquidity": "unknown",
  "score": 91.75500000000001,
  "summary": "Automated summary unavailable; deterministic risk score is 91.8 (red). See the factors field for the underlying signals.",
  "verdict": "red",
  "warnings": []
}
```

One catastrophic signal against an otherwise unremarkable mint, and the score is 91.755, red.
This is the case that justifies noisy-OR over a plain maximum: the permanent delegate alone
carries `p=0.90`, and a live mint authority compounds it rather than being shadowed by it.

Note the honest zeroes. Supply is genuinely `0` on this test mint, so the concentration figures
really are `0.0` and are reported as measured values, not as `null`. That is the distinction
transcript 2 turns on, visible here from the other side.

## What these establish

- The component instantiates in a current `zeroclaw` host and registers with its own metadata.
- It reads live mainnet and devnet state, and its numbers reproduce against an independent query.
- Token-2022 extension parsing works against real mints, including four extensions on one mint.
- Loopback narration works through the host, and cannot influence a verdict.
- A partial reading degrades to amber rather than failing, and can never come back green.

The prompt-injection deliverable has its own writeup, in `prompt-injection-test.md`.
