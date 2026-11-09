#![no_std]
// Keep the tuple event encoding stable until downstream event schemas are versioned.
#![allow(deprecated)]

use soroban_sdk::{
    contract, contracterror, contractimpl, contracttype, symbol_short, Address, BytesN, Env,
};

const INSTANCE_TTL_THRESHOLD: u32 = 100_000;
const INSTANCE_TTL_EXTEND_TO: u32 = 500_000;
const ENTRY_TTL_THRESHOLD: u32 = 100_000;
const ENTRY_TTL_EXTEND_TO: u32 = 500_000;
const MAX_PARTICIPANTS: u32 = 10_000;

#[contracterror]
#[derive(Copy, Clone, Debug, Eq, PartialEq)]
#[repr(u32)]
pub enum Error {
    AlreadyInitialized = 1,
    NotInitialized = 2,
    Unauthorized = 3,
    ParticipantMissing = 4,
    InvalidExpiry = 5,
    NoPendingAdmin = 6,
    WrongPendingAdmin = 7,
    CapacityReached = 8,
    Arithmetic = 9,
}

#[contracttype]
#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Role {
    Lab,
    Supplier,
    Logistics,
    Attestor,
}

#[contracttype]
#[derive(Clone)]
struct Licence {
    document_hash: BytesN<32>,
    expires_at: u64,
    revoked: bool,
}

#[contracttype]
#[derive(Clone)]
enum DataKey {
    Admin,
    PendingAdmin,
    Verifier,
    ParticipantCount,
    Role(Address),
    Licence(Address),
}

#[contract]
pub struct ParticipantRegistry;

fn touch_instance(env: &Env) {
    env.storage()
        .instance()
        .extend_ttl(INSTANCE_TTL_THRESHOLD, INSTANCE_TTL_EXTEND_TO);
}

fn touch_entry(env: &Env, key: &DataKey) {
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

fn verifier(env: &Env) -> Result<Address, Error> {
    env.storage()
        .instance()
        .get(&DataKey::Verifier)
        .ok_or(Error::NotInitialized)
}

#[contractimpl]
impl ParticipantRegistry {
    pub fn initialize(
        env: Env,
        initial_admin: Address,
        initial_verifier: Address,
    ) -> Result<(), Error> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(Error::AlreadyInitialized);
        }
        initial_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Admin, &initial_admin);
        env.storage()
            .instance()
            .set(&DataKey::Verifier, &initial_verifier);
        env.storage()
            .instance()
            .set(&DataKey::ParticipantCount, &0_u32);
        touch_instance(&env);
        env.events()
            .publish((symbol_short!("init"), initial_admin), initial_verifier);
        Ok(())
    }

    pub fn admin(env: Env) -> Result<Address, Error> {
        touch_instance(&env);
        admin(&env)
    }

    pub fn verifier(env: Env) -> Result<Address, Error> {
        touch_instance(&env);
        verifier(&env)
    }

    pub fn set_verifier(env: Env, new_verifier: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Verifier, &new_verifier);
        env.events()
            .publish((symbol_short!("verifier"), current_admin), new_verifier);
        Ok(())
    }

    pub fn propose_admin(env: Env, proposed_admin: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::PendingAdmin, &proposed_admin);
        env.events()
            .publish((symbol_short!("admprop"), current_admin), proposed_admin);
        Ok(())
    }

    pub fn accept_admin(env: Env, new_admin: Address) -> Result<(), Error> {
        touch_instance(&env);
        let pending: Address = env
            .storage()
            .instance()
            .get(&DataKey::PendingAdmin)
            .ok_or(Error::NoPendingAdmin)?;
        if new_admin != pending {
            return Err(Error::WrongPendingAdmin);
        }
        new_admin.require_auth();
        let previous = admin(&env)?;
        env.storage().instance().set(&DataKey::Admin, &new_admin);
        env.storage().instance().remove(&DataKey::PendingAdmin);
        env.events()
            .publish((symbol_short!("admin"), previous), new_admin);
        Ok(())
    }

    pub fn register_participant(env: Env, participant: Address, role: Role) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        let key = DataKey::Role(participant.clone());
        if !env.storage().persistent().has(&key) {
            let count: u32 = env
                .storage()
                .instance()
                .get(&DataKey::ParticipantCount)
                .ok_or(Error::NotInitialized)?;
            if count >= MAX_PARTICIPANTS {
                return Err(Error::CapacityReached);
            }
            let next = count.checked_add(1).ok_or(Error::Arithmetic)?;
            env.storage()
                .instance()
                .set(&DataKey::ParticipantCount, &next);
        }
        env.storage().persistent().set(&key, &role);
        touch_entry(&env, &key);
        env.events()
            .publish((symbol_short!("role"), participant), role);
        Ok(())
    }

    pub fn remove_participant(env: Env, participant: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_admin = admin(&env)?;
        current_admin.require_auth();
        let role_key = DataKey::Role(participant.clone());
        if !env.storage().persistent().has(&role_key) {
            return Err(Error::ParticipantMissing);
        }
        env.storage().persistent().remove(&role_key);
        let licence_key = DataKey::Licence(participant.clone());
        if env.storage().persistent().has(&licence_key) {
            env.storage().persistent().remove(&licence_key);
        }
        env.events()
            .publish((symbol_short!("removed"), current_admin), participant);
        Ok(())
    }

    pub fn get_role(env: Env, participant: Address) -> Option<Role> {
        touch_instance(&env);
        let key = DataKey::Role(participant);
        let role = env.storage().persistent().get(&key);
        if role.is_some() {
            touch_entry(&env, &key);
        }
        role
    }

    pub fn attest_licence(
        env: Env,
        participant: Address,
        document_hash: BytesN<32>,
        expires_at: u64,
    ) -> Result<(), Error> {
        touch_instance(&env);
        let current_verifier = verifier(&env)?;
        current_verifier.require_auth();
        let role_key = DataKey::Role(participant.clone());
        if !env.storage().persistent().has(&role_key) {
            return Err(Error::ParticipantMissing);
        }
        if expires_at <= env.ledger().timestamp() {
            return Err(Error::InvalidExpiry);
        }
        let key = DataKey::Licence(participant.clone());
        let licence = Licence {
            document_hash: document_hash.clone(),
            expires_at,
            revoked: false,
        };
        env.storage().persistent().set(&key, &licence);
        touch_entry(&env, &key);
        env.events().publish(
            (symbol_short!("attest"), participant),
            (document_hash, expires_at),
        );
        Ok(())
    }

    pub fn revoke_licence(env: Env, participant: Address) -> Result<(), Error> {
        touch_instance(&env);
        let current_verifier = verifier(&env)?;
        current_verifier.require_auth();
        let key = DataKey::Licence(participant.clone());
        let mut licence: Licence = env
            .storage()
            .persistent()
            .get(&key)
            .ok_or(Error::ParticipantMissing)?;
        licence.revoked = true;
        env.storage().persistent().set(&key, &licence);
        touch_entry(&env, &key);
        env.events()
            .publish((symbol_short!("revoke"), participant), licence.expires_at);
        Ok(())
    }

    pub fn is_verified(env: Env, participant: Address) -> bool {
        touch_instance(&env);
        let role_key = DataKey::Role(participant.clone());
        if !env.storage().persistent().has(&role_key) {
            return false;
        }
        touch_entry(&env, &role_key);
        let licence_key = DataKey::Licence(participant);
        let Some(licence) = env.storage().persistent().get::<_, Licence>(&licence_key) else {
            return false;
        };
        touch_entry(&env, &licence_key);
        !licence.revoked && licence.expires_at > env.ledger().timestamp()
    }

    pub fn licence_document_hash(env: Env, participant: Address) -> Option<BytesN<32>> {
        touch_instance(&env);
        let key = DataKey::Licence(participant);
        let licence = env.storage().persistent().get::<_, Licence>(&key);
        if let Some(record) = licence {
            touch_entry(&env, &key);
            Some(record.document_hash)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod test {
    use super::{Error, ParticipantRegistry, ParticipantRegistryClient, Role};
    use soroban_sdk::{
        testutils::{Address as _, Ledger},
        Address, BytesN, Env,
    };

    struct Fixture {
        env: Env,
        admin: Address,
        verifier: Address,
        contract_id: Address,
    }

    impl Fixture {
        fn client(&self) -> ParticipantRegistryClient<'_> {
            ParticipantRegistryClient::new(&self.env, &self.contract_id)
        }
    }

    fn setup() -> Fixture {
        let env = Env::default();
        env.mock_all_auths();
        let admin = Address::generate(&env);
        let verifier = Address::generate(&env);
        let contract_id = env.register(ParticipantRegistry, ());
        ParticipantRegistryClient::new(&env, &contract_id).initialize(&admin, &verifier);
        Fixture {
            env,
            admin,
            verifier,
            contract_id,
        }
    }

    #[test]
    fn roles_attest_expiry_revoke_and_remove() {
        let f = setup();
        let client = f.client();
        let supplier = Address::generate(&f.env);
        let hash = BytesN::from_array(&f.env, &[7; 32]);
        client.register_participant(&supplier, &Role::Supplier);
        assert_eq!(client.get_role(&supplier), Some(Role::Supplier));
        client.attest_licence(&supplier, &hash, &(f.env.ledger().timestamp() + 10));
        assert!(client.is_verified(&supplier));
        assert_eq!(client.licence_document_hash(&supplier), Some(hash));
        f.env.ledger().with_mut(|li| li.timestamp += 10);
        assert!(!client.is_verified(&supplier));
        client.attest_licence(
            &supplier,
            &BytesN::from_array(&f.env, &[8; 32]),
            &(f.env.ledger().timestamp() + 10),
        );
        assert!(client.is_verified(&supplier));
        client.revoke_licence(&supplier);
        assert!(!client.is_verified(&supplier));
        client.remove_participant(&supplier);
        assert_eq!(client.get_role(&supplier), None);
    }

    #[test]
    fn admin_transfer_requires_acceptance_and_expiry_is_checked() {
        let f = setup();
        let client = f.client();
        let next = Address::generate(&f.env);
        let participant = Address::generate(&f.env);
        client.propose_admin(&next);
        assert_eq!(client.admin(), f.admin);
        client.accept_admin(&next);
        assert_eq!(client.admin(), next);
        client.register_participant(&participant, &Role::Lab);
        let err = client.try_attest_licence(
            &participant,
            &BytesN::from_array(&f.env, &[1; 32]),
            &f.env.ledger().timestamp(),
        );
        assert_eq!(err, Err(Ok(Error::InvalidExpiry)));
    }

    #[test]
    fn initialization_is_one_time_and_verifier_can_rotate() {
        let f = setup();
        let client = f.client();
        assert_eq!(
            client.try_initialize(&f.admin, &f.verifier),
            Err(Ok(Error::AlreadyInitialized))
        );
        let next_verifier = Address::generate(&f.env);
        client.set_verifier(&next_verifier);
        assert_eq!(client.verifier(), next_verifier);
    }

    #[test]
    #[should_panic]
    fn initialization_requires_admin_authorization() {
        let env = Env::default();
        let admin = Address::generate(&env);
        let verifier = Address::generate(&env);
        let id = env.register(ParticipantRegistry, ());
        ParticipantRegistryClient::new(&env, &id).initialize(&admin, &verifier);
    }
}
