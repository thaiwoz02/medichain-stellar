#![no_std]
// Keep the tuple event encoding stable until downstream event schemas are versioned.
#![allow(deprecated)]
// The contract ABI keeps independently validated batch fields explicit.
#![allow(clippy::too_many_arguments)]

use soroban_sdk::{
    contract, contractclient, contracterror, contractimpl, contracttype, symbol_short, Address,
    BytesN, Env, String,
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

const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
const INSTANCE_TTL_EXTEND_TO: u32 = 500_000;
const ENTRY_TTL_THRESHOLD: u32 = 100_000;
const ENTRY_TTL_EXTEND_TO: u32 = 500_000;
const MAX_BATCH_ID_BYTES: u32 = 64;
const MAX_BATCHES: u32 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    InvalidBatch = 4,
    SupplierNotVerified = 5,
    BatchExists = 6,
    BatchMissing = 7,
    InvalidQuantity = 8,
    InsufficientQuantity = 9,
    Arithmetic = 10,
    EscrowNotConfigured = 11,
    CapacityReached = 12,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Batch {
    pub supplier: Address,
    pub product_spec_hash: BytesN<32>,
    pub storage_min_tenths_c: i32,
    pub storage_max_tenths_c: i32,
    pub batch_id: String,
    pub expires_at: u64,
    pub available_quantity: u64,
    pub reserved_quantity: u64,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    Registry,
    Escrow,
    BatchCount,
    Batch(String),
}

#[contract]
pub struct ProductBatchRegistry;

fn touch_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

fn batch_key(batch_id: &String) -> DataKey {
    DataKey::Batch(batch_id.clone())
}

fn touch_batch(env: &Env, key: &DataKey) {
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

fn get_escrow(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Escrow)
        .ok_or(Error::EscrowNotConfigured)
}

fn read_batch(env: &Env, batch_id: &String) -> Result<Batch, Error> {
    let key = batch_key(batch_id);
    let batch = env
        .storage()
        .persistent()
        .get(&key)
        .ok_or(Error::BatchMissing)?;
    touch_batch(env, &key);
    Ok(batch)
}

#[contractimpl]
impl ProductBatchRegistry {
    pub fn initialize(env: Env, initial_admin: Address, registry: Address) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        initial_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Admin, &initial_admin);
        env.storage().instance().set(&DataKey::Registry, &registry);
        env.storage().instance().set(&DataKey::BatchCount, &0_u32);
        touch_instance(&env);
        env.events()
            .publish((symbol_short!("init"), initial_admin), registry);
        Ok(())
    }

    pub fn set_escrow(env: Env, new_escrow: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        env.storage().instance().set(&DataKey::Escrow, &new_escrow);
        env.events()
            .publish((symbol_short!("escrow"), current_admin), new_escrow);
        Ok(())
    }

    pub fn register_batch(
        env: Env,
        supplier: Address,
        batch_id: String,
        product_spec_hash: BytesN<32>,
        storage_min_tenths_c: i32,
        storage_max_tenths_c: i32,
        expires_at: u64,
        quantity: u64,
    ) -> Result<(), Error> {
        touch_instance(&env);
        supplier.require_auth();
        if batch_id.is_empty() || batch_id.len() > MAX_BATCH_ID_BYTES {
            return Err(Error::InvalidBatch);
        }
        if storage_min_tenths_c >= storage_max_tenths_c || expires_at <= env.ledger().timestamp() {
            return Err(Error::InvalidBatch);
        }
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        let registry: Address = env
            .storage()
            .instance()
            .get(&DataKey::Registry)
            .ok_or(Error::NotInitialized)?;
        let registry_client = ParticipantRegistryClient::new(&env, &registry);
        if registry_client.get_role(&supplier) != Some(Role::Supplier)
            || !registry_client.is_verified(&supplier)
        {
            return Err(Error::SupplierNotVerified);
        }
        let key = batch_key(&batch_id);
        if env.storage().persistent().has(&key) {
            return Err(Error::BatchExists);
        }
        let count: u32 = env
            .storage()
            .instance()
            .get(&DataKey::BatchCount)
            .ok_or(Error::NotInitialized)?;
        if count >= MAX_BATCHES {
            return Err(Error::CapacityReached);
        }
        let next_count = count.checked_add(1).ok_or(Error::Arithmetic)?;
        let batch = Batch {
            supplier: supplier.clone(),
            product_spec_hash,
            storage_min_tenths_c,
            storage_max_tenths_c,
            batch_id: batch_id.clone(),
            expires_at,
            available_quantity: quantity,
            reserved_quantity: 0,
        };
        env.storage().persistent().set(&key, &batch);
        env.storage()
            .instance()
            .set(&DataKey::BatchCount, &next_count);
        touch_batch(&env, &key);
        env.events().publish(
            (symbol_short!("batch"), supplier, batch_id),
            (
                storage_min_tenths_c,
                storage_max_tenths_c,
                expires_at,
                quantity,
            ),
        );
        Ok(())
    }

    pub fn get_batch(env: Env, batch_id: String) -> Option<Batch> {
        touch_instance(&env);
        let key = batch_key(&batch_id);
        let batch = env.storage().persistent().get(&key);
        if batch.is_some() {
            touch_batch(&env, &key);
        }
        batch
    }

    pub fn order_terms(env: Env, batch_id: String) -> Option<(Address, i32, i32)> {
        touch_instance(&env);
        let batch = read_batch(&env, &batch_id).ok()?;
        Some((
            batch.supplier,
            batch.storage_min_tenths_c,
            batch.storage_max_tenths_c,
        ))
    }

    pub fn is_fulfillable(env: Env, batch_id: String, quantity: u64) -> bool {
        touch_instance(&env);
        let Ok(batch) = read_batch(&env, &batch_id) else {
            return false;
        };
        if batch.expires_at <= env.ledger().timestamp()
            || batch.storage_min_tenths_c >= batch.storage_max_tenths_c
            || quantity == 0
            || batch.available_quantity < quantity
        {
            return false;
        }
        let Some(registry) = env
            .storage()
            .instance()
            .get::<_, Address>(&DataKey::Registry)
        else {
            return false;
        };
        let client = ParticipantRegistryClient::new(&env, &registry);
        client.get_role(&batch.supplier) == Some(Role::Supplier)
            && client.is_verified(&batch.supplier)
    }

    pub fn reserve_batch(env: Env, batch_id: String, quantity: u64) -> Result<(), Error> {
        touch_instance(&env);
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        get_escrow(&env)?.require_auth();
        let mut batch = read_batch(&env, &batch_id)?;
        if batch.available_quantity < quantity {
            return Err(Error::InsufficientQuantity);
        }
        batch.available_quantity = batch
            .available_quantity
            .checked_sub(quantity)
            .ok_or(Error::Arithmetic)?;
        batch.reserved_quantity = batch
            .reserved_quantity
            .checked_add(quantity)
            .ok_or(Error::Arithmetic)?;
        let key = batch_key(&batch_id);
        env.storage().persistent().set(&key, &batch);
        touch_batch(&env, &key);
        env.events().publish(
            (symbol_short!("reserve"), batch_id),
            (quantity, batch.available_quantity, batch.reserved_quantity),
        );
        Ok(())
    }

    pub fn release_batch(env: Env, batch_id: String, quantity: u64) -> Result<(), Error> {
        touch_instance(&env);
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        get_escrow(&env)?.require_auth();
        let mut batch = read_batch(&env, &batch_id)?;
        if batch.reserved_quantity < quantity {
            return Err(Error::InsufficientQuantity);
        }
        batch.reserved_quantity = batch
            .reserved_quantity
            .checked_sub(quantity)
            .ok_or(Error::Arithmetic)?;
        batch.available_quantity = batch
            .available_quantity
            .checked_add(quantity)
            .ok_or(Error::Arithmetic)?;
        let key = batch_key(&batch_id);
        env.storage().persistent().set(&key, &batch);
        touch_batch(&env, &key);
        env.events().publish(
            (symbol_short!("release"), batch_id),
            (quantity, batch.available_quantity, batch.reserved_quantity),
        );
        Ok(())
    }

    pub fn fulfill_reserved(env: Env, batch_id: String, quantity: u64) -> Result<(), Error> {
        touch_instance(&env);
        if quantity == 0 {
            return Err(Error::InvalidQuantity);
        }
        get_escrow(&env)?.require_auth();
        let mut batch = read_batch(&env, &batch_id)?;
        if batch.reserved_quantity < quantity {
            return Err(Error::InsufficientQuantity);
        }
        batch.reserved_quantity = batch
            .reserved_quantity
            .checked_sub(quantity)
            .ok_or(Error::Arithmetic)?;
        let key = batch_key(&batch_id);
        env.storage().persistent().set(&key, &batch);
        touch_batch(&env, &key);
        env.events().publish(
            (symbol_short!("fulfill"), batch_id),
            (quantity, batch.available_quantity, batch.reserved_quantity),
        );
        Ok(())
    }
}

#[cfg(test)]
mod test {
    use super::{Error, ProductBatchRegistry, ProductBatchRegistryClient};
    use participant_registry::{ParticipantRegistry, ParticipantRegistryClient, Role};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env, String,
    };

    struct Fixture {
        env: Env,
        admin: Address,
        supplier: Address,
        registry_id: Address,
        contract_id: Address,
        batch_id: String,
    }

    impl Fixture {
        fn client(&self) -> ProductBatchRegistryClient<'_> {
            ProductBatchRegistryClient::new(&self.env, &self.contract_id)
        }
    }

    fn setup(quantity: u64, expiry_delta: u64) -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let verifier = Address::generate(&env);
        let supplier = Address::generate(&env);
        let registry_id = env.register(ParticipantRegistry, ());
        let registry = ParticipantRegistryClient::new(&env, &registry_id);
        registry.initialize(&admin, &verifier);
        registry.register_participant(&supplier, &Role::Supplier);
        registry.attest_licence(
            &supplier,
            &BytesN::from_array(&env, &[4; 32]),
            &(env.ledger().timestamp() + 1_000),
        );
        let contract_id = env.register(ProductBatchRegistry, ());
        let client = ProductBatchRegistryClient::new(&env, &contract_id);
        client.initialize(&admin, &registry_id);
        client.set_escrow(&Address::generate(&env));
        let batch_id = String::from_str(&env, "BATCH-1");
        client.register_batch(
            &supplier,
            &batch_id,
            &BytesN::from_array(&env, &[5; 32]),
            &20,
            &80,
            &(env.ledger().timestamp() + expiry_delta),
            &quantity,
        );
        Fixture {
            env,
            admin,
            supplier,
            registry_id,
            contract_id,
            batch_id,
        }
    }

    #[test]
    fn verified_supplier_registers_and_expiry_changes_fulfillability() {
        let f = setup(10, 100);
        let client = f.client();
        assert!(client.is_fulfillable(&f.batch_id, &10));
        assert!(!client.is_fulfillable(&f.batch_id, &11));
        assert_eq!(
            client.get_batch(&f.batch_id).unwrap().available_quantity,
            10
        );
        f.env.ledger().with_mut(|li| li.timestamp += 100);
        assert!(!client.is_fulfillable(&f.batch_id, &1));
    }

    #[test]
    fn invalid_temperature_range_and_zero_quantity_are_rejected() {
        let f = setup(10, 1_000);
        let client = f.client();
        let bad_id = String::from_str(&f.env, "BAD-RANGE");
        assert_eq!(
            client.try_register_batch(
                &f.supplier,
                &bad_id,
                &BytesN::from_array(&f.env, &[6; 32]),
                &80,
                &20,
                &(f.env.ledger().timestamp() + 100),
                &5,
            ),
            Err(Ok(Error::InvalidBatch)),
        );
        let zero_id = String::from_str(&f.env, "ZERO-QTY");
        assert_eq!(
            client.try_register_batch(
                &f.supplier,
                &zero_id,
                &BytesN::from_array(&f.env, &[7; 32]),
                &20,
                &80,
                &(f.env.ledger().timestamp() + 100),
                &0,
            ),
            Err(Ok(Error::InvalidQuantity)),
        );
        assert!(!client.is_fulfillable(&String::from_str(&f.env, "MISSING"), &1));
    }

    #[test]
    fn escrow_reservations_are_checked_and_can_be_released_or_fulfilled() {
        let f = setup(10, 1_000);
        let client = f.client();
        assert_eq!(
            client.try_reserve_batch(&f.batch_id, &11),
            Err(Ok(Error::InsufficientQuantity)),
        );
        client.reserve_batch(&f.batch_id, &6);
        let batch = client.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, 4);
        assert_eq!(batch.reserved_quantity, 6);
        client.release_batch(&f.batch_id, &2);
        let batch = client.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, 6);
        assert_eq!(batch.reserved_quantity, 4);
        assert_eq!(
            client.try_release_batch(&f.batch_id, &5),
            Err(Ok(Error::InsufficientQuantity)),
        );
        client.fulfill_reserved(&f.batch_id, &4);
        let batch = client.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, 6);
        assert_eq!(batch.reserved_quantity, 0);
        assert_eq!(
            client.try_fulfill_reserved(&f.batch_id, &1),
            Err(Ok(Error::InsufficientQuantity)),
        );
    }

    #[test]
    fn batch_quantity_accepts_u64_boundary_without_overflow() {
        let f = setup(u64::MAX, 1_000);
        let client = f.client();
        assert!(client.is_fulfillable(&f.batch_id, &u64::MAX));
        client.reserve_batch(&f.batch_id, &u64::MAX);
        let batch = client.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, 0);
        assert_eq!(batch.reserved_quantity, u64::MAX);
        client.release_batch(&f.batch_id, &u64::MAX);
        let batch = client.get_batch(&f.batch_id).unwrap();
        assert_eq!(batch.available_quantity, u64::MAX);
        assert_eq!(batch.reserved_quantity, 0);
    }

    #[test]
    fn initialization_is_one_time_and_order_terms_are_available() {
        let f = setup(10, 1_000);
        let client = f.client();
        assert_eq!(
            client.try_initialize(&f.admin, &f.registry_id),
            Err(Ok(Error::AlreadyInitialized)),
        );
        let (supplier, min, max) = client.order_terms(&f.batch_id).unwrap();
        assert_eq!(supplier, f.supplier);
        assert_eq!((min, max), (20, 80));
    }

    #[test]
    #[should_panic]
    fn initialization_requires_admin_authorization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let registry = Address::generate(&env);
        let id = env.register(ProductBatchRegistry, ());
        ProductBatchRegistryClient::new(&env, &id).initialize(&admin, &registry);
    }
}
