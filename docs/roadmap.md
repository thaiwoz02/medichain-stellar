# Roadmap

This roadmap is directional. Each stage depends on review and testing of the preceding stage; it is not a promise of delivery or a statement of production readiness.

## Build stages

1. **Scaffold and governance — complete:** repository structure, security policy, core documentation, contribution guidance, and CI/security skeleton.
2. **Soroban foundation — implemented, unaudited:** participant registry, product/batch registry, order escrow, role-gated checkpoints, immutable proof, dispute settlement, timeout paths, tests, and a testnet deployment script. No testnet deployment was performed.
3. **Review and testnet readiness — future work:** independent contract review, archival recovery/key-management plan, network deployment validation, and an end-to-end testnet demonstration. This stage is not production readiness.
4. **SDK and backend — future contributor work:** typed client helpers, event indexer, wallet authentication, PostgreSQL projection, and authorised evidence metadata.
5. **Frontend — future contributor work:** supplier/batch availability, order funding, shipment timeline, temperature evidence, delivery proof, and administration interface.
6. **Demo and contributor readiness — future work:** seeded testnet data, end-to-end excursion demonstration, contributor issues, audit preparation, and operational documentation.

## Pilot and expansion direction

- **Lagos pilot:** limited set of diagnostic labs, licensed suppliers, logistics partners, selected diagnostic reagents and test kits, and testnet-first workflows. Any real-world pilot requires legal/regulatory review, operational controls, and appropriate independent security review.
- **Regional expansion:** evaluate additional markets only after validating local licensing, payment, privacy, and logistics requirements.
- **Rural access:** explore whether coordination and settlement tooling can improve availability and visibility for underserved areas without weakening quality or compliance controls.

## Postponed / out of scope

- MediChain-owned warehouses or vehicles
- Proprietary sensor manufacturing or deployment
- AI forecasting
- Vaccine procurement
- General medicines

These items are intentionally outside the initial scope and should not be introduced without a separate product, regulatory, operational, and security review.

Fees, subscriptions, and reputation are also deliberately deferred; they are not implemented by the current contracts.

## Readiness gates

Before handling real funds or regulated shipments: complete the contract and application implementation; test state invariants and failure paths; obtain independent security review/audit; establish key-management and incident-response procedures; validate off-chain data security and backup; and obtain qualified legal/regulatory advice for the operating model.
