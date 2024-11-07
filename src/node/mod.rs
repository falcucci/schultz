use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;
use tracing::error;
use tracing::info;

use crate::error::Result;
use crate::network::manager::Manager;
use crate::network::message::Message;
use crate::primitives::Chainspec;
use crate::primitives::Payload;

/// Channel bounds
pub const CHANNEL_SIZE: usize = 10_000;

// Placeholder for the casper_types::message::Message's generic.
// Required by the event loop channels
impl Payload for Vec<u8> {}

type EventReceiver = Receiver<(SocketAddr, Message<Vec<u8>>)>;

#[derive(Clone)]
pub struct Node {
    pub manager: Arc<RwLock<Manager>>,
    pub event_rx: Arc<RwLock<EventReceiver>>,
    pub bootnode_addr: Option<SocketAddr>,
}

impl Node {
    pub async fn new(
        schultz_addr: SocketAddr,
        bootnodes_addrs: Vec<SocketAddr>,
        chainspec_path: PathBuf,
    ) -> Result<Self> {
        info!("Starting node at {:?}", schultz_addr);
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
        let chainspec = Chainspec::from_path(&chainspec_path).expect("Failed to load chainspec");

        let manager = Manager::new(schultz_addr, event_tx, chainspec).await?;

        let bootnode_addr = bootnodes_addrs.first().cloned();
        for addr in bootnodes_addrs {
            manager.connect(&addr).await?;
            manager.handshake::<Vec<u8>>(addr).await?;
        }

        info!("Started node at {:?}", manager.schultz_addr());

        Ok(Self {
            manager: Arc::new(RwLock::new(manager)),
            event_rx: Arc::new(RwLock::new(event_rx)),
            bootnode_addr,
        })
    }

    pub async fn keepalive(&self) {
        let event_rx = self.event_rx.clone();
        let manager = self.manager.clone();
        info!("Starting keepalive task");
        loop {
            self.handle_event(event_rx.clone(), manager.clone()).await;
        }
    }

    async fn handle_event(
        &self,
        event_rx: Arc<RwLock<EventReceiver>>,
        manager: Arc<RwLock<Manager>>,
    ) {
        let network_name = manager.read().await.chainspec.network_config.name.clone();
        let (addr, message) = match event_rx.write().await.recv().await {
            Some(event) => event,
            None => return,
        };

        match message {
            Message::Handshake { .. } => {
                info!("Received handshake from {}", addr);
                // Test Handshake
                // NOTE: We cannot reply to the incoming peer address because it is always
                // different to the listening address of the same peer
                // Therefore we'll test a random peer for Ping/Pong
                if let Some(test_addr) = self.bootnode_addr {
                    if let Err(e) = manager.write().await.send_ping::<Vec<u8>>(test_addr).await {
                        error!("Error {e:?} sending ping to {addr:?}");
                    }
                }
            }
            Message::Ping { .. } => {
                info!("Received a {message:?} from {network_name:?}. Not going to send a Pong!");
            }
            Message::Pong { .. } => {
                info!("Received a {message:?} from {addr:?}");
            }
            _ => {
                info!("Received unknown message from {}", addr);
            }
        }
    }
}
