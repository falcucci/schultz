use std::collections::BTreeMap;
use std::io;
use std::net::SocketAddr;
use std::pin::Pin;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use bytes::BytesMut;
use casper_hashing::Digest;
use casper_types::ProtocolVersion;
use futures::stream::SplitSink;
use futures::SinkExt;
use futures::StreamExt;
use openssl::pkey::PKeyRef;
use openssl::pkey::Private;
use openssl::ssl::Ssl;
use openssl::ssl::SslAcceptor;
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
use tokio_util::codec::Framed;
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

/// # Manager
///
/// The `Manager` struct is responsible for handling network communications,
/// including setting up TLS connections, managing a pool of connections, and
/// performing handshakes with peers.
///
/// ## Usage
///
/// To create a new `Manager`, use the `new` method, providing the required
/// parameters such as `schultz_addr`, `event_tx`, and `chainspec`.
///
/// ```rust
/// let manager = Manager::new(schultz_addr, event_tx, chainspec).await?; 
/// ```
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
    /// Creates a new `Manager` instance.
    ///
    /// This method initializes network communications, binds to the provided
    /// address, and sets up the necessary listeners for managing connections.
    ///
    /// # Parameters
    ///
    /// - `schultz_addr`: The address to bind the network listener.
    /// - `event_tx`: A channel sender for transmitting events.
    /// - `chainspec`: The chainspec configuration for the network.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the new `Manager` instance or an error
    /// if initialization fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let manager = Manager::new(schultz_addr, event_tx, chainspec).await?; 
    /// ```
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

    /// Connects to a peer at the specified address.
    ///
    /// This method establishes a TCP connection to the given address and
    /// performs a TLS handshake.
    ///
    /// # Parameters
    ///
    /// - `addr`: The address of the peer to connect to.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the connection is successful, or a `ManagerError`
    /// if an error occurs.
    ///
    /// # Example
    ///
    /// ```rust
    /// manager.connect(&peer_addr).await?; 
    /// ```
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

    /// Sends a handshake message to a peer.
    ///
    /// This method constructs and sends a handshake message to the specified
    /// peer address.
    ///
    /// # Parameters
    ///
    /// - `addr`: The address of the peer to send the handshake to.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the handshake is sent successfully, or a
    /// `ManagerError` if an error occurs.
    ///
    /// # Example
    ///
    /// ```rust
    /// manager.handshake::<Payload>(peer_addr).await?; 
    /// ```
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

    /// Sends a message to a peer.
    ///
    /// This method sends the provided payload to the specified peer address.
    ///
    /// # Parameters
    ///
    /// - `addr`: The address of the peer to send the message to.
    /// - `payload`: The message payload to send.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the message is sent successfully, or a
    /// `ManagerError` if an error occurs.
    ///
    /// # Example
    ///
    /// ```rust
    /// manager.send_message(peer_addr, payload).await?; 
    /// ```
    pub async fn send_message(&self, addr: SocketAddr, payload: Bytes) -> Result<(), ManagerError> {
        info!("Sending message to {addr:?}");
        let mut conn_pool = self.connection_pool.lock().await;
        let peer_connection = conn_pool.get_mut(&addr).ok_or(ManagerError::PeerNotFound)?;
        let message = SchultzMessage::new(payload)?;
        message.write_to_stream(peer_connection).await?;

        Ok(())
    }

    /// Sends a ping message to a peer.
    ///
    /// This method constructs and sends a ping message to the specified peer
    /// address.
    ///
    /// # Parameters
    ///
    /// - `addr`: The address of the peer to send the ping to.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the ping is sent successfully, or a
    /// `ManagerError` if an error occurs.
    ///
    /// # Example
    ///
    /// ```rust
    /// manager.send_ping::<YourPayloadType>(peer_addr).await?; 
    /// ```
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

    /// Creates a TLS acceptor for incoming connections.
    ///
    /// This function sets up an `SslAcceptor` using the provided certificate
    /// and private key. The acceptor restricts TLS parameters to secure ones
    /// defined in this crate that are compatible with connectors built with
    /// `create_tls_connector`.
    ///
    /// # Parameters
    ///
    /// - `cert`: A reference to the certificate (`X509Ref`) used for TLS.
    /// - `private_key`: A reference to the private key (`PKeyRef<Private>`)
    ///   used for TLS.
    ///
    /// # Returns
    ///
    /// Returns an `SslResult` containing the `SslAcceptor` if successful,
    /// or an error if the acceptor could not be created.
    ///
    /// # Example
    ///
    /// ```rust
    /// let acceptor = Manager::create_tls_acceptor(&cert, &private_key)?; 
    /// ```
    pub fn create_tls_acceptor(
        cert: &X509Ref,
        private_key: &PKeyRef<Private>,
    ) -> SslResult<SslAcceptor> {
        info!("Creating TLS acceptor for incoming connections");
        let mut builder = SslAcceptor::mozilla_modern_v5(SslMethod::tls_server())?;
        set_context_options(&mut builder, cert, private_key)?;

        Ok(builder.build())
    }

    /// Sets up a TLS connection with a peer.
    ///
    /// This asynchronous function initializes a TLS stream on top of an
    /// existing TCP stream using the identity provided. It returns a
    /// `SslStream` that can be used for secure communication.
    ///
    /// # Parameters
    ///
    /// - `stream`: The TCP stream to wrap with TLS.
    /// - `identity`: The identity containing the TLS certificate and secret
    ///   key.
    ///
    /// # Returns
    ///
    /// Returns a `Result` containing the `SslStream` if successful, or a
    /// `ManagerError` if the setup fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// let tls_stream = Manager::setup_tls(tcp_stream, &identity).await?; 
    /// ```
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

    /// Performs a TLS handshake on the given stream.
    ///
    /// This asynchronous function executes a TLS handshake with the connected
    /// peer. It ensures that the secure connection is properly established.
    ///
    /// # Parameters
    ///
    /// - `transport`: A mutable reference to the `SslStream` on which to
    ///   perform the handshake.
    ///
    /// # Returns
    ///
    /// Returns `Ok(())` if the handshake is successful, or a `ManagerError`
    /// if it fails.
    ///
    /// # Example
    ///
    /// ```rust
    /// Manager::perform_tls_handshake(&mut tls_stream).await?; 
    /// ```
    pub async fn perform_tls_handshake(
        transport: &mut SslStream<TcpStream>,
    ) -> Result<(), ManagerError> {
        info!("Starting TLS level handshake");
        SslStream::accept(Pin::new(transport))
            .await
            .map_err(|e| ManagerError::Tls(TLSError::TlsHandshake(e.to_string())))
    }

    /// Listens for incoming connections on the TCP endpoint.
    ///
    /// This asynchronous function continuously listens for new connections
    /// on the configured TCP endpoint. For each connection, it sets up TLS
    /// and performs a handshake, then adds the connection to the connection
    /// pool.
    ///
    /// # Returns
    ///
    /// Returns a `JoinHandle<()>` that represents the spawned task.
    ///
    /// # Example
    ///
    /// ```rust
    /// let handle = manager.listen_on_endpoint().await; 
    /// ```
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

                info!("Inserting stream into schultz connection pool");
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
                                Self::handle_incoming_message(
                                    &schultz_addr,
                                    &chainspec,
                                    peer_addr,
                                    &fully_connected_peers,
                                    &awaiting_reply_from_peers,
                                    &event_tx,
                                    bytes_read,
                                    &mut writer,
                                )
                                .await
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

    #[allow(clippy::too_many_arguments)]
    pub async fn handle_incoming_message<P: Payload>(
        schultz_addr: &SocketAddr,
        chainspec: &Chainspec,
        peer_addr: &SocketAddr,
        fully_connected_peers: &Arc<Mutex<Vec<SocketAddr>>>,
        awaiting_reply_from_peers: &Arc<Mutex<Vec<SocketAddr>>>,
        event_tx: &Sender<(SocketAddr, Message<P>)>,
        bytes_read: BytesMut,
        writer: &mut SplitSink<&mut Framed<SslStream<TcpStream>, LengthDelimitedCodec>, Bytes>,
    ) {
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
                    Self::handle_handshake_message(
                        &msg,
                        network_name,
                        protocol_version,
                        chainspec_hash,
                        schultz_addr,
                        chainspec,
                        peer_addr,
                        fully_connected_peers,
                        awaiting_reply_from_peers,
                        event_tx,
                        writer,
                    )
                    .await;
                }
                _ => {
                    info!("Ignoring post-handshake traffic from Casper");
                }
            }
        } else {
            // In essence, this approach helps maintain a stable and reliable network,
            // minimizing the impact of version differences and unexpected data formats on
            // the overall operation.
            //
            // it's common in peer-to-peer networks that each node
            // might be running different protocol versions even intentionally, which can
            // lead to changes in how messages are formatted. This code is
            // designed to handle those situations gracefully.
            trace!("BYTES FROM CASPER {bytes_read:?}");

            let mut bincode_fmt = BincodeFormat::default();

            let _: Message<P> = match Pin::new(&mut bincode_fmt).deserialize(&bytes_read) {
                Ok(message) => {
                    let _ = event_tx.send((*peer_addr, message.clone())).await;
                    message
                }
                Err(e) => {
                    warn!("Error deserializing {e:?}");
                    warn!(
                        "Received an internal message from Casper. Ignoring the deserialization \
                         error"
                    );
                    return;
                }
            };
        }
    }

    #[allow(clippy::too_many_arguments)]
    async fn handle_handshake_message<P: Payload>(
        msg: &Message<P>,
        network_name: &String,
        protocol_version: &ProtocolVersion,
        chainspec_hash: &Option<Digest>,
        schultz_addr: &SocketAddr,
        chainspec: &Chainspec,
        peer_addr: &SocketAddr,
        fully_connected_peers: &Arc<Mutex<Vec<SocketAddr>>>,
        awaiting_reply_from_peers: &Arc<Mutex<Vec<SocketAddr>>>,
        event_tx: &Sender<(SocketAddr, Message<P>)>,
        writer: &mut SplitSink<&mut Framed<SslStream<TcpStream>, LengthDelimitedCodec>, Bytes>,
    ) {
        if fully_connected_peers.lock().await.contains(peer_addr) {
            info!("Finished handshake to {peer_addr:?}. Ignoring redundant Handshakes");
            return;
        }

        if awaiting_reply_from_peers.lock().await.contains(peer_addr) {
            info!("Received handshake from the contacted peer");

            if Self::is_valid_handshake_parameters(
                network_name,
                protocol_version,
                chainspec_hash,
                chainspec,
            ) {
                info!("Handshake complete! Successfully connected to peer {peer_addr:?}");

                // Remove the peer since we are through with the
                // handshake.
                awaiting_reply_from_peers.lock().await.retain(|addr| addr != peer_addr);

                fully_connected_peers.lock().await.push(*peer_addr);
                return;
            } else {
                error!("Error connecting to peer: Bad Handshake parameters");
                return;
            }
        }

        if !Self::is_valid_handshake_parameters(
            network_name,
            protocol_version,
            chainspec_hash,
            chainspec,
        ) {
            error!("Error connecting to peer: Bad Handshake parameters");
            return;
        }

        // Notify the event loop
        let _ = event_tx.send((*peer_addr, msg.clone())).await;

        // Send back a handshake message on the same stream
        let hs: Message<P> = Message::Handshake {
            network_name: chainspec.network_config.name.clone(),
            public_addr: *schultz_addr,
            protocol_version: chainspec.protocol_version(),
            consensus_certificate: None, // not required
            is_syncing: false,           // not required
            chainspec_hash: Some(chainspec.hash()),
        };

        info!("Sending Handshake to Casper");
        trace!("{hs:?}");

        // Serialize schultz handshake
        match Pin::new(&mut MessagePackFormat)
            .serialize(&Arc::new(hs))
            .map_err(|e| ManagerError::CouldNotEncodeOurHandshake(e.to_string()))
        {
            Ok(bytes) => {
                if let Err(e) = writer.send(bytes).await {
                    error!("Error sending handshake to CASPER!: {e:?}");
                }
                fully_connected_peers.lock().await.push(*peer_addr);
            }
            Err(e) => {
                error!("Error serializing handshake for Casper!: {e:?}");
            }
        }
    }

    fn is_valid_handshake_parameters(
        network_name: &String,
        protocol_version: &ProtocolVersion,
        chainspec_hash: &Option<Digest>,
        chainspec: &Chainspec,
    ) -> bool {
        // Check if the handshake is from the correct network
        if network_name != &chainspec.network_config.name {
            error!("Network name in handshake did not match schultz network");
            return false;
        }

        // Check if the peer is the correct protocol version
        if protocol_version != &chainspec.protocol_config.version {
            error!("ProtocolVersion in handshake did not match schultz protocol version");
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
            error!("Chainspec hash in handshake did not match schultz chainspec.");
            return false;
        }

        true
    }
}
