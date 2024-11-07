use std::fmt;
use std::fmt::Debug;
use std::fmt::Display;
use std::fmt::Formatter;
use std::io;
use std::io::Cursor;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;

use bincode::config::RejectTrailing;
use bincode::config::VarintEncoding;
use bincode::config::WithOtherEndian;
use bincode::config::WithOtherIntEncoding;
use bincode::config::WithOtherLimit;
use bincode::config::WithOtherTrailing;
use bincode::Options;
use bytes::Bytes;
use bytes::BytesMut;
use casper_hashing::Digest;
use casper_types::AsymmetricType;
use casper_types::ProtocolVersion;
use casper_types::PublicKey;
use casper_types::Signature;
use futures::SinkExt;
use futures::StreamExt;
use serde::de::Error;
use serde::Deserialize;
use serde::Deserializer;
use serde::Serialize;
use serde::Serializer;
use strum::EnumDiscriminants;
use tokio::net::TcpStream;
use tokio_openssl::SslStream;
use tokio_serde::Deserializer as TokioDeserializer;
use tokio_serde::Serializer as TokioSerializer;
use tokio_util::codec::LengthDelimitedCodec;

use super::error::ManagerError;
use crate::primitives::Nonce;
use crate::utils::OptDisplay;

/// Transport type alias for base encrypted connections.
type Transport = SslStream<TcpStream>;
pub type FramedTransport = tokio_util::codec::Framed<Transport, LengthDelimitedCodec>;

/// A thin wrapper over bytes to impl Payload Trait
pub struct SchultzMessage {
    payload: Bytes,
}

impl SchultzMessage {
    pub fn new(payload: Bytes) -> Result<Self, ManagerError> { Ok(SchultzMessage { payload }) }

    // Helper to write SchultzMessage bytes to the provided stream.
    pub async fn write_to_stream(&self, stream: &mut FramedTransport) -> Result<(), ManagerError> {
        let SchultzMessage { payload, .. } = self;

        let (mut writer, mut _reader) = stream.split();

        // Send bytes of TcpStream
        writer
            .send(payload.clone())
            .await
            .map_err(|e| ManagerError::SendFailed(e.to_string()))?;

        Ok(())
    }
}

/// Certificate used to indicate that the peer is a validator using the
/// specified public key.
///
/// Note that this type has custom `Serialize` and `Deserialize` implementations
/// to allow the `public_key` and `signature` fields to be encoded to
/// all-lowercase hex, hence circumventing the checksummed-hex encoding used by
/// `PublicKey` and `Signature` in versions 1.4.2 and 1.4.3.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ConsensusCertificate {
    public_key: PublicKey,
    signature: Signature,
}

impl Display for ConsensusCertificate {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result { write!(f, "key:{}", self.public_key) }
}

/// msgpack encoder/decoder for messages.
#[derive(Debug)]
pub struct MessagePackFormat;

impl<M> TokioSerializer<M> for MessagePackFormat
where
    M: Serialize,
{
    // Note: We cast to `io::Error` because of the `Codec::Error:
    // Into<Transport::Error>` requirement.
    type Error = io::Error;

    #[inline]
    fn serialize(self: Pin<&mut Self>, item: &M) -> Result<Bytes, Self::Error> {
        rmp_serde::to_vec(item)
            .map(Into::into)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

impl<M> TokioDeserializer<M> for MessagePackFormat
where
    for<'de> M: Deserialize<'de>,
{
    type Error = io::Error;

    #[inline]
    fn deserialize(self: Pin<&mut Self>, src: &BytesMut) -> Result<M, Self::Error> {
        rmp_serde::from_read(Cursor::new(src))
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

#[derive(Clone, Debug, Deserialize, Serialize, EnumDiscriminants)]
#[strum_discriminants(derive(strum::EnumIter))]
#[allow(clippy::large_enum_variant)]
pub enum Message<P> {
    Handshake {
        /// Network we are connected to.
        network_name: String,
        /// The public address of the node connecting.
        public_addr: SocketAddr,
        /// Protocol version the node is speaking.
        #[serde(default = "default_protocol_version")]
        protocol_version: ProtocolVersion,
        /// A self-signed certificate indicating validator status.
        #[serde(default)]
        consensus_certificate: Option<ConsensusCertificate>,
        /// True if the node is syncing.
        #[serde(default)]
        is_syncing: bool,
        /// Hash of the chainspec the node is running.
        #[serde(default)]
        chainspec_hash: Option<Digest>,
    },
    /// A ping request.
    Ping {
        /// The nonce to be returned with the pong.
        nonce: Nonce,
    },
    /// A pong response.
    Pong {
        /// Nonce to match pong to ping.
        nonce: Nonce,
    },
    Payload(P),
}

/// The default protocol version to use in absence of one in the protocol
/// version field.
#[inline]
fn default_protocol_version() -> ProtocolVersion { ProtocolVersion::V1_0_0 }

/// This type and the `NonHumanReadableCertificate` are helper structs only used
/// in the `Serialize` and `Deserialize` implementations of
/// `ConsensusCertificate` to allow handshaking between nodes running the
/// casper-node v1.4.2 and v1.4.3 software versions.
///
/// Checksummed-hex encoding was introduced in 1.4.2 and was applied to
/// `PublicKey` and `Signature` types, affecting the encoding of
/// `ConsensusCertificate` since handshaking uses a human-readable
/// type of encoder/decoder.
///
/// The 1.4.3 version immediately after 1.4.2 used a slightly different style of
/// checksummed-hex encoding which is incompatible with the 1.4.2 style.  To
/// effectively disable checksummed-hex encoding, we need to use an
/// all-lowercase form of hex encoding for the `PublicKey` and `Signature`
/// types.
///
/// The `HumanReadableCertificate` enables that by explicitly being constructed
/// from all-lowercase hex encoded types, while the
/// `NonHumanReadableCertificate` is a simple mirror of `ConsensusCertificate`
/// to allow us to derive `Serialize` and `Deserialize`, avoiding complex
/// hand-written implementations for the non-human-readable case.
#[derive(Serialize, Deserialize)]
struct HumanReadableCertificate {
    public_key: String,
    signature: String,
}

#[derive(Serialize, Deserialize)]
struct NonHumanReadableCertificate {
    public_key: PublicKey,
    signature: Signature,
}

impl Serialize for ConsensusCertificate {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        if serializer.is_human_readable() {
            let human_readable_certificate = HumanReadableCertificate {
                public_key: self.public_key.to_hex().to_lowercase(),
                signature: self.signature.to_hex().to_lowercase(),
            };

            return human_readable_certificate.serialize(serializer);
        }

        let non_human_readable_certificate = NonHumanReadableCertificate {
            public_key: self.public_key.clone(),
            signature: self.signature,
        };
        non_human_readable_certificate.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for ConsensusCertificate {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        if deserializer.is_human_readable() {
            let human_readable_certificate = HumanReadableCertificate::deserialize(deserializer)?;
            let public_key = PublicKey::from_hex(
                human_readable_certificate.public_key.to_lowercase().as_bytes(),
            )
            .map_err(D::Error::custom)?;
            let signature =
                Signature::from_hex(human_readable_certificate.signature.to_lowercase().as_bytes())
                    .map_err(D::Error::custom)?;
            return Ok(ConsensusCertificate {
                public_key,
                signature,
            });
        }

        let non_human_readable_certificate =
            NonHumanReadableCertificate::deserialize(deserializer)?;
        Ok(ConsensusCertificate {
            public_key: non_human_readable_certificate.public_key,
            signature: non_human_readable_certificate.signature,
        })
    }
}

impl<P: Display> Display for Message<P> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        match self {
            Message::Handshake {
                network_name,
                public_addr,
                protocol_version,
                consensus_certificate,
                is_syncing,
                chainspec_hash,
            } => {
                write!(
                    f,
                    "handshake: {}, public addr: {}, protocol_version: {}, consensus_certificate: \
                     {}, is_syncing: {}, chainspec_hash: {}",
                    network_name,
                    public_addr,
                    protocol_version,
                    OptDisplay::new(consensus_certificate.as_ref(), "none"),
                    is_syncing,
                    OptDisplay::new(chainspec_hash.as_ref(), "none")
                )
            }
            Message::Ping { nonce } => write!(f, "ping({})", nonce),
            Message::Pong { nonce } => write!(f, "pong({})", nonce),
            Message::Payload(payload) => write!(f, "payload: {}", payload),
        }
    }
}

/// bincode encoder/decoder for messages.
#[allow(clippy::type_complexity)]
pub struct BincodeFormat(
    // Note: `bincode` encodes its options at the type level. The exact shape is determined by
    // `BincodeFormat::default()`.
    pub  WithOtherTrailing<
        WithOtherIntEncoding<
            WithOtherEndian<
                WithOtherLimit<bincode::DefaultOptions, bincode::config::Infinite>,
                bincode::config::LittleEndian,
            >,
            VarintEncoding,
        >,
        RejectTrailing,
    >,
);

impl BincodeFormat {
    /// Serializes an arbitrary serializable value with the networking bincode
    /// serializer.
    #[inline]
    pub fn serialize_arbitrary<T>(&self, item: &T) -> io::Result<Vec<u8>>
    where
        T: Serialize,
    {
        self.0
            .serialize(item)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}

impl Debug for BincodeFormat {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("BincodeFormat")
    }
}

impl Default for BincodeFormat {
    fn default() -> Self {
        let opts = bincode::options()
            .with_no_limit() // We rely on framed tokio transports to impose limits.
            .with_little_endian() // Default at the time of this writing, we are merely pinning it.
            .with_varint_encoding() // Same as above.
            .reject_trailing_bytes(); // There is no reason for us not to reject trailing bytes.
        BincodeFormat(opts)
    }
}

impl<P> TokioSerializer<Arc<Message<P>>> for BincodeFormat
where
    Message<P>: Serialize,
{
    type Error = io::Error;

    #[inline]
    fn serialize(self: Pin<&mut Self>, item: &Arc<Message<P>>) -> Result<Bytes, Self::Error> {
        let msg = &**item;
        self.serialize_arbitrary(msg).map(Into::into)
    }
}

impl<P> TokioDeserializer<Message<P>> for BincodeFormat
where
    for<'de> Message<P>: Deserialize<'de>,
{
    type Error = io::Error;

    #[inline]
    fn deserialize(self: Pin<&mut Self>, src: &BytesMut) -> Result<Message<P>, Self::Error> {
        self.0
            .deserialize(src)
            .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
    }
}
