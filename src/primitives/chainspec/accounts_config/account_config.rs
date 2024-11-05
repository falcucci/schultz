use casper_types::bytesrepr::FromBytes;
use casper_types::bytesrepr::ToBytes;
use casper_types::bytesrepr::{self};
use casper_types::Motes;
use casper_types::PublicKey;
use datasize::DataSize;
use num::Zero;
use serde::Deserialize;
use serde::Serialize;

use super::ValidatorConfig;

/// Configuration of an individial account in accounts.toml
#[derive(PartialEq, Ord, PartialOrd, Eq, Serialize, Deserialize, DataSize, Debug, Clone)]
pub struct AccountConfig {
    /// Public Key.
    pub public_key: PublicKey,
    /// Balance.
    pub balance: Motes,
    /// Validator config.
    pub validator: Option<ValidatorConfig>,
}

impl AccountConfig {
    /// Creates a new `AccountConfig`.
    pub fn new(public_key: PublicKey, balance: Motes, validator: Option<ValidatorConfig>) -> Self {
        Self {
            public_key,
            balance,
            validator,
        }
    }

    /// Public key.
    pub fn public_key(&self) -> PublicKey { self.public_key.clone() }

    /// Balance.
    pub fn balance(&self) -> Motes { self.balance }

    /// Bonded amount.
    pub fn bonded_amount(&self) -> Motes {
        match self.validator {
            Some(validator_config) => validator_config.bonded_amount(),
            None => Motes::zero(),
        }
    }

    /// Is this a genesis validator?
    pub fn is_genesis_validator(&self) -> bool { self.validator.is_some() }
}

impl ToBytes for AccountConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(self.public_key.to_bytes()?);
        buffer.extend(self.balance.to_bytes()?);
        buffer.extend(self.validator.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        self.public_key.serialized_length()
            + self.balance.serialized_length()
            + self.validator.serialized_length()
    }
}

impl FromBytes for AccountConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (public_key, remainder) = FromBytes::from_bytes(bytes)?;
        let (balance, remainder) = FromBytes::from_bytes(remainder)?;
        let (validator, remainder) = FromBytes::from_bytes(remainder)?;
        let account_config = AccountConfig {
            public_key,
            balance,
            validator,
        };
        Ok((account_config, remainder))
    }
}
