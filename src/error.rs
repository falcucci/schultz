use thiserror::Error;

use crate::network::error::ManagerError;

pub type Result<T> = std::result::Result<T, Error>;

#[derive(Error, Debug)]
pub enum Error {
    #[error("Error from the Communications module {0:?}")]
    NetworkManager(ManagerError),
}

impl From<ManagerError> for Error {
    fn from(err: ManagerError) -> Self { Error::NetworkManager(err) }
}
