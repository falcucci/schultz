// TODO - remove once schemars stops causing warning.
#![allow(clippy::field_reassign_with_default)]

use std::str::FromStr;

use casper_types::bytesrepr::FromBytes;
use casper_types::bytesrepr::ToBytes;
use casper_types::bytesrepr::{self};
use casper_types::ProtocolVersion;
use datasize::DataSize;
use serde::Deserialize;
use serde::Serialize;

use super::activation_point::ActivationPoint;
use super::global_state_update::GlobalStateUpdate;

/// Configuration values associated with the protocol.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize, DataSize, Debug)]
pub struct ProtocolConfig {
    /// Protocol version.
    #[data_size(skip)]
    pub version: ProtocolVersion,
    /// Whether we need to clear latest blocks back to the switch block just
    /// before the activation point or not.
    pub hard_reset: bool,
    /// This protocol config applies starting at the era specified in the
    /// activation point.
    pub activation_point: ActivationPoint,
    /// Any arbitrary updates we might want to make to the global state at the
    /// start of the era specified in the activation point.
    pub global_state_update: Option<GlobalStateUpdate>,
}

impl ToBytes for ProtocolConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(self.version.to_string().to_bytes()?);
        buffer.extend(self.hard_reset.to_bytes()?);
        buffer.extend(self.activation_point.to_bytes()?);
        buffer.extend(self.global_state_update.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        self.version.to_string().serialized_length()
            + self.hard_reset.serialized_length()
            + self.activation_point.serialized_length()
            + self.global_state_update.serialized_length()
    }
}

impl FromBytes for ProtocolConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (protocol_version_string, remainder) = String::from_bytes(bytes)?;
        let version = ProtocolVersion::from_str(&protocol_version_string)
            .map_err(|_| bytesrepr::Error::Formatting)?;
        let (hard_reset, remainder) = bool::from_bytes(remainder)?;
        let (activation_point, remainder) = ActivationPoint::from_bytes(remainder)?;
        let (global_state_update, remainder) = Option::<GlobalStateUpdate>::from_bytes(remainder)?;
        let protocol_config = ProtocolConfig {
            version,
            hard_reset,
            activation_point,
            global_state_update,
        };
        Ok((protocol_config, remainder))
    }
}

#[cfg(test)]
mod tests {
    use casper_types::EraId;

    use super::*;
    use crate::types::Block;

    #[test]
    fn activation_point_bytesrepr_roundtrip() {
        let mut rng = crate::new_rng();
        let activation_point = ActivationPoint::random(&mut rng);
        bytesrepr::test_serialization_roundtrip(&activation_point);
    }

    #[test]
    fn protocol_config_bytesrepr_roundtrip() {
        let mut rng = crate::new_rng();
        let config = ProtocolConfig::random(&mut rng);
        bytesrepr::test_serialization_roundtrip(&config);
    }

    #[test]
    fn toml_roundtrip() {
        let mut rng = crate::new_rng();
        let config = ProtocolConfig::random(&mut rng);
        let encoded = toml::to_string_pretty(&config).unwrap();
        let decoded = toml::from_str(&encoded).unwrap();
        assert_eq!(config, decoded);
    }

    #[test]
    fn should_perform_checks_without_global_state_update() {
        let mut rng = crate::new_rng();
        let mut protocol_config = ProtocolConfig::random(&mut rng);

        // We force `global_state_update` to be `None`.
        protocol_config.global_state_update = None;

        assert!(protocol_config.is_valid());
    }

    #[test]
    fn should_perform_checks_with_global_state_update() {
        let mut rng = crate::new_rng();
        let mut protocol_config = ProtocolConfig::random(&mut rng);

        // We force `global_state_update` to be `Some`.
        protocol_config.global_state_update = Some(GlobalStateUpdate::random(&mut rng));

        assert!(protocol_config.is_valid());
    }

    #[test]
    fn should_recognize_blocks_before_activation_point() {
        let past_version = ProtocolVersion::from_parts(1, 0, 0);
        let current_version = ProtocolVersion::from_parts(2, 0, 0);
        let future_version = ProtocolVersion::from_parts(3, 0, 0);

        let upgrade_era = EraId::from(5);
        let previous_era = upgrade_era.saturating_sub(1);

        let mut rng = crate::new_rng();
        let protocol_config = ProtocolConfig {
            version: current_version,
            hard_reset: false,
            activation_point: ActivationPoint::EraId(upgrade_era),
            global_state_update: None,
        };

        // The block before this protocol version: a switch block with previous era and
        // version.
        let block =
            Block::random_with_specifics(&mut rng, previous_era, 100, past_version, true, None);
        assert!(protocol_config.is_last_block_before_activation(block.header()));

        // Not the activation point: wrong era.
        let block =
            Block::random_with_specifics(&mut rng, upgrade_era, 100, past_version, true, None);
        assert!(!protocol_config.is_last_block_before_activation(block.header()));

        // Not the activation point: wrong version.
        let block =
            Block::random_with_specifics(&mut rng, previous_era, 100, current_version, true, None);
        assert!(!protocol_config.is_last_block_before_activation(block.header()));
        let block =
            Block::random_with_specifics(&mut rng, previous_era, 100, future_version, true, None);
        assert!(!protocol_config.is_last_block_before_activation(block.header()));

        // Not the activation point: not a switch block.
        let block =
            Block::random_with_specifics(&mut rng, previous_era, 100, past_version, false, None);
        assert!(!protocol_config.is_last_block_before_activation(block.header()));
    }
}
