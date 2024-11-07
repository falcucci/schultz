use casper_types::bytesrepr::FromBytes;
use casper_types::bytesrepr::ToBytes;
use casper_types::bytesrepr::{self};
use casper_types::Motes;
use casper_types::TimeDiff;
use casper_types::U512;
use datasize::DataSize;
use serde::Deserialize;
use serde::Serialize;

/// The maximum amount of motes that payment code execution can cost.
pub const MAX_PAYMENT_AMOUNT: u64 = 2_500_000_000;

/// Configuration values associated with deploys.
#[derive(Copy, Clone, DataSize, PartialEq, Eq, Serialize, Deserialize, Debug)]
// Disallow unknown fields to ensure config files and command-line overrides contain valid keys.
#[serde(deny_unknown_fields)]
pub struct DeployConfig {
    pub(crate) max_payment_cost: Motes,
    pub(crate) max_ttl: TimeDiff,
    pub(crate) max_dependencies: u8,
    pub(crate) max_block_size: u32,
    pub(crate) max_deploy_size: u32,
    pub(crate) block_max_deploy_count: u32,
    pub(crate) block_max_transfer_count: u32,
    pub(crate) block_max_approval_count: u32,
    pub(crate) block_gas_limit: u64,
    pub(crate) payment_args_max_length: u32,
    pub(crate) session_args_max_length: u32,
    pub(crate) native_transfer_minimum_motes: u64,
    pub(crate) max_timestamp_leeway: TimeDiff,
}

impl DeployConfig {
    /// Validates `DeployConfig` parameters
    pub fn is_valid(&self) -> bool {
        // the total number of deploys + transfers should not exceed the number of
        // approvals because each deploy or transfer needs at least one approval
        // to be valid
        if let Some(total_deploy_and_transfer_slots) =
            self.block_max_deploy_count.checked_add(self.block_max_transfer_count)
        {
            self.block_max_approval_count >= total_deploy_and_transfer_slots
        } else {
            false
        }
    }
}

impl ToBytes for DeployConfig {
    fn to_bytes(&self) -> Result<Vec<u8>, bytesrepr::Error> {
        let mut buffer = bytesrepr::allocate_buffer(self)?;
        buffer.extend(self.max_payment_cost.value().to_bytes()?);
        buffer.extend(self.max_ttl.to_bytes()?);
        buffer.extend(self.max_dependencies.to_bytes()?);
        buffer.extend(self.max_block_size.to_bytes()?);
        buffer.extend(self.max_deploy_size.to_bytes()?);
        buffer.extend(self.block_max_deploy_count.to_bytes()?);
        buffer.extend(self.block_max_transfer_count.to_bytes()?);
        buffer.extend(self.block_max_approval_count.to_bytes()?);
        buffer.extend(self.block_gas_limit.to_bytes()?);
        buffer.extend(self.payment_args_max_length.to_bytes()?);
        buffer.extend(self.session_args_max_length.to_bytes()?);
        buffer.extend(self.native_transfer_minimum_motes.to_bytes()?);
        buffer.extend(self.max_timestamp_leeway.to_bytes()?);
        Ok(buffer)
    }

    fn serialized_length(&self) -> usize {
        self.max_payment_cost.value().serialized_length()
            + self.max_ttl.serialized_length()
            + self.max_dependencies.serialized_length()
            + self.max_block_size.serialized_length()
            + self.max_deploy_size.serialized_length()
            + self.block_max_deploy_count.serialized_length()
            + self.block_max_transfer_count.serialized_length()
            + self.block_max_approval_count.serialized_length()
            + self.block_gas_limit.serialized_length()
            + self.payment_args_max_length.serialized_length()
            + self.session_args_max_length.serialized_length()
            + self.native_transfer_minimum_motes.serialized_length()
            + self.max_timestamp_leeway.serialized_length()
    }
}

impl FromBytes for DeployConfig {
    fn from_bytes(bytes: &[u8]) -> Result<(Self, &[u8]), bytesrepr::Error> {
        let (max_payment_cost, remainder) = U512::from_bytes(bytes)?;
        let max_payment_cost = Motes::new(max_payment_cost);
        let (max_ttl, remainder) = TimeDiff::from_bytes(remainder)?;
        let (max_dependencies, remainder) = u8::from_bytes(remainder)?;
        let (max_block_size, remainder) = u32::from_bytes(remainder)?;
        let (max_deploy_size, remainder) = u32::from_bytes(remainder)?;
        let (block_max_deploy_count, remainder) = u32::from_bytes(remainder)?;
        let (block_max_transfer_count, remainder) = u32::from_bytes(remainder)?;
        let (block_max_approval_count, remainder) = u32::from_bytes(remainder)?;
        let (block_gas_limit, remainder) = u64::from_bytes(remainder)?;
        let (payment_args_max_length, remainder) = u32::from_bytes(remainder)?;
        let (session_args_max_length, remainder) = u32::from_bytes(remainder)?;
        let (native_transfer_minimum_motes, remainder) = u64::from_bytes(remainder)?;
        let (max_timestamp_leeway, remainder) = TimeDiff::from_bytes(remainder)?;
        let config = DeployConfig {
            max_payment_cost,
            max_ttl,
            max_dependencies,
            max_block_size,
            max_deploy_size,
            block_max_deploy_count,
            block_max_transfer_count,
            block_max_approval_count,
            block_gas_limit,
            payment_args_max_length,
            session_args_max_length,
            native_transfer_minimum_motes,
            max_timestamp_leeway,
        };
        Ok((config, remainder))
    }
}
