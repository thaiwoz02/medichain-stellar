# Contributing to MediChain

Thank you for considering a contribution. MediChain coordinates B2B supply workflows; it does not distribute regulated products. Contributions must preserve the separation between on-chain trust/state and sensitive off-chain data.

## Before starting

- Read the [README](README.md), [architecture](docs/architecture.md), [threat model](docs/threat-model.md), and [regulatory notes](docs/regulatory-notes.md).
- For planned work, choose a scoped item under `docs/issues/` when available. For larger changes, discuss the design before implementation.
- Keep pull requests focused and identify any effect on contract authorization, escrow safety, data privacy, upgradeability, or regulated workflows.

## Development expectations

- Follow the existing language and formatting conventions. Pin dependency versions and update lockfiles when dependencies change.
- Validate untrusted inputs at boundaries and make authorization explicit. Do not rely on the UI or API for contract-level security.
- Do not put patient data, personal data, detailed temperature logs, private keys, seeds, or commercially sensitive pricing on-chain.
- Never commit credentials, `.env` files, private keys, or testnet secrets. `.env.example` is the only environment file intended for version control.
- Contract code and tests must clearly distinguish testnet/demo behavior from production readiness.

## Checks

The project is being built in stages, so some commands are not available until their component is implemented. Once the relevant manifests and lockfiles exist, run the applicable CI checks locally:

```sh
cargo fmt --manifest-path contracts/Cargo.toml --all -- --check
cargo clippy --manifest-path contracts/Cargo.toml --workspace --all-targets -- -D warnings
cargo test --manifest-path contracts/Cargo.toml --workspace
cargo audit --file contracts/Cargo.lock
npm audit --audit-level=high
```

Install and run the configured secret check before committing:

```sh
pipx install pre-commit
pre-commit install
pre-commit run --all-files
```

The CI workflows are authoritative for the checks currently applicable to the repository.

## Pull requests

- Explain the problem, the approach, and how it was tested.
- Link related issues and call out any migration, deployment, or operational impact.
- Include tests for behavior changes, especially access control, state transitions, timeouts, excursions, disputes, and settlement.
- Do not claim regulatory compliance, audit status, or production safety without evidence.
- Keep changes reviewable; separate unrelated refactors.

By participating, you agree to follow the [Code of Conduct](CODE_OF_CONDUCT.md).
