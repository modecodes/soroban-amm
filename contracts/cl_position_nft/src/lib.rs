//! Concentrated-liquidity position NFT scaffold.
//!
//! The contract stores ownership and approval keys for LP position NFTs minted
//! by the concentrated-liquidity pool. This issue establishes the storage
//! layout and one-time initialization used by the later mint/transfer/query
//! implementation work.

#![no_std]

use soroban_sdk::{contract, contracterror, contractimpl, contracttype, Address, Env, Vec};

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NftError {
    AlreadyInitialized = 1,
    Unauthorized = 2,
    TokenNotFound = 3,
    NotOwnerOrApproved = 4,
    InvalidReceiver = 5,
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DataKey {
    Admin,
    ClPool,
    NextTokenId,
    Owner(u64),
    Approved(u64),
    OperatorApproval(Address, Address),
    TokenPosition(u64),
    OwnedTokens(Address),
}

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PositionMeta {
    pub pool: Address,
    pub lower_tick: i32,
    pub upper_tick: i32,
}

#[contract]
pub struct NftContract;

#[contractimpl]
impl NftContract {
    /// Initialize the position NFT contract.
    ///
    /// Global state is stored in instance storage. Per-token and per-owner
    /// state introduced by later lifecycle functions is represented by
    /// [`DataKey`] variants intended for persistent storage.
    pub fn initialize(env: Env, admin: Address, cl_pool: Address) -> Result<(), NftError> {
        if env.storage().instance().has(&DataKey::Admin) {
            return Err(NftError::AlreadyInitialized);
        }

        env.storage().instance().set(&DataKey::Admin, &admin);
        env.storage().instance().set(&DataKey::ClPool, &cl_pool);
        env.storage().instance().set(&DataKey::NextTokenId, &0_u64);

        Ok(())
    }

    pub fn admin(env: Env) -> Address {
        env.storage().instance().get(&DataKey::Admin).unwrap()
    }

    pub fn cl_pool(env: Env) -> Address {
        env.storage().instance().get(&DataKey::ClPool).unwrap()
    }

    pub fn next_token_id(env: Env) -> u64 {
        env.storage()
            .instance()
            .get(&DataKey::NextTokenId)
            .unwrap_or(0)
    }

    pub fn owned_tokens(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .persistent()
            .get(&DataKey::OwnedTokens(owner))
            .unwrap_or(Vec::new(&env))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    struct Setup {
        env: Env,
        client: NftContractClient<'static>,
        admin: Address,
        cl_pool: Address,
    }

    fn setup() -> Setup {
        let env = Env::default();
        let contract_id = env.register_contract(None, NftContract);
        let client = NftContractClient::new(&env, &contract_id);
        let admin = Address::generate(&env);
        let cl_pool = Address::generate(&env);

        Setup {
            env,
            client,
            admin,
            cl_pool,
        }
    }

    #[test]
    fn initialize_stores_global_state_once() {
        let s = setup();

        s.client.initialize(&s.admin, &s.cl_pool);

        assert_eq!(s.client.admin(), s.admin);
        assert_eq!(s.client.cl_pool(), s.cl_pool);
        assert_eq!(s.client.next_token_id(), 0);
    }

    #[test]
    fn initialize_twice_returns_already_initialized() {
        let s = setup();
        s.client.initialize(&s.admin, &s.cl_pool);

        let other_admin = Address::generate(&s.env);
        let other_pool = Address::generate(&s.env);
        let err = s
            .client
            .try_initialize(&other_admin, &other_pool)
            .unwrap_err()
            .unwrap();

        assert_eq!(err, NftError::AlreadyInitialized);
    }

    #[test]
    fn owned_tokens_defaults_to_empty_persistent_vec() {
        let s = setup();
        s.client.initialize(&s.admin, &s.cl_pool);
        let owner = Address::generate(&s.env);

        assert_eq!(s.client.owned_tokens(&owner).len(), 0);
    }
}
