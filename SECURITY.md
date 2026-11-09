# Security Policy

MediChain handles escrowed funds and proof-of-condition records for medical supply shipments, so we take security reports seriously.

## Reporting a vulnerability

**Please do not open a public issue, pull request, or discussion for security problems.**

Report privately using either channel:

1. **GitHub private vulnerability reporting (preferred):** go to the Security tab of this repository and choose "Report a vulnerability".
2. **Email:** mukielemukiala@gmail.com

Please include:
- A clear description of the issue and its impact
- The affected component (contract name, API route, frontend page) and version or commit hash
- Steps to reproduce, or a proof of concept (for contracts, a failing test or a testnet transaction hash is ideal)
- Any suggested fix

## What to expect

| Step | Target time |
|------|-------------|
| Acknowledgement of your report | within 72 hours |
| Initial assessment and severity rating | within 7 days |
| Status updates | at least every 14 days |
| Fix or mitigation for critical/high issues | as soon as practical, target 30 days |

We will credit reporters in the release notes and advisory unless you prefer to stay anonymous. We do not currently offer a paid bug bounty.

## Scope

In scope:
- Soroban contracts in /contracts (registry, batch registry, escrow, attestation, proof of condition, fees, reputation)
- The API in /apps/api and web app in /apps/web
- The SDK in /packages/sdk
- Authentication (SEP-10 flow) and role/authorisation logic

Examples of high-impact issues: loss, theft, or locking of escrowed funds; bypassing excursion checks or escrow release conditions; forging or altering attestations or proof records; authorisation bypass; privilege escalation between roles; admin or verifier key compromise paths.

Out of scope:
- Findings on third-party services or the Stellar network itself (report those to the relevant project)
- Social engineering, physical attacks, or volumetric denial-of-service
- Issues requiring a compromised user device or private key
- Reports from automated scanners without a demonstrated impact
- Testnet-only deployment quirks that have no mainnet impact

## Supported versions

This project is pre-release. Only the latest commit on main is supported. No audited mainnet deployment exists yet; do not use it with real funds until an independent audit has been published.

## Safe harbor

If you act in good faith, avoid privacy violations and service disruption, only test against your own accounts or testnet deployments, and give us reasonable time to fix the issue before public disclosure, we will not pursue legal action against you for your research. Please do not access, modify, or exfiltrate data that is not yours.

## Disclosure

We follow coordinated disclosure. We aim to publish a GitHub Security Advisory once a fix is available, and ask for up to 90 days before public disclosure, or less if the fix ships sooner.

## Known assumptions

Contracts trust authorised attestors, not raw sensors. See /docs/threat-model.md for the full threat model and trust assumptions.
