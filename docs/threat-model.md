# Threat Model

## Audit status

**The contracts are unaudited and testnet-only.** No production deployment is supported. Do not use real funds, real regulated shipments, or this software as the sole evidence for a medical-supply decision. An independent security audit and operational/legal review are required before any mainnet or production use.

## Scope and assets

The intended protocol manages participant roles, supplier verification attestations, batch availability, escrowed test-asset funds, order transitions, authorised temperature checkpoints, excursion flags, dispute resolution, and proof hashes. Off-chain components manage authentication sessions, indexed events, log files, and application access.

Primary assets are escrow funds, correct fulfilment and payout state, integrity of verification and proof records, privileged keys, and the confidentiality of off-chain personal, patient, and commercial information.

## Trust boundaries and assumptions

- Soroban contracts enforce the protocol's final authorization and state invariants. The API, indexer, and UI are not trusted to enforce them.
- The chain trusts **authorised attestors, not raw sensors**. A signed checkpoint only proves that an authorised key attested to data; it does not prove that a sensor was accurate, calibrated, uncompromised, or correctly associated with a shipment.
- Verifier decisions and licence evidence are external inputs. A document hash does not prove the underlying licence is valid; expiry and revocation processes must be operated.
- Stellar transaction and address data are public. Hashes can leak information if their source material is guessable or too small; use salted/structured commitments where appropriate and do not anchor sensitive plaintext.
- The database is an untrusted, rebuildable projection. Event consumers must verify source, ordering/finality assumptions, and idempotency.
- Testnet assets and deployments do not establish production readiness.

## Threats and planned mitigations

| Threat | Impact | Planned controls / review focus |
|---|---|---|
| Unauthorized role, batch, attestation, or order action | Fraud, invalid stock state, or loss of funds | Contract `require_auth`; explicit role checks; verifier/admin separation; test negative authorization paths |
| Admin or verifier key compromise | False licence attestations, abusive configuration, or blocked participants | Multisig-capable administration; separation of duties; expiry/revocation; key custody and recovery runbooks; narrow admin powers |
| Escrow release with an unresolved excursion or dispute | Funds released despite compromised condition | Contract invariant blocks ordinary release; excursion and dispute state are contract-owned; repeated-call and transition-order tests |
| Reentrancy-style or cross-contract callback hazards | Duplicate or inconsistent payout/state updates | Soroban execution model differs from EVM; nevertheless use checks-effects-interactions reasoning, avoid unsafe callback assumptions, make transitions idempotent, and test cross-contract call ordering |
| Auth bypass via relayer, API, or mismatched signer | Impersonation or unauthorized transaction | Bind every privileged action to the intended signer/address; require contract authorization; never treat API authentication as on-chain authorization |
| Griefing through reservations, spam, stuck orders, or storage growth | Inventory denial, operational blockage, or elevated resource cost | Reservation expiry/release rules; deterministic timeouts; bounded per-call work and storage; access-controlled writes; TTL extension; avoid unbounded scans |
| Temperature oracle or data-integrity failure | Incorrect excursion determination or false condition proof | Restrict attestors; bind readings to shipment/batch and time; hash source logs; document calibration/custody assumptions; use independent checks in operations |
| Replay, stale, duplicated, or out-of-order checkpoint | Wrong shipment state or duplicate effects | Include shipment/order identity, timestamp/sequence context, and signer authorization; reject invalid transitions; ensure indexer idempotency |
| Hash substitution or misleading proof viewer | False evidence of delivery condition | Canonical serialization and domain separation; verify expected hash and on-chain record; show source, signer, and verification limitations |
| Integer, unit, time, or rounding errors | Incorrect quantities, limits, fees, or timeouts | Explicit fixed units (temperature in tenths °C; defined timestamp/quantity units); checked arithmetic; bounds; edge-case tests |
| Upgrade or storage-layout error | Frozen state or altered rules | Document upgrade authority and migration policy before deployment; test migrations; preserve access controls; no silent upgrade path |
| Sensitive data exposed through logs, chain, or database | Privacy and commercial harm | Data minimisation; never store patient data; avoid sensitive on-chain fields; redact application logs; restrict off-chain storage access and retention |
| Vulnerable dependency or CI secret leak | Compromise of build, keys, or users | Lockfiles and dependency audit; secret scanning; least-privilege CI; no keys or seed phrases in repository/workflows |

## Soroban-specific design review checklist

Before a contract is considered ready for deployment, verify that every state-changing path checks the correct authorization; every cross-contract call is bounded and its failure behavior is understood; TTL extension covers persistent records; storage reads/writes and collection sizes are bounded; fee and payout math is checked; emitted events correspond to committed state changes; and no timeout, dispute, or replay path permits double payment or permanently locks funds without recourse.

The contracts must define who may create and resolve disputes, what conditions allow cancellation/refund, what happens when a party never responds, and whether split payouts are deterministic. Those rules are not yet implemented or fully specified.

## Key management

Admin, verifier, supplier, carrier, and user signing keys are distinct trust domains. Do not store seeds in source control, `.env` examples, CI logs, or the application database. Prefer hardware-backed or multisignature controls for privileged roles where supported. Document key creation, rotation, revocation, recovery, and incident response before any production deployment. Compromise of an authorised attestor remains a material protocol risk.

## Residual risks

A public blockchain cannot keep transaction counterparties or amounts confidential. Contract correctness cannot validate real-world licensure, product authenticity, sensor calibration, chain-of-custody, or legal compliance. A compromised authorised key can make valid-looking attestations. The off-chain file store may be unavailable or lose content even when a hash remains on-chain. These risks require operational controls and are not eliminated by smart contracts.
