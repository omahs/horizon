use near_sdk::borsh::{self, BorshDeserialize, BorshSerialize};
use near_sdk::json_types::U128;
use near_sdk::store::LookupSet;
use near_sdk::{assert_one_yocto, env, require};
use near_sdk::{near_bindgen, AccountId};
use near_sdk_contract_tools::owner::OwnerExternal;
use near_sdk_contract_tools::standard::nep141::{
    Nep141, Nep141Controller, Nep141Hook, Nep141Transfer, Nep141Resolver,
};
use near_sdk_contract_tools::{owner::Owner, FungibleToken, Owner};

/// The versioned whitelist item.
#[derive(BorshDeserialize, BorshSerialize)]
enum VersionedAllowList {
    V0(AccountId),
}

impl From<AccountId> for VersionedAllowList {
    fn from(value: AccountId) -> Self {
        Self::V0(value)
    }
}

impl From<VersionedAllowList> for AccountId {
    fn from(value: VersionedAllowList) -> Self {
        match value {
            VersionedAllowList::V0(account_id) => account_id,
        }
    }
}

/// The fungible token contract struct.
#[derive(BorshDeserialize, BorshSerialize, Owner, FungibleToken)]
#[fungible_token(name = "NEAR Horizon", symbol = "NHZN", decimals = 4)]
#[near_bindgen]
pub struct Contract {
    allowlist: LookupSet<VersionedAllowList>,
    fund_amount: u128,
}

impl Default for Contract {
    fn default() -> Self {
        Self {
            allowlist: LookupSet::new(b"allowlist".to_vec()),
            fund_amount: 0,
        }
    }
}

/// A constant representing one NEAR Horizon token (10^4 miliNHZN).
const ONE_NHZN: u128 = 1_000;

#[near_bindgen]
impl Contract {
    #[init]
    pub fn new(owner_id: AccountId, total_supply: U128, fund_amount: Option<U128>) -> Self {
        let mut contract = Self {
            allowlist: LookupSet::new(b"a"),
            fund_amount: fund_amount
                .map(|fund_amount| fund_amount.into())
                .unwrap_or(50_000 * ONE_NHZN),
        };

        Owner::init(&mut contract, &owner_id);
        contract.allowlist.insert(owner_id.clone().into());
        contract.deposit_unchecked(&owner_id, total_supply.into());

        contract
    }

    /// Returns boolean indicating whether the given account ID is on the allowlist.
    pub fn on_allowlist(&self, account_id: AccountId) -> bool {
        self.allowlist.contains(&account_id.clone().into())
    }

    /// adds credits to total_supply
    #[payable]
    pub fn add_deposit(&mut self, deposit: U128) {
        self.assert_owner();
        assert_one_yocto();
        self.deposit_unchecked(&self.own_get_owner().unwrap(), deposit.into());
    }

    /// registers an account on the allowlist 
    #[payable]
    pub fn register_holder(&mut self, account_id: AccountId) {
        self.assert_owner();
        assert_one_yocto();
        self.allowlist.insert(account_id.into());
    }

    /// removes an account from the allowlist
    pub fn remove_holder(&mut self, account_id: AccountId) {
        self.assert_owner();
        self.allowlist.remove(&account_id.into());
    }

    /// idk what this does
    #[payable]
    pub fn claim_credits(&mut self) {
        assert_one_yocto();
        let claimer = env::predecessor_account_id();

        self.transfer(
            claimer.clone(),
            self.own_get_owner().unwrap(),
            self.ft_balance_of(claimer).into(),
            Some("Claiming credits".to_string()),
        );
    }

    /// funds a single account on the allowlist with the default amount of credits
    #[payable]
    pub fn fund_program_participant(&mut self, account_id: AccountId) {
        self.assert_owner();
        assert_one_yocto();
        self.allowlist.insert(account_id.clone().into());
        self.transfer(
            self.own_get_owner().unwrap(),
            account_id,
            self.fund_amount,
            Some("Awarding credits to program participant".to_string()),
        );
    }

    /// funds multiple accounts on the allowlist with the default amount of credits
    #[payable]
    pub fn fund_program_participants(&mut self, account_ids: Vec<AccountId>) {
        self.assert_owner();
        assert_one_yocto();
        for account_id in account_ids {
            self.allowlist.insert(account_id.clone().into());
            self.transfer(
                self.own_get_owner().unwrap(),
                account_id,
                self.fund_amount,
                Some("Awarding credits to program participant".to_string()),
            );
        }
    }

    /// funds a single account on the allowlist with a specified amount of credits
    #[payable]
    pub fn fund_program_participant_with_amount(
        &mut self,
        account_id: AccountId,
        amount: U128,
    ) {
        self.assert_owner();
        assert_one_yocto();
        self.allowlist.insert(account_id.clone().into());
        self.transfer(
            self.own_get_owner().unwrap(),
            account_id,
            amount.into(),
            Some("Awarding credits to program participant".to_string()),
        );
    }

    /// funds multiple accounts on the allowlist with a specified amount of credits
    #[payable]
    pub fn fund_program_participants_with_amount(
        &mut self,
        account_ids: Vec<AccountId>,
        amount: U128,
    ) {
        self.assert_owner();
        assert_one_yocto();
        for account_id in account_ids {
            self.allowlist.insert(account_id.clone().into());
            self.transfer(
                self.own_get_owner().unwrap(),
                account_id,
                amount.into(),
                Some("Awarding credits to program participant".to_string()),
            );
        }
    }
}

impl Nep141Hook for Contract {
    /// checks that the sender and receiver are on the allowlist
    fn before_transfer(&mut self, transfer: &Nep141Transfer) {
        require!(
            self.allowlist.contains(&transfer.sender_id.clone().into()),
            "ERR_SENDER_NOT_REGISTERED"
        );
        require!(
            self.allowlist
                .contains(&transfer.receiver_id.clone().into()),
            "ERR_RECEIVER_NOT_REGISTERED"
        )
    }

    /// emits a Transfer event
    fn after_transfer(&mut self, _transfer: &Nep141Transfer, _state: ()) {}
}


#[cfg(test)]
mod tests {
    use near_sdk::{test_utils::VMContextBuilder, testing_env};
    use near_sdk_contract_tools::{owner::OwnerExternal, standard::nep141::Nep141};

    use super::*;

    #[test]
    fn test_init() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let total_supply = 1_000_000;
        let contract = Contract::new(bob.clone(), total_supply.into(), None);

        assert_eq!(contract.own_get_owner(), Some(bob));
        assert_eq!(contract.ft_total_supply(), total_supply.into());
    }

    #[test]
    fn test_on_allowlist() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), None);
        
        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();
        testing_env!(context);

        let transfer_amount = 1_000;

        contract.register_holder(alice.clone());

        assert_eq!(contract.on_allowlist(bob), true);
        assert_eq!(contract.on_allowlist(alice), true);
    }

    #[test]
    fn test_add_deposit() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), None);

        assert_eq!(contract.own_get_owner(), Some(bob.clone()));
        assert_eq!(contract.ft_total_supply(), total_supply.into());
        assert_eq!(contract.ft_balance_of(bob.clone()), total_supply.into());

        let additional_deposit = 10;
        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);

        contract.add_deposit(additional_deposit.into());

        assert_eq!(
            contract.ft_total_supply(),
            (total_supply + additional_deposit).into()
        );
        assert_eq!(
            contract.ft_balance_of(bob),
            (total_supply + additional_deposit).into()
        );
    }

    #[test]
    fn test_register_holder() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), None);

        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);

        let transfer_amount = 1_000;

        contract.register_holder(alice.clone());
        contract.transfer(bob.clone(), alice.clone(), transfer_amount, None);

        assert_eq!(contract.ft_balance_of(alice), transfer_amount.into());
        assert_eq!(
            contract.ft_balance_of(bob),
            (total_supply - transfer_amount).into()
        );
        assert_eq!(contract.ft_total_supply(), total_supply.into());
    }

    #[test]
    fn test_remove_holder() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), None);

        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);

        let transfer_amount = 1_000;

        contract.register_holder(alice.clone());
        contract.transfer(bob.clone(), alice.clone(), transfer_amount, None);

        assert_eq!(contract.ft_balance_of(alice.clone()), transfer_amount.into());
        assert_eq!(
            contract.ft_balance_of(bob),
            (total_supply - transfer_amount).into()
        );
        assert_eq!(contract.ft_total_supply(), total_supply.into());

        contract.remove_holder(alice.clone());
        assert_eq!(contract.on_allowlist(alice.clone()), false);
    }

    #[test]
    fn test_claim_credits() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), None);

        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);

        let transfer_amount = 1_000_u128;

        contract.register_holder(alice.clone());
        contract.transfer(bob.clone(), alice.clone(), transfer_amount, None);

        let context = VMContextBuilder::new()
            .predecessor_account_id(alice.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);

        contract.claim_credits();

        assert_eq!(contract.ft_balance_of(alice), 0.into());
        assert_eq!(contract.ft_balance_of(bob), total_supply.into());
        assert_eq!(contract.ft_total_supply(), total_supply.into());
    }

    #[test]
    fn test_fund_program_participant() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), Some(50_000.into()));

        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);
        contract.register_holder(alice.clone());

        contract.fund_program_participant(alice.clone());

        assert_eq!(contract.ft_balance_of(alice), 50_000.into());
        assert_eq!(contract.ft_balance_of(bob), (total_supply - 50_000).into());
        assert_eq!(contract.ft_total_supply(), total_supply.into());
    }

    #[test]
    fn test_burn_credits() {
        let bob: AccountId = "bob.near".parse().unwrap();
        let alice: AccountId = "alice.near".parse().unwrap();
        let total_supply = 1_000_000;
        let mut contract = Contract::new(bob.clone(), total_supply.into(), Some(50_000.into()));

        let context = VMContextBuilder::new()
            .predecessor_account_id(bob.clone())
            .attached_deposit(1)
            .build();

        testing_env!(context);
        contract.register_holder(alice.clone());

        contract.fund_program_participant(alice.clone());

        assert_eq!(contract.ft_balance_of(alice.clone()), 50_000.into());
        assert_eq!(contract.ft_balance_of(bob), (total_supply - 50_000).into());
        assert_eq!(contract.ft_total_supply(), total_supply.into());

        contract.burn(alice.clone(), 10_000, Some("expired credit duration".to_string()));

        assert_eq!(contract.ft_balance_of(alice.clone()), 40_000.into());
        assert_eq!(contract.ft_total_supply(),990_000.into());
    }
}
