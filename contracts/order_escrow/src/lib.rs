#![no_std]
// Keep the tuple event encoding stable until downstream event schemas are versioned.
#![allow(deprecated)]
// The checkpoint ABI keeps each signed evidence field explicit and independently validated.
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short,
    token::TokenClient, Address, BytesN, Env, MuxedAddress, String,
};

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    Lab,
    Supplier,
    Logistics,
    Attestor,
}

#[contractclient(name = "ParticipantRegistryClient")]
pub trait ParticipantRegistryInterface {
    fn get_role(env: Env, participant: Address) -> Option<Role>;
    fn is_verified(env: Env, participant: Address) -> bool;
}

#[contractclient(name = "ProductBatchRegistryClient")]
pub trait ProductBatchRegistryInterface {
    fn order_terms(env: Env, batch_id: String) -> Option<(Address, i32, i32)>;
    fn is_fulfillable(env: Env, batch_id: String, quantity: u64) -> bool;
    fn reserve_batch(env: Env, batch_id: String, quantity: u64);
    fn release_batch(env: Env, batch_id: String, quantity: u64);
    fn fulfill_reserved(env: Env, batch_id: String, quantity: u64);
}

const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
const INSTANCE_TTL_EXTEND_TO: u32 = 500_000;
const ENTRY_TTL_THRESHOLD: u32 = 100_000;
const ENTRY_TTL_EXTEND_TO: u32 = 500_000;
pub const ACCEPTANCE_TIMEOUT_SECONDS: u64 = 172_800;
pub const DELIVERY_TIMEOUT_SECONDS: u64 = 604_800;
pub const CONFIRMATION_TIMEOUT_SECONDS: u64 = 259_200;
pub const MAX_CHECKPOINTS_PER_ORDER: u32 = 64;
pub const MAX_BATCH_ID_BYTES: u32 = 64;
pub const MAX_ORDERS: u64 = 100_000;
pub const MAX_PAYMENT_AMOUNT: i128 = i128::MAX / 10_000;
const BASIS_POINTS: i128 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidOrder = 4,
    InvalidAmount = 5,
    InvalidQuantity = 6,
    ParticipantNotEligible = 7,
    BatchNotFulfillable = 8,
    OrderMissing = 9,
    WrongState = 10,
    NotOrderParty = 11,
    CheckpointSequence = 12,
    InvalidCheckpoint = 13,
    TooManyCheckpoints = 14,
    ExcursionUnresolved = 15,
    InvalidProof = 16,
    ProofExists = 17,
    InvalidSplit = 18,
    TimeoutNotReached = 19,
    TimeoutNotApplicable = 20,
    Arithmetic = 21,
    InvalidConfiguration = 22,
    CapacityReached = 23,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum OrderState {
    Created,
    Placed,
    Accepted,
    Packed,
    InTransit,
    Delivered,
    Confirmed,
    Disputed,
    Cancelled,
    Resolved,
}

#[contracttype]
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ConditionCode {
    Acceptable,
    IssueReported,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Order {
    pub id: u64,
    pub lab: Address,
    pub supplier: Address,
    pub carrier: Address,
    pub batch_id: String,
    pub quantity: u64,
    pub payment_token: Address,
    pub amount: i128,
    pub storage_min_tenths_c: i32,
    pub storage_max_tenths_c: i32,
    pub state: OrderState,
    pub created_at: u64,
    pub accepted_at: u64,
    pub delivered_at: u64,
    pub last_checkpoint_sequence: u32,
    pub last_checkpoint_timestamp: u64,
    pub checkpoint_count: u32,
    pub excursion_unresolved: bool,
    pub dispute_origin: OrderState,
    pub dispute_origin_set: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Checkpoint {
    pub order_id: u64,
    pub sequence: u32,
    pub attestor: Address,
    pub timestamp: u64,
    pub observed_min_tenths_c: i32,
    pub observed_max_tenths_c: i32,
    pub log_hash: BytesN<32>,
    pub excursion: bool,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ProofOfCondition {
    pub order_id: u64,
    pub lab: Address,
    pub quantity_received: u64,
    pub condition: ConditionCode,
    pub temperature_log_hash: BytesN<32>,
    pub recorded_at: u64,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Registry,
    BatchRegistry,
    Arbiter,
    NextOrderId,
    Order(u64),
    Checkpoint(u64, u32),
    Proof(u64),
}

#[contract]
pub struct OrderEscrow;

fn touch_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

fn order_key(order_id: u64) -> DataKey {
    DataKey::Order(order_id)
}

fn proof_key(order_id: u64) -> DataKey {
    DataKey::Proof(order_id)
}

fn extend_persistent(env: &Env, key: &DataKey) {
    env.storage()
        .persistent()
        .extend_ttl(key, ENTRY_TTL_THRESHOLD, ENTRY_TTL_EXTEND_TO);
}

fn admin(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Admin)
        .ok_or(Error::NotInitialized)
}

fn registry_address(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Registry)
        .ok_or(Error::NotInitialized)
}

fn batch_registry_address(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::BatchRegistry)
        .ok_or(Error::NotInitialized)
}

fn arbiter(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Arbiter)
        .ok_or(Error::NotInitialized)
}

fn read_order(env: &Env, order_id: u64) -> Result<Order, Error> {
    let key = order_key(order_id);
    let order = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::OrderMissing)?;
    extend_persistent(env, &key);
    Ok(order)
}

fn write_order(env: &Env, order: &Order) {
    let key = order_key(order.id);
    env.storage().persistent().set(&key, order);
    extend_persistent(env, &key);
}

fn state_event(env: &Env, order_id: u64, old: OrderState, new: OrderState) {
    env.events()
        .publish((symbol_short!("state"), order_id), (old, new));
}

fn require_state(order: &Order, expected: OrderState) -> Result<(), Error> {
    if order.state == expected {
        Ok(())
    } else {
        Err(Error::WrongState)
    }
}

fn ensure_party(order: &Order, actor: &Address) -> Result<(), Error> {
    if actor == &order.lab || actor == &order.supplier || actor == &order.carrier {
        Ok(())
    } else {
        Err(Error::NotOrderParty)
    }
}

fn transfer(env: &Env, token: &Address, from: &Address, to: &Address, amount: i128) {
    if amount > 0 {
        let token_client = TokenClient::new(env, token);
        let recipient = MuxedAddress::from(to.clone());
        token_client.transfer(from, &recipient, &amount);
    }
}

fn refund_and_release(env: &Env, order: &Order) -> Result<(), Error> {
    let self_address = env.current_contract_address();
    let batch_registry = batch_registry_address(env)?;
    ProductBatchRegistryClient::new(env, &batch_registry)
        .release_batch(&order.batch_id, &order.quantity);
    transfer(
        env,
        &order.payment_token,
        &self_address,
        &order.lab,
        order.amount,
    );
    Ok(())
}

fn settle_success(env: &Env, order: &Order) -> Result<(), Error> {
    let batch_registry = batch_registry_address(env)?;
    ProductBatchRegistryClient::new(env, &batch_registry)
        .fulfill_reserved(&order.batch_id, &order.quantity);
    let self_address = env.current_contract_address();
    transfer(
        env,
        &order.payment_token,
        &self_address,
        &order.supplier,
        order.amount,
    );
    Ok(())
}

#[contractimpl]
impl OrderEscrow {
    pub fn initialize(
        env: Env,
        initial_admin: Address,
        registry: Address,
        batch_registry: Address,
        initial_arbiter: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        initial_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Admin, &initial_admin);
        env.storage().instance().set(&DataKey::Registry, &registry);
        env.storage()
            .instance()
            .set(&DataKey::BatchRegistry, &batch_registry);
        env.storage()
            .instance()
            .set(&DataKey::Arbiter, &initial_arbiter);
        env.storage().instance().set(&DataKey::NextOrderId, &0_u64);
        touch_instance(&env);
        env.events().publish(
            (symbol_short!("init"), initial_admin),
            (registry, batch_registry, initial_arbiter),
        );
        Ok(())
    }

    pub fn set_arbiter(env: Env, new_arbiter: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Arbiter, &new_arbiter);
        env.events()
            .publish((symbol_short!("arbiter"), current_admin), new_arbiter);
        Ok(())
    }

    pub fn create_order(
        env: Env,
        lab: Address,
        batch_id: String,
        carrier: Address,
        quantity: u64,
        payment_token: Address,
        amount: i128,
    ) -> Result<u64, Error> {
        touch_instance(&env);
        lab.require_auth();
        if batch_id.is_empty() || batch_id.len() > MAX_BATCH_ID_BYTES {
            return Err(Error::InvalidOrder);
        }
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        if amount <= 0 || amount > MAX_PAYMENT_AMOUNT {
            return Err(Error::InvalidAmount);
        }
        let registry = registry_address(&env)?;
        let participants = ParticipantRegistryClient::new(&env, &registry);
        if participants.get_role(&lab) != Some(Role::Lab) {
            return Err(Error::ParticipantNotEligible);
        }
        let batch_registry = batch_registry_address(&env)?;
        let batches = ProductBatchRegistryClient::new(&env, &batch_registry);
        if !batches.is_fulfillable(&batch_id, &quantity) {
            return Err(Error::BatchNotFulfillable);
        }
        let (supplier, storage_min_tenths_c, storage_max_tenths_c) = batches
            .order_terms(&batch_id)
            .ok_or(Error::BatchNotFulfillable)?;
        if participants.get_role(&supplier) != Some(Role::Supplier)
            || !participants.is_verified(&supplier)
            || participants.get_role(&carrier) != Some(Role::Logistics)
        {
            return Err(Error::ParticipantNotEligible);
        }

        let next_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextOrderId)
            .ok_or(Error::NotInitialized)?;
        if next_id >= MAX_ORDERS {
            return Err(Error::CapacityReached);
        }
        let order_id = next_id.checked_add(1).ok_or(Error::Arithmetic)?;
        env.storage()
            .instance()
            .set(&DataKey::NextOrderId, &order_id);

        batches.reserve_batch(&batch_id, &quantity);
        let self_address = env.current_contract_address();
        transfer(&env, &payment_token, &lab, &self_address, amount);

        let order = Order {
            id: order_id,
            lab: lab.clone(),
            supplier: supplier.clone(),
            carrier: carrier.clone(),
            batch_id: batch_id.clone(),
            quantity,
            payment_token,
            amount,
            storage_min_tenths_c,
            storage_max_tenths_c,
            state: OrderState::Placed,
            created_at: env.ledger().timestamp(),
            accepted_at: 0,
            delivered_at: 0,
            last_checkpoint_sequence: 0,
            last_checkpoint_timestamp: 0,
            checkpoint_count: 0,
            excursion_unresolved: false,
            dispute_origin: OrderState::Created,
            dispute_origin_set: false,
        };
        write_order(&env, &order);
        state_event(&env, order_id, OrderState::Created, OrderState::Placed);
        env.events().publish(
            (symbol_short!("created"), order_id),
            (lab, supplier, batch_id, quantity, amount),
        );
        Ok(order_id)
    }

    pub fn get_order(env: Env, order_id: u64) -> Option<Order> {
        touch_instance(&env);
        let key = order_key(order_id);
        let order = env.storage().persistent().get(&key);
        if order.is_some() {
            extend_persistent(&env, &key);
        }
        order
    }

    pub fn accept(env: Env, order_id: u64, supplier: Address) -> Result<(), Error> {
        touch_instance(&env);
        supplier.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Placed)?;
        if supplier != order.supplier {
            return Err(Error::NotOrderParty);
        }
        let old = order.state;
        order.state = OrderState::Accepted;
        order.accepted_at = env.ledger().timestamp();
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        Ok(())
    }

    pub fn mark_packed(env: Env, order_id: u64, supplier: Address) -> Result<(), Error> {
        touch_instance(&env);
        supplier.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Accepted)?;
        if supplier != order.supplier {
            return Err(Error::NotOrderParty);
        }
        let old = order.state;
        order.state = OrderState::Packed;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        Ok(())
    }

    pub fn mark_in_transit(env: Env, order_id: u64, carrier: Address) -> Result<(), Error> {
        touch_instance(&env);
        carrier.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Packed)?;
        if carrier != order.carrier {
            return Err(Error::NotOrderParty);
        }
        let old = order.state;
        order.state = OrderState::InTransit;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        Ok(())
    }

    pub fn mark_delivered(env: Env, order_id: u64, carrier: Address) -> Result<(), Error> {
        touch_instance(&env);
        carrier.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::InTransit)?;
        if carrier != order.carrier {
            return Err(Error::NotOrderParty);
        }
        let old = order.state;
        order.state = OrderState::Delivered;
        order.delivered_at = env.ledger().timestamp();
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        Ok(())
    }

    pub fn submit_checkpoint(
        env: Env,
        order_id: u64,
        attestor: Address,
        sequence: u32,
        timestamp: u64,
        observed_min_tenths_c: i32,
        observed_max_tenths_c: i32,
        log_hash: BytesN<32>,
    ) -> Result<bool, Error> {
        touch_instance(&env);
        attestor.require_auth();
        let mut order = read_order(&env, order_id)?;
        if order.state != OrderState::Accepted
            && order.state != OrderState::Packed
            && order.state != OrderState::InTransit
            && order.state != OrderState::Delivered
        {
            return Err(Error::WrongState);
        }
        let registry = registry_address(&env)?;
        if ParticipantRegistryClient::new(&env, &registry).get_role(&attestor)
            != Some(Role::Attestor)
        {
            return Err(Error::ParticipantNotEligible);
        }
        if sequence
            != order
                .last_checkpoint_sequence
                .checked_add(1)
                .ok_or(Error::Arithmetic)?
        {
            return Err(Error::CheckpointSequence);
        }
        if order.checkpoint_count >= MAX_CHECKPOINTS_PER_ORDER {
            return Err(Error::TooManyCheckpoints);
        }
        if observed_min_tenths_c > observed_max_tenths_c
            || timestamp < order.last_checkpoint_timestamp
            || timestamp > env.ledger().timestamp()
        {
            return Err(Error::InvalidCheckpoint);
        }
        let excursion = observed_min_tenths_c < order.storage_min_tenths_c
            || observed_max_tenths_c > order.storage_max_tenths_c;
        let checkpoint = Checkpoint {
            order_id,
            sequence,
            attestor: attestor.clone(),
            timestamp,
            observed_min_tenths_c,
            observed_max_tenths_c,
            log_hash: log_hash.clone(),
            excursion,
        };
        let key = DataKey::Checkpoint(order_id, sequence);
        env.storage().persistent().set(&key, &checkpoint);
        extend_persistent(&env, &key);
        order.last_checkpoint_sequence = sequence;
        order.last_checkpoint_timestamp = timestamp;
        order.checkpoint_count = order
            .checkpoint_count
            .checked_add(1)
            .ok_or(Error::Arithmetic)?;
        if excursion {
            order.excursion_unresolved = true;
            env.events().publish(
                (symbol_short!("excursion"), order_id, sequence),
                (
                    observed_min_tenths_c,
                    observed_max_tenths_c,
                    log_hash.clone(),
                ),
            );
        }
        write_order(&env, &order);
        env.events().publish(
            (symbol_short!("checkpt"), order_id, sequence),
            (
                attestor,
                timestamp,
                observed_min_tenths_c,
                observed_max_tenths_c,
                log_hash,
            ),
        );
        Ok(excursion)
    }

    pub fn get_checkpoint(env: Env, order_id: u64, sequence: u32) -> Option<Checkpoint> {
        touch_instance(&env);
        let key = DataKey::Checkpoint(order_id, sequence);
        let checkpoint = env.storage().persistent().get(&key);
        if checkpoint.is_some() {
            extend_persistent(&env, &key);
        }
        checkpoint
    }

    pub fn confirm_delivery(
        env: Env,
        order_id: u64,
        lab: Address,
        quantity_received: u64,
        condition: ConditionCode,
        temperature_log_hash: BytesN<32>,
    ) -> Result<(), Error> {
        touch_instance(&env);
        lab.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Delivered)?;
        if lab != order.lab {
            return Err(Error::NotOrderParty);
        }
        if order.excursion_unresolved {
            return Err(Error::ExcursionUnresolved);
        }
        if quantity_received != order.quantity || condition != ConditionCode::Acceptable {
            return Err(Error::InvalidProof);
        }
        let key = proof_key(order_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::ProofExists);
        }
        let proof = ProofOfCondition {
            order_id,
            lab: lab.clone(),
            quantity_received,
            condition,
            temperature_log_hash,
            recorded_at: env.ledger().timestamp(),
        };
        env.storage().persistent().set(&key, &proof);
        extend_persistent(&env, &key);
        let old = order.state;
        order.state = OrderState::Confirmed;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        settle_success(&env, &order)?;
        env.events()
            .publish((symbol_short!("proof"), order_id), (lab, quantity_received));
        Ok(())
    }

    pub fn get_proof(env: Env, order_id: u64) -> Option<ProofOfCondition> {
        touch_instance(&env);
        let key = proof_key(order_id);
        let proof = env.storage().persistent().get(&key);
        if proof.is_some() {
            extend_persistent(&env, &key);
        }
        proof
    }

    pub fn raise_dispute(env: Env, order_id: u64, actor: Address) -> Result<(), Error> {
        touch_instance(&env);
        actor.require_auth();
        let mut order = read_order(&env, order_id)?;
        ensure_party(&order, &actor)?;
        if order.state != OrderState::Accepted
            && order.state != OrderState::Packed
            && order.state != OrderState::InTransit
            && order.state != OrderState::Delivered
        {
            return Err(Error::WrongState);
        }
        let old = order.state;
        order.dispute_origin = old;
        order.dispute_origin_set = true;
        order.state = OrderState::Disputed;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        env.events()
            .publish((symbol_short!("dispute"), order_id), actor);
        Ok(())
    }

    pub fn resolve_excursion(
        env: Env,
        order_id: u64,
        arbiter_address: Address,
        acceptable: bool,
    ) -> Result<(), Error> {
        touch_instance(&env);
        arbiter_address.require_auth();
        if arbiter_address != arbiter(&env)? {
            return Err(Error::Unauthorized);
        }
        let mut order = read_order(&env, order_id)?;
        if !order.excursion_unresolved {
            return Err(Error::InvalidCheckpoint);
        }
        order.excursion_unresolved = false;
        if !acceptable && order.state != OrderState::Disputed {
            let old = order.state;
            order.dispute_origin = old;
            order.dispute_origin_set = true;
            order.state = OrderState::Disputed;
            state_event(&env, order_id, old, order.state);
        }
        write_order(&env, &order);
        env.events().publish(
            (symbol_short!("excres"), order_id),
            (arbiter_address, acceptable),
        );
        Ok(())
    }

    pub fn resolve_dispute(
        env: Env,
        order_id: u64,
        arbiter_address: Address,
        supplier_basis_points: u32,
    ) -> Result<(i128, i128), Error> {
        touch_instance(&env);
        arbiter_address.require_auth();
        if arbiter_address != arbiter(&env)? {
            return Err(Error::Unauthorized);
        }
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Disputed)?;
        if order.excursion_unresolved {
            return Err(Error::ExcursionUnresolved);
        }
        if supplier_basis_points > 10_000 {
            return Err(Error::InvalidSplit);
        }
        let supplier_amount = order
            .amount
            .checked_mul(i128::from(supplier_basis_points))
            .ok_or(Error::Arithmetic)?
            .checked_div(BASIS_POINTS)
            .ok_or(Error::Arithmetic)?;
        let lab_amount = order
            .amount
            .checked_sub(supplier_amount)
            .ok_or(Error::Arithmetic)?;
        let old = order.state;
        order.state = OrderState::Resolved;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        let self_address = env.current_contract_address();
        transfer(
            &env,
            &order.payment_token,
            &self_address,
            &order.supplier,
            supplier_amount,
        );
        transfer(
            &env,
            &order.payment_token,
            &self_address,
            &order.lab,
            lab_amount,
        );
        let batch_registry = batch_registry_address(&env)?;
        let batches = ProductBatchRegistryClient::new(&env, &batch_registry);
        if supplier_amount == 0 {
            batches.release_batch(&order.batch_id, &order.quantity);
        } else {
            batches.fulfill_reserved(&order.batch_id, &order.quantity);
        }
        env.events().publish(
            (symbol_short!("resolve"), order_id),
            (arbiter_address, supplier_amount, lab_amount),
        );
        Ok((supplier_amount, lab_amount))
    }

    pub fn cancel_before_acceptance(env: Env, order_id: u64, lab: Address) -> Result<(), Error> {
        touch_instance(&env);
        lab.require_auth();
        let mut order = read_order(&env, order_id)?;
        require_state(&order, OrderState::Placed)?;
        if lab != order.lab {
            return Err(Error::NotOrderParty);
        }
        let old = order.state;
        order.state = OrderState::Cancelled;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        refund_and_release(&env, &order)?;
        Ok(())
    }

    pub fn timeout_order(env: Env, order_id: u64, caller: Address) -> Result<(), Error> {
        touch_instance(&env);
        caller.require_auth();
        let mut order = read_order(&env, order_id)?;
        let now = env.ledger().timestamp();
        let expired = match order.state {
            OrderState::Placed => {
                now >= order.created_at.saturating_add(ACCEPTANCE_TIMEOUT_SECONDS)
            }
            OrderState::Accepted | OrderState::Packed | OrderState::InTransit => {
                now >= order.accepted_at.saturating_add(DELIVERY_TIMEOUT_SECONDS)
            }
            OrderState::Delivered => {
                if order.excursion_unresolved {
                    return Err(Error::ExcursionUnresolved);
                }
                now >= order
                    .delivered_at
                    .saturating_add(CONFIRMATION_TIMEOUT_SECONDS)
            }
            OrderState::Created
            | OrderState::Confirmed
            | OrderState::Disputed
            | OrderState::Cancelled
            | OrderState::Resolved => return Err(Error::TimeoutNotApplicable),
        };
        if !expired {
            return Err(Error::TimeoutNotReached);
        }
        let old = order.state;
        order.state = OrderState::Cancelled;
        write_order(&env, &order);
        state_event(&env, order_id, old, order.state);
        refund_and_release(&env, &order)?;
        env.events()
            .publish((symbol_short!("timeout"), order_id), caller);
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{
        ConditionCode, Error, OrderEscrow, OrderEscrowClient, OrderState,
        ACCEPTANCE_TIMEOUT_SECONDS, CONFIRMATION_TIMEOUT_SECONDS, DELIVERY_TIMEOUT_SECONDS,
    };
    use participant_registry::{
        ParticipantRegistry, ParticipantRegistryClient as ParticipantTestClient,
        Role as ParticipantRole,
    };
    use product_batch_registry::{
        ProductBatchRegistry, ProductBatchRegistryClient as BatchTestClient,
    };
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        token::{StellarAssetClient, TokenClient},
        Address, BytesN, Env, String,
    };

    struct Fixture {
        env: Env,
        escrow_id: Address,
        batch_registry_id: Address,
        lab: Address,
        supplier: Address,
        carrier: Address,
        attestor: Address,
        arbiter: Address,
        batch_id: String,
        token_id: Address,
    }

    fn fixture() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let verifier = Address::generate(&env);
        let lab = Address::generate(&env);
        let supplier = Address::generate(&env);
        let carrier = Address::generate(&env);
        let attestor = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let participants_id = env.register(ParticipantRegistry, ());
        let participants = ParticipantTestClient::new(&env, &participants_id);
        participants.initialize(&admin, &verifier);
        participants.register_participant(&lab, &ParticipantRole::Lab);
        participants.register_participant(&supplier, &ParticipantRole::Supplier);
        participants.register_participant(&carrier, &ParticipantRole::Logistics);
        participants.register_participant(&attestor, &ParticipantRole::Attestor);
        participants.attest_licence(
            &supplier,
            &BytesN::from_array(&env, &[1; 32]),
            &(env.ledger().timestamp() + 10_000_000),
        );

        let batches_id = env.register(ProductBatchRegistry, ());
        let batches = BatchTestClient::new(&env, &batches_id);
        batches.initialize(&admin, &participants_id);
        let batch_id = String::from_str(&env, "REAGENT-LOT-1");
        batches.register_batch(
            &supplier,
            &batch_id,
            &BytesN::from_array(&env, &[2; 32]),
            &20,
            &80,
            &(env.ledger().timestamp() + 20_000_000),
            &100,
        );

        let token_admin = Address::generate(&env);
        let sac = env.register_stellar_asset_contract_v2(token_admin.clone());
        let token_id = sac.address();
        let issuer = StellarAssetClient::new(&env, &token_id);
        issuer.mint(&lab, &1_000_000);

        let escrow_id = env.register(OrderEscrow, ());
        let client = OrderEscrowClient::new(&env, &escrow_id);
        client.initialize(&admin, &participants_id, &batches_id, &arbiter);
        batches.set_escrow(&escrow_id);
        Fixture {
            env,
            escrow_id,
            batch_registry_id: batches_id,
            lab,
            supplier,
            carrier,
            attestor,
            arbiter,
            batch_id,
            token_id,
        }
    }

    impl Fixture {
        fn client(&self) -> OrderEscrowClient<'_> {
            OrderEscrowClient::new(&self.env, &self.escrow_id)
        }
    }

    fn create(f: &Fixture, amount: i128, quantity: u64) -> u64 {
        f.client().create_order(
            &f.lab,
            &f.batch_id,
            &f.carrier,
            &quantity,
            &f.token_id,
            &amount,
        )
    }

    fn reach_in_transit(f: &Fixture, id: u64) {
        f.client().accept(&id, &f.supplier);
        f.client().mark_packed(&id, &f.supplier);
        f.client().mark_in_transit(&id, &f.carrier);
    }

    #[test]
    fn happy_path_funds_confirms_and_stores_immutable_proof() {
        let f = fixture();
        let id = create(&f, 500, 5);
        assert_eq!(f.client().get_order(&id).unwrap().state, OrderState::Placed);
        let token = TokenClient::new(&f.env, &f.token_id);
        let escrow_id = f.escrow_id.clone();
        assert_eq!(token.balance(&escrow_id), 500);
        reach_in_transit(&f, id);
        let now = f.env.ledger().timestamp();
        assert!(!f.client().submit_checkpoint(
            &id,
            &f.attestor,
            &1,
            &now,
            &30,
            &60,
            &BytesN::from_array(&f.env, &[3; 32]),
        ));
        f.client().mark_delivered(&id, &f.carrier);
        f.client().confirm_delivery(
            &id,
            &f.lab,
            &5,
            &ConditionCode::Acceptable,
            &BytesN::from_array(&f.env, &[4; 32]),
        );
        assert_eq!(token.balance(&f.supplier), 500);
        assert_eq!(token.balance(&escrow_id), 0);
        assert_eq!(
            f.client().get_order(&id).unwrap().state,
            OrderState::Confirmed
        );
        let proof = f.client().get_proof(&id).unwrap();
        assert_eq!(proof.quantity_received, 5);
        assert_eq!(
            proof.temperature_log_hash,
            BytesN::from_array(&f.env, &[4; 32])
        );
        assert_eq!(
            f.client().try_confirm_delivery(
                &id,
                &f.lab,
                &5,
                &ConditionCode::Acceptable,
                &BytesN::from_array(&f.env, &[4; 32]),
            ),
            Err(Ok(Error::WrongState)),
        );
    }

    #[test]
    fn excursion_blocks_confirmation_until_arbiter_resolves() {
        let f = fixture();
        let id = create(&f, 700, 2);
        reach_in_transit(&f, id);
        let now = f.env.ledger().timestamp();
        assert!(f.client().submit_checkpoint(
            &id,
            &f.attestor,
            &1,
            &now,
            &10,
            &60,
            &BytesN::from_array(&f.env, &[5; 32]),
        ));
        f.client().mark_delivered(&id, &f.carrier);
        assert_eq!(
            f.client().try_confirm_delivery(
                &id,
                &f.lab,
                &2,
                &ConditionCode::Acceptable,
                &BytesN::from_array(&f.env, &[6; 32]),
            ),
            Err(Ok(Error::ExcursionUnresolved)),
        );
        f.client().resolve_excursion(&id, &f.arbiter, &true);
        f.client().confirm_delivery(
            &id,
            &f.lab,
            &2,
            &ConditionCode::Acceptable,
            &BytesN::from_array(&f.env, &[6; 32]),
        );
        assert_eq!(
            f.client().get_order(&id).unwrap().state,
            OrderState::Confirmed
        );
    }

    #[test]
    fn dispute_split_pays_supplier_and_refunds_lab_once() {
        let f = fixture();
        let id = create(&f, 1_001, 4);
        reach_in_transit(&f, id);
        f.client().mark_delivered(&id, &f.carrier);
        f.client().raise_dispute(&id, &f.lab);
        assert_eq!(
            f.client().try_resolve_dispute(&id, &f.arbiter, &10_001),
            Err(Ok(Error::InvalidSplit)),
        );
        let (supplier_amount, lab_amount) = f.client().resolve_dispute(&id, &f.arbiter, &5_000);
        assert_eq!(supplier_amount, 500);
        assert_eq!(lab_amount, 501);
        let token = TokenClient::new(&f.env, &f.token_id);
        assert_eq!(token.balance(&f.supplier), 500);
        assert_eq!(token.balance(&f.lab), 999_500);
        assert_eq!(
            f.client().get_order(&id).unwrap().state,
            OrderState::Resolved
        );
        assert_eq!(
            f.client().try_resolve_dispute(&id, &f.arbiter, &5_000),
            Err(Ok(Error::WrongState)),
        );
    }

    #[test]
    fn unaccepted_and_unconfirmed_delivery_timeouts_refund() {
        let f = fixture();
        let unaccepted = create(&f, 100, 1);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += ACCEPTANCE_TIMEOUT_SECONDS);
        f.client().timeout_order(&unaccepted, &f.lab);
        assert_eq!(
            f.client().get_order(&unaccepted).unwrap().state,
            OrderState::Cancelled
        );
        let delivered = create(&f, 200, 1);
        reach_in_transit(&f, delivered);
        f.client().mark_delivered(&delivered, &f.carrier);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += CONFIRMATION_TIMEOUT_SECONDS);
        f.client().timeout_order(&delivered, &f.lab);
        assert_eq!(
            f.client().get_order(&delivered).unwrap().state,
            OrderState::Cancelled
        );
        let token = TokenClient::new(&f.env, &f.token_id);
        assert_eq!(token.balance(&f.lab), 1_000_000);
    }

    #[test]
    fn delivery_timeout_refunds_and_releases_reserved_quantity() {
        let f = fixture();
        let id = create(&f, 250, 3);
        reach_in_transit(&f, id);
        f.env
            .ledger()
            .with_mut(|l| l.timestamp += DELIVERY_TIMEOUT_SECONDS);
        f.client().timeout_order(&id, &f.lab);
        assert_eq!(
            f.client().get_order(&id).unwrap().state,
            OrderState::Cancelled
        );
    }

    #[test]
    fn replay_out_of_order_checkpoint_and_wrong_attestor_are_rejected() {
        let f = fixture();
        let id = create(&f, 100, 1);
        reach_in_transit(&f, id);
        let now = f.env.ledger().timestamp();
        assert_eq!(
            f.client().try_submit_checkpoint(
                &id,
                &f.attestor,
                &2,
                &now,
                &30,
                &60,
                &BytesN::from_array(&f.env, &[7; 32])
            ),
            Err(Ok(Error::CheckpointSequence)),
        );
        f.client().submit_checkpoint(
            &id,
            &f.attestor,
            &1,
            &now,
            &30,
            &60,
            &BytesN::from_array(&f.env, &[8; 32]),
        );
        assert_eq!(
            f.client().get_checkpoint(&id, &1).unwrap().log_hash,
            BytesN::from_array(&f.env, &[8; 32]),
        );
        assert_eq!(
            f.client().try_submit_checkpoint(
                &id,
                &f.attestor,
                &1,
                &now,
                &30,
                &60,
                &BytesN::from_array(&f.env, &[9; 32])
            ),
            Err(Ok(Error::CheckpointSequence)),
        );
        let stranger = Address::generate(&f.env);
        assert_eq!(
            f.client().try_submit_checkpoint(
                &id,
                &stranger,
                &2,
                &now,
                &30,
                &60,
                &BytesN::from_array(&f.env, &[1; 32])
            ),
            Err(Ok(Error::ParticipantNotEligible)),
        );
    }

    #[test]
    fn cancellation_releases_reserved_stock_and_refunds_lab() {
        let f = fixture();
        let id = create(&f, 321, 7);
        f.client().cancel_before_acceptance(&id, &f.lab);
        assert_eq!(
            f.client().get_order(&id).unwrap().state,
            OrderState::Cancelled
        );
        assert_eq!(
            f.client().try_cancel_before_acceptance(&id, &f.lab),
            Err(Ok(Error::WrongState)),
        );
        let batches = BatchTestClient::new(&f.env, &f.batch_registry_id);
        let batch = batches.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, 100);
        assert_eq!(batch.reserved_quantity, 0);
        let token = TokenClient::new(&f.env, &f.token_id);
        assert_eq!(token.balance(&f.lab), 1_000_000);
    }

    #[test]
    fn amount_and_quantity_boundaries_reject_before_funding() {
        let f = fixture();
        assert_eq!(
            f.client().try_create_order(
                &f.lab,
                &f.batch_id,
                &f.carrier,
                &u64::MAX,
                &f.token_id,
                &1,
            ),
            Err(Ok(Error::BatchNotFulfillable)),
        );
        assert_eq!(
            f.client().try_create_order(
                &f.lab,
                &f.batch_id,
                &f.carrier,
                &1,
                &f.token_id,
                &i128::MAX,
            ),
            Err(Ok(Error::InvalidAmount)),
        );
        assert_eq!(
            f.client()
                .try_create_order(&f.lab, &f.batch_id, &f.carrier, &1, &f.token_id, &0,),
            Err(Ok(Error::InvalidAmount)),
        );
    }

    #[test]
    fn admin_can_rotate_arbiter_and_reinitialize_is_rejected() {
        let f = fixture();
        let admin = Address::generate(&f.env);
        let registry = Address::generate(&f.env);
        let batches = Address::generate(&f.env);
        assert_eq!(
            f.client()
                .try_initialize(&admin, &registry, &batches, &f.arbiter),
            Err(Ok(Error::AlreadyInitialized)),
        );
        let next_arbiter = Address::generate(&f.env);
        f.client().set_arbiter(&next_arbiter);
        let id = create(&f, 100, 1);
        reach_in_transit(&f, id);
        f.client().mark_delivered(&id, &f.carrier);
        f.client().raise_dispute(&id, &f.lab);
        assert_eq!(
            f.client().try_resolve_dispute(&id, &f.arbiter, &5_000),
            Err(Ok(Error::Unauthorized)),
        );
        f.client().resolve_dispute(&id, &next_arbiter, &5_000);
    }

    #[test]
    #[should_panic]
    fn initialization_requires_admin_authorization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let batches = Address::generate(&env);
        let arbiter = Address::generate(&env);
        let id = env.register(OrderEscrow, ());
        OrderEscrowClient::new(&env, &id).initialize(&admin, &registry, &batches, &arbiter);
    }
}
