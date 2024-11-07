use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use casper_hashing::Digest;
use casper_types::ProtocolVersion;
use futures::SinkExt;
use futures::StreamExt;
use openssl::pkey::PKeyRef;
use openssl::pkey::Private;
use openssl::ssl::Ssl;
use openssl::ssl::SslAcceptor;
use openssl::ssl::SslConnector;
use openssl::ssl::SslMethod;
use openssl::x509::X509Ref;
use rand::RngCore;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio::time::interval;
use tokio_openssl::SslStream;
use tokio_serde::Deserializer;
use tokio_serde::Serializer;
use tokio_util::codec::LengthDelimitedCodec;
use tracing::error;
use tracing::info;
use tracing::trace;
use tracing::warn;

use super::error::ManagerError;
use super::error::TLSError;
use super::message::FramedTransport;
use super::message::Message;
use super::message::MessagePackFormat;
use super::message::SchultzMessage;
use super::tls;
use super::tls::set_context_options;
use super::tls::Identity;
use super::tls::SslResult;
use crate::network::message::BincodeFormat;
use crate::network::tls::validate_self_signed_cert;
use crate::primitives::Chainspec;
use crate::primitives::Nonce;
use crate::primitives::Payload;

/// Maximum frame length to be decoded from incoming stream
pub const MAX_FRAME_LEN: usize = 25165824; // 25 MB as Bytes

/// Connection Pool polling rate
pub const POLLING_RATE: u64 = 1; // 1 ms

pub struct Manager {
    schultz_addr: SocketAddr,
    tcp_ep: Arc<Mutex<TcpListener>>,
    identity: Identity,
    pub chainspec: Chainspec,
    connection_pool: Arc<Mutex<BTreeMap<SocketAddr, FramedTransport>>>,
    awaiting_hs_reply_from: Arc<Mutex<Vec<SocketAddr>>>,
    fully_connected_peers: Arc<Mutex<Vec<SocketAddr>>>,
    endpoint_listener_handle: Option<JoinHandle<()>>,
    conn_pool_listener_handle: Option<JoinHandle<()>>,
}

impl Manager {
    pub async fn new<P: Payload>(
        schultz_addr: SocketAddr,
        event_tx: Sender<(SocketAddr, Message<P>)>,
        chainspec: Chainspec,
    ) -> Result<Self, ManagerError> {
        info!("Starting network communications...");
        let listener = TcpListener::bind(schultz_addr)
            .await
            .map_err(|error| ManagerError::ListenerCreation(error, schultz_addr))?;

        let identity = Identity::with_generated_certs().expect("Failed to generate identity");

        let mut schultz = Self {
            schultz_addr,
            tcp_ep: Arc::new(Mutex::new(listener)),
            identity,
            chainspec,
            connection_pool: Arc::new(Mutex::new(BTreeMap::new())),
            awaiting_hs_reply_from: Arc::new(Mutex::new(Vec::new())),
            fully_connected_peers: Arc::new(Mutex::new(Vec::new())),
            endpoint_listener_handle: None,
            conn_pool_listener_handle: None,
        };

        let endpoint_listener_handle = schultz.listen_on_endpoint().await;
        let conn_pool_listener_handle = schultz.listen_to_connection_pool(event_tx).await;

        schultz.endpoint_listener_handle = Some(endpoint_listener_handle);
        schultz.conn_pool_listener_handle = Some(conn_pool_listener_handle);

        info!("Network communications started!");
        trace!("Waiting for incoming connections...");

        Ok(schultz)
    }

    pub fn schultz_addr(&self) -> SocketAddr { self.schultz_addr }

    pub async fn connect(&self, addr: &SocketAddr) -> Result<(), ManagerError> {
        info!("Connecting to {addr:?}");
        let stream = TcpStream::connect(addr).await.map_err(TLSError::TcpConnection)?;

        stream.set_nodelay(true).map_err(|_| TLSError::TcpNoDelay)?;

        let mut transport =
            tls::create_tls_connector(&self.identity.tls_certificate, &self.identity.secret_key)
                .and_then(|connector| connector.configure())
                .and_then(|mut config| {
                    config.set_verify_hostname(false);
                    config.into_ssl("this-will-not-be-checked.example.com")
                })
                .and_then(|ssl| SslStream::new(ssl, stream))
                .map_err(|error| TLSError::TlsInitialization(error.to_string()))?;

        SslStream::connect(Pin::new(&mut transport))
            .await
            .map_err(|error| TLSError::TlsHandshake(error.to_string()))?;

        let peer_cert = transport.ssl().peer_certificate().ok_or(TLSError::NoPeerCertificate)?;

        tls::validate_peer_cert(peer_cert).map_err(|_| TLSError::FailedToValidateSignature)?;

        let framed_transport = tokio_util::codec::Framed::new(
            transport,
            LengthDelimitedCodec::builder().max_frame_length(MAX_FRAME_LEN).new_codec(),
        );

        self.connection_pool.lock().await.insert(*addr, framed_transport);

        Ok(())
    }

    pub async fn handshake<P: Payload>(&self, addr: SocketAddr) -> Result<(), ManagerError> {
        let mut encoder = MessagePackFormat;
        let hs: Message<P> = Message::Handshake {
            network_name: self.chainspec.network_config.name.clone(),
            public_addr: self.schultz_addr,
            protocol_version: self.chainspec.protocol_config.version,
            consensus_certificate: None,
            is_syncing: false,
            chainspec_hash: Some(self.chainspec.hash()),
        };

        let serialized_handshake_message = Pin::new(&mut encoder)
            .serialize(&Arc::new(hs))
            .map_err(|error| TLSError::TlsHandshake(error.to_string()))?;

        trace!("1.Trying to send a Handshake to {addr:?}");

        self.send_message(addr, serialized_handshake_message).await?;

        info!("Sent a handshake to {addr:?}");

        self.awaiting_hs_reply_from.lock().await.push(addr);

        Ok(())
    }

    pub async fn send_message(&self, addr: SocketAddr, payload: Bytes) -> Result<(), ManagerError> {
        info!("Sending message to {addr:?}");
        let mut conn_pool = self.connection_pool.lock().await;
        let peer_connection = conn_pool.get_mut(&addr).ok_or(ManagerError::PeerNotFound)?;
        let message = SchultzMessage::new(payload)?;
        message.write_to_stream(peer_connection).await?;

        Ok(())
    }

    pub async fn send_ping<P: Payload>(&self, addr: SocketAddr) -> Result<(), ManagerError> {
        info!("Sending a ping to {addr:?}");
        let ping: Message<P> = Message::Ping {
            nonce: Nonce::new(rand::thread_rng().next_u64()),
        };

        let mut encoder = BincodeFormat::default();

        let serialized_ping_message = Pin::new(&mut encoder)
            .serialize(&Arc::new(ping))
            .map_err(|e| ManagerError::CouldNotEncodeOurHandshake(e.to_string()))?;

        self.send_message(addr, serialized_ping_message).await?;

        info!("Sent a ping to {addr:?}");

        Ok(())
    }

    /// Creates a TLS acceptor for a server.
    ///
    /// The acceptor will restrict TLS parameters to secure one defined in this
    /// crate that are compatible with connectors built with
    /// `create_tls_connector`.
    ///
    /// Incoming certificates must still be validated using `validate_cert`.
    pub fn create_tls_acceptor(
        cert: &X509Ref,
        private_key: &PKeyRef<Private>,
    ) -> SslResult<SslAcceptor> {
        info!("Creating TLS acceptor for incoming connections");
        let mut builder = SslAcceptor::mozilla_modern_v5(SslMethod::tls_server())?;
        set_context_options(&mut builder, cert, private_key)?;

        Ok(builder.build())
    }

    pub async fn setup_tls(
        stream: TcpStream,
        identity: &Identity,
    ) -> Result<SslStream<TcpStream>, ManagerError> {
        info!("Setting up TLS with connected peer");
        Self::create_tls_acceptor(&identity.tls_certificate, &identity.secret_key)
            .and_then(|ssl_acceptor| Ssl::new(ssl_acceptor.context()))
            .and_then(|ssl| SslStream::new(ssl, stream))
            .map_err(|e| ManagerError::Tls(TLSError::TlsInitialization(e.to_string())))
    }

    pub async fn perform_tls_handshake(
        transport: &mut SslStream<TcpStream>,
    ) -> Result<(), ManagerError> {
        info!("Starting TLS level handshake");
        SslStream::accept(Pin::new(transport))
            .await
            .map_err(|e| ManagerError::Tls(TLSError::TlsHandshake(e.to_string())))
    }

    pub async fn listen_on_endpoint(&self) -> JoinHandle<()> {
        let connection_pool = self.connection_pool.clone();
        let identity = self.identity.clone();
        let tcp_ep = self.tcp_ep.clone();
        info!("Starting to listen on TCP Endpoint for incoming connections");
        tokio::spawn(async move {
            loop {
                let (stream, peer_addr) = match tcp_ep.lock().await.accept().await {
                    Ok(connection) => connection,
                    Err(e) => {
                        error!("Error accepting connection at endpoint {e:?}");
                        continue;
                    }
                };

                info!("New connection received!");
                info!("Setting up TLS with connected peer");
                let mut transport: SslStream<TcpStream> =
                    match Self::setup_tls(stream, &identity).await {
                        Ok(stream) => stream,
                        Err(e) => {
                            error!("Error accepting connection at endpoint {e:?}");
                            continue;
                        }
                    };

                info!("Performing TLS handshake with connected peer");
                if let Err(e) = Self::perform_tls_handshake(&mut transport).await {
                    error!("Error accepting connection to endpoint {e:?}");
                    continue;
                }

                info!("Receiving peer Ssl certificates");
                let peer_cert =
                    match transport.ssl().peer_certificate().ok_or(TLSError::NoPeerCertificate) {
                        Ok(cert) => cert,
                        Err(e) => {
                            error!("Error accepting connection at endpoint {e:?}");
                            continue;
                        }
                    };

                info!("Verifying peer's certificates for sanity");
                let _validated_peer_cert = match validate_self_signed_cert(peer_cert) {
                    Ok(peer_cert) => peer_cert,
                    Err(e) => {
                        error!("Error accepting connection at endpoint {e:?}");
                        continue;
                    }
                };

                info!("Framing the stream to match Casper's encoding");
                // Frame the transport
                let framed_transport = tokio_util::codec::Framed::new(
                    transport,
                    LengthDelimitedCodec::builder().max_frame_length(MAX_FRAME_LEN).new_codec(),
                );

                info!("Inserting stream into our connection pool");
                // insert into connection pool
                let _ = connection_pool.lock().await.insert(peer_addr, framed_transport);
            }
        })
    }

    pub async fn listen_to_connection_pool<P: Payload>(
        &self,
        event_tx: Sender<(SocketAddr, Message<P>)>,
    ) -> JoinHandle<()> {
        info!("Starting connection pool listener thread");
        let schultz_addr = self.schultz_addr();
        let all_receivers = self.connection_pool.clone();
        let chainspec = self.chainspec.clone();
        let awaiting_reply_from_peers = self.awaiting_hs_reply_from.clone();
        let fully_connected_peers = self.fully_connected_peers.clone();
        tokio::spawn(async move {
            // Polling interval
            let mut interval = interval(Duration::from_millis(POLLING_RATE));

            loop {
                // Wait for the polling to happen
                interval.tick().await;

                for (peer_addr, stream) in all_receivers.lock().await.iter_mut() {
                    // Split into a bi-directional stream
                    let (mut writer, mut reader) = stream.split();

                    if let Ok(Some(msg)) =
                        tokio::time::timeout(Duration::from_millis(POLLING_RATE), reader.next())
                            .await
                    {
                        match msg {
                            Ok(bytes_read) => {
                                let mut encoder = MessagePackFormat;
                                let remote_message: Result<Message<P>, io::Error> =
                                    Pin::new(&mut encoder).deserialize(&bytes_read);

                                if let Ok(msg) = remote_message {
                                    match &msg {
                                        Message::Handshake {
                                            network_name,
                                            protocol_version,
                                            chainspec_hash,
                                            ..
                                        } => {
                                            if fully_connected_peers
                                                .lock()
                                                .await
                                                .contains(peer_addr)
                                            {
                                                info!(
                                                    "Finished handshake to {peer_addr:?}. \
                                                     Ignoring redundant Handshakes"
                                                );
                                                continue;
                                            }

                                            if awaiting_reply_from_peers
                                                .lock()
                                                .await
                                                .contains(peer_addr)
                                            {
                                                info!("Received handshake from the contacted peer");

                                                if Self::is_valid_handshake_parameters(
                                                    network_name,
                                                    protocol_version,
                                                    chainspec_hash,
                                                    &chainspec,
                                                ) {
                                                    info!(
                                                        "Handshake complete! Successfully \
                                                         connected to peer {peer_addr:?}"
                                                    );

                                                    // Remove the peer since we are through with the
                                                    // handshake.
                                                    awaiting_reply_from_peers
                                                        .lock()
                                                        .await
                                                        .retain(|addr| addr != peer_addr);

                                                    fully_connected_peers
                                                        .lock()
                                                        .await
                                                        .push(*peer_addr);
                                                    continue;
                                                } else {
                                                    error!(
                                                        "Error connecting to peer: Bad Handshake \
                                                         parameters"
                                                    );
                                                    continue;
                                                }
                                            }

                                            if !Self::is_valid_handshake_parameters(
                                                network_name,
                                                protocol_version,
                                                chainspec_hash,
                                                &chainspec,
                                            ) {
                                                error!(
                                                    "Error connecting to peer: Bad Handshake \
                                                     parameters"
                                                );
                                                continue;
                                            }

                                            // Notify the event loop
                                            let _ = event_tx.send((*peer_addr, msg.clone())).await;

                                            // Send back a handshake message on the same stream
                                            let hs: Message<P> = Message::Handshake {
                                                network_name: chainspec.network_config.name.clone(),
                                                public_addr: schultz_addr,
                                                protocol_version: chainspec.protocol_version(),
                                                consensus_certificate: None, // not required
                                                is_syncing: false,           // not required
                                                chainspec_hash: Some(chainspec.hash()),
                                            };

                                            info!("Sending Handshake to Casper");
                                            trace!("{hs:?}");

                                            // Serialize our handshake
                                            match Pin::new(&mut encoder)
                                                .serialize(&Arc::new(hs))
                                                .map_err(|e| {
                                                    ManagerError::CouldNotEncodeOurHandshake(
                                                        e.to_string(),
                                                    )
                                                }) {
                                                Ok(bytes) => {
                                                    if let Err(e) = writer.send(bytes).await {
                                                        error!(
                                                            "Error sending handshake to CASPER!: \
                                                             {e:?}"
                                                        );
                                                    }
                                                    fully_connected_peers
                                                        .lock()
                                                        .await
                                                        .push(*peer_addr);
                                                }
                                                Err(e) => {
                                                    error!(
                                                        "Error serializing handshake for Casper!: \
                                                         {e:?}"
                                                    );
                                                    continue;
                                                }
                                            }
                                        }
                                        _ => {
                                            info!("Ignoring post-handshake traffic from Casper");
                                        }
                                    }
                                } else {
                                    // NOTE: This block is just done for demonstration purpose
                                    // This proves that the serialization format has changed since
                                    // handshake was a success.
                                    trace!("BYTES FROM CASPER {bytes_read:?}");

                                    let mut bincode_fmt = BincodeFormat::default();

                                    let _: Message<P> = match Pin::new(&mut bincode_fmt)
                                        .deserialize(&bytes_read)
                                    {
                                        Ok(message) => {
                                            let _ =
                                                event_tx.send((*peer_addr, message.clone())).await;
                                            message
                                        }
                                        Err(e) => {
                                            warn!("Error deserializing {e:?}");
                                            warn!(
                                                "Received an internal message from Casper. \
                                                 Ignoring the deserialization error"
                                            );
                                            continue;
                                        }
                                    };
                                }
                            }
                            Err(e) => {
                                error!("Error reading from client: {:?}", e);
                            }
                        }
                    }
                }
            }
        })
    }

    fn is_valid_handshake_parameters(
        network_name: &String,
        protocol_version: &ProtocolVersion,
        chainspec_hash: &Option<Digest>,
        chainspec: &Chainspec,
    ) -> bool {
        // Check if the handshake is from the correct network
        if network_name != &chainspec.network_config.name {
            error!("Network name in handshake did not match our network");
            return false;
        }

        // Check if the peer is the correct protocol version
        if protocol_version != &chainspec.protocol_config.version {
            error!("ProtocolVersion in handshake did not match our protocol version");
            return false;
        }

        // Check if the peer has Chainspec config. This should be true because we have
        // a protocol version at this point.
        let peer_chainspec_hash = if let Some(hash) = chainspec_hash {
            hash
        } else {
            error!("Chainspec hash missing from handshake.");
            return false;
        };

        // Check if the peer has the same Chainspec config hash
        if peer_chainspec_hash != &chainspec.hash() {
            error!("Chainspec hash in handshake did not match our chainspec.");
            return false;
        }

        true
    }
}
