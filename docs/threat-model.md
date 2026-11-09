# Threat Model

## Audit status

**The contracts are unaudited and testnet-only.** No production deployment is supported. Do not use real funds, real regulated shipments, or this software as the sole evidence for a medical-supply decision. An independent security audit and operational/legal review are required before any mainnet or production use.

## Scope and assets

The implemented protocol manages participant roles, supplier verification attestations, batch availability, escrowed test-token funds, order transitions, authorised temperature checkpoints, excursion flags, dispute resolution, and proof hashes. There is no API, indexer, SDK, web client, or file store in this stage.

Primary assets are escrow funds, correct fulfilment and payout state, integrity of verification and proof records, privileged keys, and the confidentiality of off-chain personal, patient, and commercial information that a future application may handle.

## Trust boundaries and assumptions

- Soroban contracts enforce protocol authorization and state invariants. A future API, indexer, or UI will not be trusted to enforce them.
- The chain trusts **authorised attestors, not raw sensors**. A signed checkpoint proves only that an authorised key attested to data; it does not prove sensor accuracy, calibration, integrity, or shipment association.
- Verifier decisions and licence evidence are external inputs. A document hash does not prove that the underlying licence is valid; expiry and revocation processes must be operated.
- Stellar transaction, address, batch identifier, checkpoint-summary, and amount data are public. Hashes can leak information if the source is guessable or too small; do not anchor sensitive plaintext or unsalted predictable values.
- The database/indexer is not implemented. Any future projection must be rebuildable and treat chain events as the authoritative source.
- Testnet assets and local Wasm builds do not establish production readiness.

## Threats and controls

| Threat | Impact | Implemented controls / remaining review focus |
|---|---|---|
| Unauthorized role, batch, attestation, or order action | Fraud, invalid inventory, or loss of funds | State-changing actor methods call `require_auth`; role, named-party, verifier, admin, escrow-address, and arbiter checks are explicit. Add broader actor-by-actor negative authorization tests. |
| Admin, verifier, or arbiter key compromise | False attestations, abusive configuration, or unresolved payouts | Privileges are separated by role/address; admin transfer is two-step; verifier attestations expire and may be revoked. There is no contract-enforced multisig or key recovery process. |
| Release with unresolved excursion or dispute | Funds released despite compromised condition | Normal confirmation checks the excursion flag; dispute settlement requires the configured arbiter; settlement transitions are terminal and state is written before external calls. |
| Reentrancy-style or cross-contract callback hazard | Duplicate or inconsistent payout/state | Soroban's execution/rollback semantics differ from EVM, but external calls occur only after the order is set terminal; failed subcalls must roll back the transaction. This assumption needs independent review. |
| Replay, stale, duplicated, or out-of-order checkpoint | Incorrect excursion/proof state | Exact next sequence is required; timestamps cannot decrease or exceed ledger time; per-order checkpoint count is capped; signer must have the Attestor role. |
| Integer, unit, time, or rounding errors | Incorrect quantities, limits, or payout | Temperatures are in tenths °C; quantities are `u64`; financial values are positive `i128`; quantities/order IDs use checked arithmetic; split is 0–10,000 basis points and remainder returns to the Lab. Timeout addition saturates rather than overflows. |
| Storage exhaustion or unbounded per-call work | Resource denial or excessive cost | No collection scans; batch IDs are limited to 64 bytes; checkpoints to 64 per order; lifetime caps are 10,000 participant registrations, 10,000 batch registrations, and 100,000 orders. Network ledger resource limits still apply. |
| TTL expiry or archived order/batch/attestation | Queries or settlement may be unavailable; funds may appear stuck | Instance and persistent reads/writes extend TTL from a 100,000-ledger threshold to 500,000 ledgers. No archival restoration flow exists; long-idle disputed orders are a known operational risk. |
| Arbiter unavailability / party silence | Escrow remains locked or order remains open | Placed, unaccepted, in-flight, and delivered states have deterministic timeout paths; unresolved excursions block delivered timeout; disputed orders have no automatic timeout and require arbiter action. |
| Temperature oracle or evidence-integrity failure | Incorrect excursion determination or condition proof | Attestors are role-authorised, readings are bounded and hashed, and the underlying logs are not on-chain. The contract cannot verify sensors, calibration, chain of custody, or log contents. |
| Hash substitution or misleading proof viewer | False evidence of delivery condition | Proof is bound to order ID and can be written only once; there is no proof viewer or off-chain canonicalization scheme yet. |
| Sensitive data exposed through chain, future logs, or storage | Privacy and commercial harm | Protocol data minimization; no patient data is required; token amount and addresses are visibly public. A future file store must define access, retention, redaction, and encryption. |
| Vulnerable dependency or CI secret leak | Compromise of build, keys, or users | SDK is exactly pinned and a lockfile is committed; CI includes a Rust dependency audit and secret scanning. No deployment keys belong in CI or source control. |
| Upgrade or storage-layout error | Frozen state or changed rules | No upgrade mechanism is provided in this stage; an upgrade/migration policy must be reviewed before deployment. |

## Implementation review

The following code and tests were reviewed for this foundation; this is not an independent audit or a proof of correctness.

| Area | Review performed | Status and limitation |
|---|---|---|
| Authorization | Checked initialization, admin/verifier methods, supplier and carrier milestones, lab actions, Attestor checkpoints, configured escrow calls, and arbiter resolution for `require_auth` plus role/party matching. Negative initializer tests and wrong-attestor/arbiter tests exist. | The visible unit suite does not exhaust every unauthorized caller for every public function. A full independent authorization matrix remains required. |
| Arithmetic and units | Reviewed checked quantity add/subtract, order ID increments, checkpoint counters, positive/max payment checks, split multiplication/division, and `lab_amount = amount - supplier_amount`. | Includes `u64::MAX`, insufficient stock, `i128::MAX`, zero amount, and split-over-10,000 tests. Soroban host arithmetic and token decimals still require deployment-level review. |
| Storage growth and loops | Reviewed fixed-size role/licence/order/batch/proof/checkpoint records, 64-byte batch IDs, 64-checkpoint cap, fixed lifetime caps, and absence of list scans. | These finite caps are protocol-wide lifetime limits, not adjustable configuration. Ledger event history and unrelated network state are not capped by the contract. |
| TTL and archival | Checked extension calls on contract instances and persistent entries, with threshold 100,000 and target 500,000 ledgers. | There is no archival restore/recovery entrypoint. An order or batch left idle past its TTL may need a separately designed recovery process; a long-idle dispute can strand funds operationally. |
| Double payout / inventory release | Reviewed terminal-state writes before token/batch calls, error propagation for subcalls, order/proof immutability, and repeated cancellation/confirmation/dispute calls. Integration tests cover happy settlement, split settlement, cancel/release, timeouts, and replays. | Atomic rollback and cross-contract behavior were tested in the local Soroban test environment, not on testnet. This requires independent audit. |
| Stuck funds | Checked 48-hour unaccepted refund, 7-day accepted/in-flight refund, 72-hour delivered refund, unresolved-excursion gate, and arbiter split. | A disputed order has no timeout. A delivered order with unresolved excursion cannot use its delivery timeout until the arbiter resolves the excursion. Arbiter loss/unavailability can leave funds locked. |
| Deployment and network behavior | Built local Wasm artifacts with the pinned CLI; inspected deployment script inputs and output handling. | No testnet RPC/deployment transaction was run. Network-facing CLI flags, fees, identities, test asset issuance, and contract-call authorization remain unverified in a live network. |

## Key management

Admin, verifier, supplier, carrier, attestor, arbiter, and user signing keys are distinct trust domains. Do not store seeds in source control, `.env` examples, CI logs, or an application database. The deployment script consumes existing CLI identities or public addresses supplied through environment variables and writes only public IDs. Prefer hardware-backed or multisignature controls for privileged roles where supported. Document key creation, rotation, revocation, recovery, and incident response before production deployment. Compromise of an authorised verifier, arbiter, or attestor remains a material risk.

## Residual risks

A public blockchain cannot keep transaction counterparties or amounts confidential. Contract correctness cannot validate real-world licensure, product authenticity, sensor calibration, chain of custody, or legal compliance. A compromised authorised key can make valid-looking attestations. The off-chain evidence store does not yet exist; later systems may lose or expose files even when a hash remains on-chain. Records can be archived after long inactivity, and disputes have no automatic settlement deadline. These risks require operational controls and are not eliminated by smart contracts.
