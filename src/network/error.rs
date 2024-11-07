use std::io;
use std::net::SocketAddr;

use openssl::error::ErrorStack;
use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error, Serialize)]
pub enum ManagerError {
    #[error("Failed to bind to address")]
    PeerNotFound,
    #[error("Error sending message to peer")]
    SendFailed(String),
    #[error("failed to get listener addr")]
    ListenerCreation(
        #[serde(skip_serializing)]
        #[source]
        io::Error,
        SocketAddr,
    ),
    #[error("Error serializing protocol handshake")]
    CouldNotEncodeOurHandshake(String),
    #[error("Error from the Communications module {0:?}")]
    #[serde(skip_serializing)]
    Tls(TLSError),
}

#[derive(Error, Debug)]
pub enum TLSError {
    #[error("Error setting up TCP connection {0:?}")]
    TcpConnection(io::Error),
    #[error("Could not set Nodelay for TCP")]
    TcpNoDelay,
    #[error("Error generating TLS certs {0:?}")]
    CouldNotGenerateTlsCertificate(ErrorStack),
    #[error("Error extracting keys from TLS handshake")]
    CouldNotExtractEcKey,
    #[error("Error initializing TLS Handshake {0:?}")]
    TlsInitialization(String),
    #[error("Error during TLS Handshake")]
    TlsHandshake(String),
    #[error("Could not find Peer's TLS certificate")]
    NoPeerCertificate,
    #[error("Signature Algorithm mimatch during TLS handshake")]
    WrongSignatureAlgorithm,
    #[error("EC Curve mimatch during TLS handshake")]
    WrongCurve,
    #[error("Could not verify certificate subject/issuer")]
    CorruptSubjectOrIssuer,
    #[error("TLS certificate was not self-signed")]
    NotSelfSigned,
    #[error("Serial number mismatch during TLS handshake")]
    WrongSerialNumber,
    #[error("Timing mismatch during TLS handshake")]
    TimeIssue,
    #[error("TLS certificate yet to be valid")]
    NotYetValid,
    #[error("Expired TLS certificate")]
    Expired,
    #[error("Error reading PublicKey from TLS connections")]
    CannotReadPublicKey,
    #[error("Error verifying PublicKey during TLS handshake")]
    KeyFailsCheck,
    #[error("Error validating Signature during TLS handshake")]
    FailedToValidateSignature,
    #[error("Error verifying Signature during TLS handshake")]
    InvalidSignature,
    #[error("Error verifying Serial number during TLS handshake")]
    InvalidSerialNumber,
}

impl From<TLSError> for ManagerError {
    fn from(value: TLSError) -> Self { ManagerError::Tls(value) }
}
