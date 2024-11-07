//! The accounts config is a set of configuration options that is used to create
//! accounts at genesis, and set up auction contract with validators and
//! delegators.
mod account_config;
mod delegator_config;
mod validator_config;

use std::path::Path;

pub use account_config::AccountConfig;
use casper_types::bytesrepr::Bytes;
use casper_types::bytesrepr::FromBytes;
use casper_types::bytesrepr::ToBytes;
use casper_types::bytesrepr::{self};
use casper_types::file_utils;
use casper_types::Motes;
use casper_types::PublicKey;
use datasize::DataSize;
pub use delegator_config::DelegatorConfig;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
pub use validator_config::ValidatorConfig;

use super::error::ChainspecAccountsLoadError;

const CHAINSPEC_ACCOUNTS_FILENAME: &str = "accounts.toml";

fn sorted_vec_deserializer<'de, T, D>(deserializer: D) -> Result<Vec<T>, D::Error>
where
    T: Deserialize<'de> + Ord,
    D: Deserializer<'de>,
{
    let mut vec = Vec::<T>::deserialize(deserializer)?;
    vec.sort_unstable();
    Ok(vec)
}

/// Special account in the system that is useful only for some private chains.
#[derive(DataSize, Debug, Clone, PartialEq, Eq, Ord, PartialOrd, Serialize, Deserialize)]
pub struct AdministratorAccount {
    public_key: PublicKey,
    balance: Motes,
}

impl AdministratorAccount {
    /// Creates new special account.
    pub fn new(public_key: PublicKey, balance: Motes) -> Self {
        Self {
            public_key,
            balance,
        }
    }

    /// Gets a reference to the administrator account's public key.
    pub fn public_key(&self) -> &PublicKey { &self.public_key }
}

impl ToBytes for AdministratorAccount {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let AdministratorAccount {
            public_key,
            balance,
        } = self;
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(public_key.to_bytes()?);
        buffer.extend(balance.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        let AdministratorAccount {
            public_key,
            balance,
        } = self;
        public_key.serialized_length() + balance.serialized_length()
    }
}

impl FromBytes for AdministratorAccount {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (public_key, remainder) = FromBytes::from_bytes(bytes)?;
        let (balance, remainder) = FromBytes::from_bytes(remainder)?;
        let administrator_account = AdministratorAccount {
            public_key,
            balance,
        };
        Ok((administrator_account, remainder))
    }
}

/// Configuration values associated with accounts.toml
#[derive(PartialEq, Eq, Serialize, Deserialize, DataSize, Debug, Clone)]
pub struct AccountsConfig {
    #[serde(deserialize_with = "sorted_vec_deserializer")]
    accounts: Vec<AccountConfig>,
    #[serde(default, deserialize_with = "sorted_vec_deserializer")]
    delegators: Vec<DelegatorConfig>,
    #[serde(
        default,
        deserialize_with = "sorted_vec_deserializer",
        skip_serializing_if = "Vec::is_empty"
    )]
    administrators: Vec<AdministratorAccount>,
}

impl AccountsConfig {
    /// Create new accounts config instance.
    pub fn new(
        accounts: Vec<AccountConfig>,
        delegators: Vec<DelegatorConfig>,
        administrators: Vec<AdministratorAccount>,
    ) -> Self {
        Self {
            accounts,
            delegators,
            administrators,
        }
    }

    /// Accounts.
    pub fn accounts(&self) -> &[AccountConfig] { &self.accounts }

    /// Delegators.
    pub fn delegators(&self) -> &[DelegatorConfig] { &self.delegators }

    /// Administrators.
    pub fn administrators(&self) -> &[AdministratorAccount] { &self.administrators }

    /// Account.
    pub fn account(&self, public_key: &PublicKey) -> Option<&AccountConfig> {
        self.accounts.iter().find(|account| &account.public_key == public_key)
    }

    pub fn is_genesis_validator(&self, public_key: &PublicKey) -> bool {
        match self.account(public_key) {
            None => false,
            Some(account_config) => account_config.is_genesis_validator(),
        }
    }

    /// Returns `Self` and the raw bytes of the file.
    ///
    /// If the file doesn't exist, returns `Ok` with an empty `AccountsConfig`
    /// and `None` bytes.
    pub(super) fn from_dir<P: AsRef<Path>>(
        dir_path: P,
    ) -> Result<(Self, Option<Bytes>), ChainspecAccountsLoadError> {
        let accounts_path = dir_path.as_ref().join(CHAINSPEC_ACCOUNTS_FILENAME);
        if !accounts_path.is_file() {
            let config = AccountsConfig::new(vec![], vec![], vec![]);
            let maybe_bytes = None;
            return Ok((config, maybe_bytes));
        }
        let bytes = file_utils::read_file(accounts_path)?;
        let config: AccountsConfig = toml::from_slice(&bytes)?;
        Ok((config, Some(Bytes::from(bytes))))
    }
}

impl ToBytes for AccountsConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(self.accounts.to_bytes()?);
        buffer.extend(self.delegators.to_bytes()?);
        buffer.extend(self.administrators.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        self.accounts.serialized_length()
            + self.delegators.serialized_length()
            + self.administrators.serialized_length()
    }
}

impl FromBytes for AccountsConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (accounts, remainder) = FromBytes::from_bytes(bytes)?;
        let (delegators, remainder) = FromBytes::from_bytes(remainder)?;
        let (administrators, remainder) = FromBytes::from_bytes(remainder)?;
        let accounts_config = AccountsConfig::new(accounts, delegators, administrators);
        Ok((accounts_config, remainder))
    }
}
