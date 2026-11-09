# Soroban contracts

This workspace contains the Stage 2 protocol foundation: `participant_registry`, `product_batch_registry`, and `order_escrow`. All crates use the exact shared `soroban-sdk = 28.0.0` pin. The repository root pins Rust 1.99.1 and `wasm32v1-none`; the exact CLI used for the Wasm build is recorded in [`../scripts/stellar-cli-version`](../scripts/stellar-cli-version) (30.0.1).

The contracts require authorization for actor-controlled state changes, use checked arithmetic, emit events, and avoid collection scans or unbounded loops. They apply finite per-record and lifetime bounds: batch IDs are at most 64 bytes, orders at most 64 checkpoints each, 10,000 participant registrations, 10,000 batch registrations, and 100,000 lifetime orders. Persistent entries extend TTL on access from a 100,000-ledger threshold to 500,000 ledgers. See the [architecture notes](../docs/architecture.md#storage-and-ttl) and [threat model](../docs/threat-model.md#implementation-review) for archival and operational limitations.

**Unaudited and testnet-only. Do not use real funds or regulated shipments.** The deployment script sends public testnet transactions, requires existing funded CLI identities, and creates a test credit asset; it does not generate keys.

## Toolchain and commands

The root [`rust-toolchain.toml`](../rust-toolchain.toml) pins Rust and the Wasm target. The shared SDK version and `Cargo.lock` pin Rust dependencies.

Run from the repository root:

```sh
cargo fmt --manifest-path contracts/Cargo.toml --all -- --check
cargo clippy --manifest-path contracts/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path contracts/Cargo.toml --workspace
stellar contract build --manifest-path contracts/Cargo.toml --locked
```

`stellar contract build` targets `wasm32v1-none`, applies the release profile, and emits optimized Wasm artifacts under `contracts/target/wasm32v1-none/release/`. A local build does not deploy a contract or contact testnet.

## `participant_registry`

The contract stores one role per Stellar address (`Lab`, `Supplier`, `Logistics`, `Attestor`) and a verifier-issued licence attestation containing a 32-byte document hash, expiry timestamp, and revocation state. The admin controls roles and verifier rotation; admin transfer is two-step, and only the pending admin may accept. A participant is verified only while it has a role and an unrevoked, unexpired attestation. Lifetime registration is capped at 10,000 records.

Public functions: `initialize`, `admin`, `verifier`, `set_verifier`, `propose_admin`, `accept_admin`, `register_participant`, `remove_participant`, `get_role`, `attest_licence`, `revoke_licence`, `is_verified`, and `licence_document_hash`.

## `product_batch_registry`

A currently verified Supplier registers a unique batch ID, product-specification hash, storage range in tenths of °C, expiry timestamp, and available quantity. `is_fulfillable(batch_id, quantity)` checks the supplier's current verification, batch expiry, valid temperature range, nonzero requested quantity, and sufficient stock. Only the configured order-escrow address may reserve, release, or finalize stock. `fulfill_reserved` consumes reserved stock; it never returns that quantity to availability. Lifetime registration is capped at 10,000 batches.

Public functions: `initialize`, `set_escrow`, `register_batch`, `get_batch`, `order_terms`, `is_fulfillable`, `reserve_batch`, `release_batch`, and `fulfill_reserved`.

## `order_escrow`

A registered Lab orders against a fulfillable batch, names a registered logistics provider, and funds escrow using the SEP-41 token interface (including a Stellar Asset Contract). A registry-attested Supplier accepts and packs; the named Carrier marks transit and delivery. A role-authorised Attestor signs numbered checkpoints. Orders have a 100,000 lifetime cap, and each order stores at most 64 immutable checkpoints, rejects sequence gaps/replays and future or decreasing timestamps, and flags readings outside the batch storage range.

An unresolved excursion blocks Lab confirmation and delivered-timeout settlement. The configured Arbiter may accept the excursion or move the order into dispute. The Lab, Supplier, or Carrier may raise a dispute from `Accepted`, `Packed`, `InTransit`, or `Delivered`. Arbiter resolution uses a Supplier share of 0–10,000 basis points; the remainder is returned to the Lab. Arbiter resolution ends in `Resolved`, distinct from `Confirmed`.

Lab-initiated `confirm_delivery` requires full-quantity `Acceptable` proof and no unresolved excursion; it stores one immutable proof and pays the Supplier. Separately, expiry of the 72-hour dispute window on a `Delivered` order pays the Supplier and fulfills the reserved quantity without creating a Lab-signed proof record; the order enters terminal `Confirmed` and the timeout event identifies this path. An arbiter can also make an explicit split payout from `Disputed`. There is no carrier fee or platform fee in this stage.

### Order transition table

`Created` is an event-only pre-state; stored orders begin in `Placed`. Timeouts may be submitted by any signing address. All named parties must both authorize and match the order record.

| From → To | Function and actor | Condition / effect |
|---|---|---|
| `Created → Placed` | `create_order`; registered Lab | Verified Supplier and eligible Carrier; sufficient unexpired batch; escrow funded and stock reserved. |
| `Placed → Accepted` | `accept`; named Supplier | Supplier accepts the order. |
| `Accepted → Packed` | `mark_packed`; named Supplier | Order is packed. |
| `Packed → InTransit` | `mark_in_transit`; named Carrier | Shipment enters transit. |
| `InTransit → Delivered` | `mark_delivered`; named Carrier | Delivery timestamp is recorded. |
| `Delivered → Confirmed` | `confirm_delivery`; named Lab | Full quantity, `Acceptable` condition, and no unresolved excursion; immutable proof stored and Supplier paid. |
| `Placed → Cancelled` | `cancel_before_acceptance`; named Lab | Refund Lab and release reserved quantity. |
| `Placed → Cancelled` | `timeout_order`; any signer at 48 hours after creation | Refund Lab and release reserved quantity. |
| `Accepted` or `Packed → Cancelled` | `timeout_order`; any signer at 7 days after acceptance | Supplier has not marked transit; refund Lab and release reserved quantity. |
| `InTransit → Disputed` | `timeout_order`; any signer at 7 days after acceptance | Keep funds escrowed and quantity reserved for Arbiter resolution; do not refund automatically. |
| `Delivered → Confirmed` | `timeout_order`; any signer at 72 hours after delivery | Only if no unresolved excursion; use successful settlement to pay Supplier and fulfill reserved quantity. No Lab-signed proof record is created; the timeout event distinguishes this settlement. |
| `Accepted`, `Packed`, `InTransit`, or `Delivered → Disputed` | `raise_dispute`; Lab, named Supplier, or named Carrier | Escalates to arbiter; no automatic payout. |
| `Disputed → Resolved` | `resolve_dispute`; configured Arbiter | Pays the selected split once; zero Supplier share releases inventory, nonzero share finalizes it. |

A checkpoint does not itself change state. `submit_checkpoint` is allowed in `Accepted`, `Packed`, `InTransit`, and `Delivered`; it requires the Attestor role, the exact next sequence, a nondecreasing timestamp no later than ledger time, and minimum temperature ≤ maximum. Any reading outside the registered range sets `excursion_unresolved`. The Arbiter may clear the flag as acceptable without a state change, or mark it unacceptable and move the order to `Disputed` unless it is already disputed. `Confirmed`, `Resolved`, and `Cancelled` are terminal. Repeat payout or terminal-transition attempts fail.

### Deterministic timeout windows

Timeouts use Stellar ledger timestamps in seconds:

- An unaccepted `Placed` order refunds after 48 hours from creation.
- `Accepted` or `Packed` refunds after 7 days from acceptance if transit has not begun, and releases reserved quantity.
- `InTransit` moves to `Disputed` after 7 days from acceptance; funds and reserved quantity remain pending Arbiter resolution.
- `Delivered` settles to the Supplier after 72 hours if it has not been confirmed or disputed and has no unresolved excursion. This timeout-derived `Confirmed` state does not contain a Lab-signed proof record; the Lab had the window to dispute, and a timeout event records the automatic settlement.
- An unresolved excursion blocks delivered-timeout settlement. A disputed order has no automatic timeout; the Arbiter must resolve it. Arbiter unavailability is a known stuck-funds risk.

Public functions: `initialize`, `set_arbiter`, `create_order`, `get_order`, `accept`, `mark_packed`, `mark_in_transit`, `mark_delivered`, `submit_checkpoint`, `get_checkpoint`, `confirm_delivery`, `get_proof`, `raise_dispute`, `resolve_excursion`, `resolve_dispute`, `cancel_before_acceptance`, and `timeout_order`.

## Boundaries and limitations

The contracts trust authorised attestors, not raw sensors, and cannot validate a licence, product, shipment, or temperature log in the real world. Public addresses, batch IDs, payment asset/amount, and checkpoint summaries are visible on-ledger. Detailed logs and all patient/personal data must remain off-chain; this implementation has no API or file store. Persistent entries can be archived after long inactivity, and there is no recovery/restore workflow in this stage. See the [threat model](../docs/threat-model.md) for the review checklist and residual risks.

## Testnet deployment

[`../scripts/deploy-testnet.sh`](../scripts/deploy-testnet.sh) builds and deploys the three contracts, issues and deploys a test credit asset, initializes roles/configuration, and saves only public contract and asset IDs to the ignored `testnet-addresses.env` file. It requires existing CLI identities provided by environment variables and funded testnet accounts. The script has **not** been run against testnet; no contract addresses or network results are claimed.

## Sources verified for SDK/CLI usage

- [Stellar: Write, Test, and Deploy a Rust Smart Contract](https://developers.stellar.org/docs/build/smart-contracts/getting-started/hello-world) — SDK 28 workspace setup and `stellar contract build`.
- [Stellar Rust SDK documentation](https://developers.stellar.org/docs/tools/sdks/contract-sdks) — released Soroban Rust SDKs.
- [Stellar state archival](https://developers.stellar.org/docs/learn/fundamentals/contract-development/storage/state-archival) — persistent-entry TTL behavior.
- [Stellar token interface](https://developers.stellar.org/docs/tokens/token-interface) — SEP-41 token calls.
- [Stellar CLI testnet deployment](https://developers.stellar.org/docs/build/smart-contracts/getting-started/deploy-to-testnet) — deployment/invocation workflow.
- [Deploy a Stellar Asset Contract](https://developers.stellar.org/docs/tools/cli/cookbook/deploy-stellar-asset-contract) — test credit-asset contract setup.
