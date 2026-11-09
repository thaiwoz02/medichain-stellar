#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EXPECTED_CLI_VERSION="$(<"$ROOT_DIR/scripts/stellar-cli-version")"
ADDRESS_FILE="${MEDICHAIN_ADDRESS_FILE:-$ROOT_DIR/testnet-addresses.env}"
NETWORK="testnet"
ASSET_CODE="${MEDICHAIN_TEST_ASSET_CODE:-MDC}"
TOKEN_ISSUE_AMOUNT_STROOPS="${MEDICHAIN_TEST_TOKEN_AMOUNT_STROOPS:-1000000}"

fail() {
  printf 'error: %s\n' "$*" >&2
  exit 1
}

command -v stellar >/dev/null 2>&1 || fail "Install Stellar CLI $EXPECTED_CLI_VERSION and make 'stellar' available on PATH."
command -v cargo >/dev/null 2>&1 || fail "Install Rust and Cargo using the official Stellar setup guide."
actual_cli_version="$(stellar --version | awk 'NR == 1 { print $2 }')"
[[ "$actual_cli_version" == "$EXPECTED_CLI_VERSION" ]] || fail "Expected Stellar CLI $EXPECTED_CLI_VERSION; found $actual_cli_version."

: "${MEDICHAIN_ADMIN_ID:?Set an existing Stellar CLI identity alias for contract administration.}"
: "${MEDICHAIN_VERIFIER_ID:?Set an existing CLI identity alias for supplier licence attestations.}"
: "${MEDICHAIN_LAB_ID:?Set an existing CLI identity alias for the test-asset trustline.}"
: "${MEDICHAIN_SUPPLIER_ID:?Set the supplier address or CLI identity alias.}"
: "${MEDICHAIN_CARRIER_ID:?Set the logistics address or CLI identity alias.}"
: "${MEDICHAIN_ATTESTOR_ID:?Set the attestor address or CLI identity alias.}"
: "${MEDICHAIN_ARBITER_ID:?Set the arbiter address or CLI identity alias.}"
: "${MEDICHAIN_ASSET_ISSUER_ID:?Set an existing funded CLI identity alias for the test asset issuer.}"

public_key() {
  local value="$1"
  if [[ "$value" == G* ]]; then
    printf '%s' "$value"
  else
    stellar keys address "$value"
  fi
}

invoke() {
  local contract_id="$1"
  local source_identity="$2"
  shift 2
  stellar contract invoke \
    --contract-id "$contract_id" \
    --source-account "$source_identity" \
    --network "$NETWORK" \
    --send yes \
    -- "$@"
}

contract_id_for_alias() {
  local value
  value="$(stellar contract alias show "$1" 2>/dev/null | grep -Eo 'C[A-Z2-7]{55}' | head -n 1 || true)"
  [[ -n "$value" ]] || fail "Could not resolve contract alias '$1'."
  printf '%s' "$value"
}

[[ "$ASSET_CODE" =~ ^[A-Za-z0-9]{1,12}$ ]] || fail "MEDICHAIN_TEST_ASSET_CODE must be 1-12 alphanumeric characters."
[[ "$TOKEN_ISSUE_AMOUNT_STROOPS" =~ ^[1-9][0-9]*$ ]] || fail "MEDICHAIN_TEST_TOKEN_AMOUNT_STROOPS must be a positive integer number of asset stroops."

admin_address="$(public_key "$MEDICHAIN_ADMIN_ID")"
verifier_address="$(public_key "$MEDICHAIN_VERIFIER_ID")"
lab_address="$(public_key "$MEDICHAIN_LAB_ID")"
supplier_address="$(public_key "$MEDICHAIN_SUPPLIER_ID")"
carrier_address="$(public_key "$MEDICHAIN_CARRIER_ID")"
attestor_address="$(public_key "$MEDICHAIN_ATTESTOR_ID")"
arbiter_address="$(public_key "$MEDICHAIN_ARBITER_ID")"
issuer_address="$(public_key "$MEDICHAIN_ASSET_ISSUER_ID")"
asset="$ASSET_CODE:$issuer_address"

printf 'Building Soroban contracts for Stellar testnet...\n'
stellar contract build --manifest-path "$ROOT_DIR/contracts/Cargo.toml" --locked

printf 'Creating the test credit asset trustline and issuing test units...\n'
stellar tx new change-trust \
  --source-account "$MEDICHAIN_LAB_ID" \
  --line "$asset" \
  --network "$NETWORK"
stellar tx new payment \
  --source-account "$MEDICHAIN_ASSET_ISSUER_ID" \
  --destination "$lab_address" \\
  --asset "$asset" \\
  --amount "$TOKEN_ISSUE_AMOUNT_STROOPS" \\
  --network "$NETWORK"
printf 'Deploying the test asset Stellar Asset Contract...\n'
stellar contract asset deploy \
  --source "$MEDICHAIN_ASSET_ISSUER_ID" \
  --network "$NETWORK" \
  --asset "$asset"
token_address="$(stellar contract id asset --network "$NETWORK" --asset "$asset" | grep -Eo 'C[A-Z2-7]{55}' | head -n 1)"

printf 'Deploying the three MediChain contracts...\n'
for package in participant_registry product_batch_registry order_escrow; do
  stellar contract deploy \
    --wasm "$ROOT_DIR/contracts/target/wasm32v1-none/release/${package}.wasm" \
    --source-account "$MEDICHAIN_ADMIN_ID" \
    --network "$NETWORK" \
    --alias "medichain_${package}"
done
registry_id="$(contract_id_for_alias medichain_participant_registry)"
batches_id="$(contract_id_for_alias medichain_product_batch_registry)"
escrow_id="$(contract_id_for_alias medichain_order_escrow)"

printf 'Initializing contract configuration...\n'
invoke "$registry_id" "$MEDICHAIN_ADMIN_ID" initialize \
  --initial-admin "$admin_address" \
  --initial-verifier "$(public_key "$MEDICHAIN_VERIFIER_ID")"
invoke "$batches_id" "$MEDICHAIN_ADMIN_ID" initialize \
  --initial-admin "$admin_address" \
  --registry "$registry_id"
invoke "$escrow_id" "$MEDICHAIN_ADMIN_ID" initialize \
  --initial-admin "$admin_address" \
  --registry "$registry_id" \
  --batch-registry "$batches_id" \
  --initial-arbiter "$arbiter_address"
invoke "$batches_id" "$MEDICHAIN_ADMIN_ID" set-escrow --new-escrow "$escrow_id"

printf 'Registering test participant roles and supplier attestation...\n'
for role_pair in \
  "lab:$lab_address" \
  "supplier:$supplier_address" \
  "logistics:$carrier_address" \
  "attestor:$attestor_address"; do
  role="${role_pair%%:*}"
  participant="${role_pair#*:}"
  invoke "$registry_id" "$MEDICHAIN_ADMIN_ID" register-participant \
    --participant "$participant" \
    --role "$role"
done
expiry="$(($(date +%s) + 31_536_000))"
licence_hash="${MEDICHAIN_SUPPLIER_LICENCE_HASH:-$(printf '%064d' 0)}"
[[ "$licence_hash" =~ ^[A-Fa-f0-9]{64}$ ]] || fail "MEDICHAIN_SUPPLIER_LICENCE_HASH must be exactly 64 hexadecimal characters."
invoke "$registry_id" "$MEDICHAIN_VERIFIER_ID" attest-licence \
  --participant "$supplier_address" \
  --document-hash "$licence_hash" \
  --expires-at "$expiry"

umask 077
cat > "$ADDRESS_FILE.tmp" <<EOF
MEDICHAIN_NETWORK=$NETWORK
MEDICHAIN_PARTICIPANT_REGISTRY=$registry_id
MEDICHAIN_PRODUCT_BATCH_REGISTRY=$batches_id
MEDICHAIN_ORDER_ESCROW=$escrow_id
MEDICHAIN_TEST_ASSET=$token_address
MEDICHAIN_TEST_ASSET_CODE=$asset
EOF
mv "$ADDRESS_FILE.tmp" "$ADDRESS_FILE"
printf 'Deployment complete. Public contract and test-asset IDs saved to %s\n' "$ADDRESS_FILE"
printf 'This is a testnet deployment only. No contract audit or production readiness is implied.\n'
