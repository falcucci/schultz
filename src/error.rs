use thiserror::Error;

use crate::network::error::NetworkManagerError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error from the Communications module {0:?}")]
    NetworkManager(NetworkManagerError),
}

impl From<NetworkManagerError> for Error {
    fn from(err: NetworkManagerError) -> Self { Error::NetworkManager(err) }
}
