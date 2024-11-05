use std::collections::BTreeMap;
use std::net::SocketAddr;
use std::sync::Arc;

use openssl::ssl::SslStream;
use tokio::net::TcpListener;
use tokio::net::TcpStream;
use tokio::sync::mpsc::Sender;
use tokio::sync::Mutex;
use tokio::task::JoinHandle;
use tokio_util::codec::LengthDelimitedCodec;
use tracing::debug;

use super::error::ManagerError;
use super::message::Message;
use super::tls::Identity;
use crate::primitives::Chainspec;

/// Transport type alias for base encrypted connections.
type Transport = SslStream<TcpStream>;
pub type FramedTransport = tokio_util::codec::Framed<Transport, LengthDelimitedCodec>;

pub struct Manager {
    local_addr: SocketAddr,
    tcp_ep: Arc<Mutex<TcpListener>>,
    identity: Identity,
    chainspec: Chainspec,
    connection_pool: Arc<Mutex<BTreeMap<SocketAddr, FramedTransport>>>,
    awaiting_hs_reply_from: Arc<Mutex<Vec<SocketAddr>>>,
    fully_connected_peers: Arc<Mutex<Vec<SocketAddr>>>,
    endpoint_listener_handle: Option<JoinHandle<()>>,
    conn_pool_listener_handle: Option<JoinHandle<()>>,
}

impl Manager {
    pub async fn new(
        local_addr: SocketAddr,
        event_tx: Sender<(SocketAddr, Message<Vec<u8>>)>,
        chainspec: Chainspec,
    ) -> Result<Self, ManagerError> {
        let listener = TcpListener::bind(local_addr)
            .await
            .map_err(|error| ManagerError::ListenerCreation(error, local_addr))?;

        let identity = Identity::with_generated_certs().expect("Failed to generate identity");

        let mut network = Self {
            local_addr,
            tcp_ep: Arc::new(Mutex::new(listener)),
            identity,
            chainspec,
            connection_pool: Arc::new(Mutex::new(BTreeMap::new())),
            awaiting_hs_reply_from: Arc::new(Mutex::new(Vec::new())),
            fully_connected_peers: Arc::new(Mutex::new(Vec::new())),
            endpoint_listener_handle: None,
            conn_pool_listener_handle: None,
        };

        Ok(network)
    }
}
