use casper_types::bytesrepr::FromBytes;
use casper_types::bytesrepr::ToBytes;
use casper_types::bytesrepr::{self};
use datasize::DataSize;
use serde::Serialize;

use super::accounts_config::AccountsConfig;

/// Configuration values associated with the network.
#[derive(Clone, DataSize, PartialEq, Eq, Serialize, Debug)]
pub struct NetworkConfig {
    /// The network name.
    pub name: String,
    /// The maximum size of an accepted network message, in bytes.
    pub maximum_net_message_size: u32,
    /// Validator accounts specified in the chainspec.
    // Note: `accounts_config` must be the last field on this struct due to issues in the TOML
    // crate - see <https://github.com/alexcrichton/toml-rs/search?q=ValueAfterTable&type=issues>.
    pub accounts_config: AccountsConfig,
}

impl ToBytes for NetworkConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(self.name.to_bytes()?);
        buffer.extend(self.accounts_config.to_bytes()?);
        buffer.extend(self.maximum_net_message_size.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        self.name.serialized_length()
            + self.accounts_config.serialized_length()
            + self.maximum_net_message_size.serialized_length()
    }
}

impl FromBytes for NetworkConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (name, remainder) = String::from_bytes(bytes)?;
        let (accounts_config, remainder) = FromBytes::from_bytes(remainder)?;
        let (maximum_net_message_size, remainder) = FromBytes::from_bytes(remainder)?;
        let config = NetworkConfig {
            name,
            maximum_net_message_size,
            accounts_config,
        };
        Ok((config, remainder))
    }
}
