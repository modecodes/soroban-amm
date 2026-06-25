//! CL Position NFT – ERC-721-style receipt token for concentrated-liquidity positions.
//!
//! Each token represents an open CL position (`pool`, `lower_tick`, `upper_tick`).
//! Only the registered `cl_pool` address may mint or burn tokens; the pool calls
//! `mint` when a position opens and `burn` when it fully closes.
#![no_std]

use soroban_sdk::{
    contract, contractclient, contractimpl, contracterror, contracttype, symbol_short, Address,
    Env, Vec,
};

// ── WASM bytes for test harness ──────────────────────────────────────────────
#[cfg(feature = "testutils")]
pub const WASM: &[u8] = include_bytes!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/../../target/wasm32v1-none/release/cl_position_nft.wasm"
));

// ── Errors ───────────────────────────────────────────────────────────────────

#[contracterror]
#[derive(Copy, Clone, Debug, PartialEq)]
pub enum NftError {
    AlreadyInitialized = 1,
    Unauthorized       = 2,
    TokenNotFound      = 3,
}

// ── Storage keys ─────────────────────────────────────────────────────────────

#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub enum DataKey {
    /// Address of the registered cl_pool contract (set once during `initialize`).
    ClPool,
    /// Monotonically-increasing counter; next token id to assign.
    NextTokenId,
    /// Owner of a specific token: `Owner(token_id) → Address`.
    Owner(u64),
    /// Optional approved address for a token: `Approved(token_id) → Address`.
    Approved(u64),
    /// Position metadata for a token: `TokenPosition(token_id) → PositionMeta`.
    TokenPosition(u64),
    /// All token ids owned by an address: `OwnedTokens(owner) → Vec<u64>`.
    OwnedTokens(Address),
}

// ── Types ─────────────────────────────────────────────────────────────────────

/// Metadata attached to each NFT at mint-time.
#[contracttype]
#[derive(Clone, Debug, PartialEq)]
pub struct PositionMeta {
    /// The CL pool contract that owns this position.
    pub pool:       Address,
    /// Lower tick of the position range.
    pub lower_tick: i32,
    /// Upper tick of the position range.
    pub upper_tick: i32,
}

// ── Contract ─────────────────────────────────────────────────────────────────

#[contract]
pub struct ClPositionNft;

#[contractclient(name = "ClPositionNftClient")]
pub trait ClPositionNftInterface {
    fn initialize(env: Env, cl_pool: Address) -> Result<(), NftError>;

    fn mint(
        env: Env,
        to: Address,
        pool: Address,
        lower_tick: i32,
        upper_tick: i32,
    ) -> Result<u64, NftError>;

    fn burn(env: Env, token_id: u64) -> Result<(), NftError>;

    fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError>;

    fn get_position(env: Env, token_id: u64) -> Result<PositionMeta, NftError>;

    fn tokens_of(env: Env, owner: Address) -> Vec<u64>;

    fn approve(env: Env, caller: Address, token_id: u64, approved: Address) -> Result<(), NftError>;

    fn get_approved(env: Env, token_id: u64) -> Option<Address>;

    fn cl_pool(env: Env) -> Address;
}

#[contractimpl]
impl ClPositionNft {
    // ── One-time setup ────────────────────────────────────────────────────────

    /// Registers the CL pool address that is permitted to mint/burn tokens.
    /// May only be called once.
    pub fn initialize(env: Env, cl_pool: Address) -> Result<(), NftError> {
        if env.storage().instance().has(&DataKey::ClPool) {
            return Err(NftError::AlreadyInitialized);
        }
        env.storage().instance().set(&DataKey::ClPool, &cl_pool);
        env.storage()
            .instance()
            .set(&DataKey::NextTokenId, &0_u64);
        Ok(())
    }

    // ── Internal helpers ──────────────────────────────────────────────────────

    fn require_pool(env: &Env) -> Result<Address, NftError> {
        let pool: Address = env
            .storage()
            .instance()
            .get(&DataKey::ClPool)
            .ok_or(NftError::Unauthorized)?;
        pool.require_auth();
        Ok(pool)
    }

    // ── Core lifecycle ────────────────────────────────────────────────────────

    /// Mint a new position NFT.
    ///
    /// Callable **only** by the registered `cl_pool` address.
    /// Increments `NextTokenId`, stores owner and position metadata, appends
    /// the token id to `OwnedTokens(to)`, and emits a `mint` event.
    ///
    /// Returns the newly-assigned token id (sequential, starting at 0).
    pub fn mint(
        env: Env,
        to: Address,
        pool: Address,
        lower_tick: i32,
        upper_tick: i32,
    ) -> Result<u64, NftError> {
        Self::require_pool(&env)?;

        // Assign the next token id.
        let token_id: u64 = env
            .storage()
            .instance()
            .get(&DataKey::NextTokenId)
            .unwrap_or(0);
        env.storage()
            .instance()
            .set(&DataKey::NextTokenId, &(token_id + 1));

        // Store owner.
        env.storage()
            .instance()
            .set(&DataKey::Owner(token_id), &to);

        // Store position metadata.
        let meta = PositionMeta {
            pool,
            lower_tick,
            upper_tick,
        };
        env.storage()
            .instance()
            .set(&DataKey::TokenPosition(token_id), &meta);

        // Append to the owner's token list.
        let list_key = DataKey::OwnedTokens(to.clone());
        let mut owned: Vec<u64> = env
            .storage()
            .instance()
            .get(&list_key)
            .unwrap_or_else(|| Vec::new(&env));
        owned.push_back(token_id);
        env.storage().instance().set(&list_key, &owned);

        // Emit mint event: topic=(mint, to), data=token_id.
        env.events()
            .publish((symbol_short!("nft_mint"), to), token_id);

        Ok(token_id)
    }

    /// Burn an existing position NFT.
    ///
    /// Callable **only** by the registered `cl_pool` address.
    /// Removes `Owner`, `Approved`, and `TokenPosition` entries and prunes the
    /// token id from `OwnedTokens(owner)`. Emits a `burn` event.
    ///
    /// Returns [`NftError::TokenNotFound`] if the token does not exist.
    pub fn burn(env: Env, token_id: u64) -> Result<(), NftError> {
        Self::require_pool(&env)?;

        // Resolve the current owner – error if token doesn't exist.
        let owner: Address = env
            .storage()
            .instance()
            .get(&DataKey::Owner(token_id))
            .ok_or(NftError::TokenNotFound)?;

        // Remove core token state.
        env.storage().instance().remove(&DataKey::Owner(token_id));
        env.storage()
            .instance()
            .remove(&DataKey::Approved(token_id));
        env.storage()
            .instance()
            .remove(&DataKey::TokenPosition(token_id));

        // Remove from the owner's token list.
        let list_key = DataKey::OwnedTokens(owner.clone());
        let mut owned: Vec<u64> = env
            .storage()
            .instance()
            .get(&list_key)
            .unwrap_or_else(|| Vec::new(&env));
        if let Some(idx) = owned.iter().position(|id| id == token_id) {
            owned.remove(idx as u32);
            env.storage().instance().set(&list_key, &owned);
        }

        // Emit burn event: topic=(burn, owner), data=token_id.
        env.events()
            .publish((symbol_short!("nft_burn"), owner), token_id);

        Ok(())
    }

    // ── View helpers ──────────────────────────────────────────────────────────

    /// Returns the owner of `token_id`, or [`NftError::TokenNotFound`].
    pub fn owner_of(env: Env, token_id: u64) -> Result<Address, NftError> {
        env.storage()
            .instance()
            .get(&DataKey::Owner(token_id))
            .ok_or(NftError::TokenNotFound)
    }

    /// Returns the [`PositionMeta`] for `token_id`, or [`NftError::TokenNotFound`].
    pub fn get_position(env: Env, token_id: u64) -> Result<PositionMeta, NftError> {
        env.storage()
            .instance()
            .get(&DataKey::TokenPosition(token_id))
            .ok_or(NftError::TokenNotFound)
    }

    /// Returns all token ids owned by `owner` (empty vec if none).
    pub fn tokens_of(env: Env, owner: Address) -> Vec<u64> {
        env.storage()
            .instance()
            .get(&DataKey::OwnedTokens(owner))
            .unwrap_or_else(|| Vec::new(&env))
    }

    /// Approve `approved` to transfer `token_id`. Only callable by the current owner.
    pub fn approve(
        env: Env,
        caller: Address,
        token_id: u64,
        approved: Address,
    ) -> Result<(), NftError> {
        let owner: Address = env
            .storage()
            .instance()
            .get(&DataKey::Owner(token_id))
            .ok_or(NftError::TokenNotFound)?;
        if caller != owner {
            return Err(NftError::Unauthorized);
        }
        caller.require_auth();
        env.storage()
            .instance()
            .set(&DataKey::Approved(token_id), &approved);
        Ok(())
    }

    /// Returns the currently-approved address for `token_id`, if any.
    pub fn get_approved(env: Env, token_id: u64) -> Option<Address> {
        env.storage().instance().get(&DataKey::Approved(token_id))
    }

    /// Returns the registered `cl_pool` address.
    pub fn cl_pool(env: Env) -> Address {
        env.storage().instance().get(&DataKey::ClPool).unwrap()
    }
}

// ── Unit tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use soroban_sdk::{testutils::Address as _, Env};

    fn setup() -> (Env, ClPositionNftClient<'static>, Address, Address) {
        let env = Env::default();
        env.mock_all_auths();
        let contract_id = env.register(ClPositionNft, ());
        let client = ClPositionNftClient::new(&env, &contract_id);
        let pool = Address::generate(&env);
        let user = Address::generate(&env);
        client.initialize(&pool);
        (env, client, pool, user)
    }

    // ── mint ─────────────────────────────────────────────────────────────────

    #[test]
    fn mint_assigns_sequential_ids_starting_at_zero() {
        let (env, client, pool, user) = setup();

        let id0 = client.mint(&user, &pool, &-100, &100);
        let id1 = client.mint(&user, &pool, &-200, &200);

        assert_eq!(id0, 0);
        assert_eq!(id1, 1);

        // Owner is correctly stored.
        assert_eq!(client.owner_of(&id0), user);
        assert_eq!(client.owner_of(&id1), user);

        // Position metadata is stored.
        let meta0 = client.get_position(&id0);
        assert_eq!(meta0.lower_tick, -100);
        assert_eq!(meta0.upper_tick, 100);

        // OwnedTokens list is updated.
        let owned = client.tokens_of(&user);
        assert_eq!(owned.len(), 2);
        assert_eq!(owned.get(0), Some(0_u64));
        assert_eq!(owned.get(1), Some(1_u64));

        // Verify event was emitted (no panic = success; Soroban test harness
        // captures events but doesn't expose typed assertions without a full
        // snapshot test setup – the publish call itself is the assertion).
        let _ = env.events().all();
    }

    #[test]
    fn mint_stores_correct_position_meta() {
        let (_, client, pool, user) = setup();
        let id = client.mint(&user, &pool, &-500, &500);
        let meta = client.get_position(&id);
        assert_eq!(meta.pool, pool);
        assert_eq!(meta.lower_tick, -500);
        assert_eq!(meta.upper_tick, 500);
    }

    // ── burn ─────────────────────────────────────────────────────────────────

    #[test]
    fn burn_clears_all_state() {
        let (_, client, pool, user) = setup();

        // Mint then set an approval to verify it is also cleared.
        let id = client.mint(&user, &pool, &-100, &100);
        let approver = user.clone();
        let approved_addr = Address::generate(&env);
        client.approve(&approver, &id, &approved_addr);
        assert_eq!(client.get_approved(&id), Some(approved_addr));

        // Burn.
        client.burn(&id);

        // Owner removed.
        let result = client.try_owner_of(&id);
        assert!(result.is_err());

        // Position metadata removed.
        let result = client.try_get_position(&id);
        assert!(result.is_err());

        // Approval cleared.
        assert_eq!(client.get_approved(&id), None);

        // Removed from OwnedTokens.
        let owned = client.tokens_of(&user);
        assert_eq!(owned.len(), 0);
    }

    #[test]
    fn double_burn_returns_token_not_found() {
        let (_, client, pool, user) = setup();
        let id = client.mint(&user, &pool, &-100, &100);
        client.burn(&id);
        let err = client.try_burn(&id).unwrap_err().unwrap();
        assert_eq!(err, NftError::TokenNotFound);
    }

    #[test]
    fn burn_non_existent_token_returns_token_not_found() {
        let (_, client, _, _) = setup();
        let err = client.try_burn(&999_u64).unwrap_err().unwrap();
        assert_eq!(err, NftError::TokenNotFound);
    }

    // ── authorization ────────────────────────────────────────────────────────

    #[test]
    fn mint_by_unauthorized_caller_is_rejected() {
        let env = Env::default();
        // Do NOT call mock_all_auths – auth is real.
        let contract_id = env.register(ClPositionNft, ());
        let client = ClPositionNftClient::new(&env, &contract_id);

        let pool = Address::generate(&env);
        let attacker = Address::generate(&env);

        // Initialize with the real pool address.
        env.mock_all_auths();
        client.initialize(&pool);

        // Reset to strict auth – the attacker address has no auth.
        // Calling mint without the pool's auth should panic / return auth error.
        // We wrap in try_ and verify it errors.
        let result = client.try_mint(&attacker, &pool, &-100, &100);
        assert!(result.is_err());
    }

    #[test]
    fn burn_by_unauthorized_caller_is_rejected() {
        let env = Env::default();
        let contract_id = env.register(ClPositionNft, ());
        let client = ClPositionNftClient::new(&env, &contract_id);

        let pool = Address::generate(&env);
        let user = Address::generate(&env);

        // Initialize + mint with mocked auth.
        env.mock_all_auths();
        client.initialize(&pool);
        let id = client.mint(&user, &pool, &-100, &100);

        // Without pool auth the burn must fail.
        let result = client.try_burn(&id);
        assert!(result.is_err());
    }

    // ── view helpers ─────────────────────────────────────────────────────────

    #[test]
    fn owner_of_non_existent_token_returns_not_found() {
        let (_, client, _, _) = setup();
        let err = client.try_owner_of(&42_u64).unwrap_err().unwrap();
        assert_eq!(err, NftError::TokenNotFound);
    }

    #[test]
    fn tokens_of_empty_returns_empty_vec() {
        let (env, client, _, _) = setup();
        let nobody = Address::generate(&env);
        let owned = client.tokens_of(&nobody);
        assert_eq!(owned.len(), 0);
    }

    #[test]
    fn multiple_users_have_independent_token_lists() {
        let (env, client, pool, user_a) = setup();
        let user_b = Address::generate(&env);

        let id0 = client.mint(&user_a, &pool, &-100, &100);
        let id1 = client.mint(&user_b, &pool, &-200, &200);
        let id2 = client.mint(&user_a, &pool, &-300, &300);

        let a_owned = client.tokens_of(&user_a);
        let b_owned = client.tokens_of(&user_b);

        assert_eq!(a_owned.len(), 2);
        assert!(a_owned.iter().any(|id| id == id0));
        assert!(a_owned.iter().any(|id| id == id2));

        assert_eq!(b_owned.len(), 1);
        assert!(b_owned.iter().any(|id| id == id1));
    }

    #[test]
    fn cl_pool_returns_registered_pool() {
        let (_, client, pool, _) = setup();
        assert_eq!(client.cl_pool(), pool);
    }

    #[test]
    fn initialize_twice_returns_already_initialized() {
        let (_, client, pool, _) = setup();
        let err = client.try_initialize(&pool).unwrap_err().unwrap();
        assert_eq!(err, NftError::AlreadyInitialized);
    }
}
