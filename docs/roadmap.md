# Roadmap

This roadmap is directional. Each stage depends on review and testing of the preceding stage; it is not a promise of delivery or a statement of production readiness.

## Build stages

1. **Scaffold and governance:** repository structure, security policy, core documentation, contribution guidance, and CI/security skeleton.
2. **Participant and batch registries:** Soroban roles, verifier attestations, batch metadata, quantity reservation/release, and focused unit tests.
3. **Order and condition core:** escrow, cold-chain attestations, proof of condition, authorization/state-transition tests, and happy-path, excursion, dispute, and timeout integration coverage.
4. **Protocol utilities:** capped fees, optional subscription authorization, reputation counters, and testnet deployment tooling.
5. **SDK and backend:** generated TypeScript bindings, event indexer, SEP-10 wallet authentication, PostgreSQL/Prisma, and authorised temperature-log ingestion.
6. **Frontend:** supplier directory, product/batch availability, order funding, shipment timeline, temperature chart, delivery confirmation, proof verification, and administration panel.
7. **Demo and contributor readiness:** seeded testnet data, end-to-end demo including an excursion, contributor issues, audit preparation, and final operational documentation.

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

## Readiness gates

Before handling real funds or regulated shipments: complete the contract and application implementation; test state invariants and failure paths; obtain independent security review/audit; establish key-management and incident-response procedures; validate off-chain data security and backup; and obtain qualified legal/regulatory advice for the operating model.
