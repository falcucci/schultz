use std::net::SocketAddr;
use std::path::PathBuf;
use std::sync::Arc;

use tokio::sync::mpsc::Receiver;
use tokio::sync::RwLock;

use crate::error::Result;
use crate::network::manager::Manager;
use crate::network::message::Message;
use crate::primitives::Chainspec;

/// Channel bounds
pub const CHANNEL_SIZE: usize = 10_000;

type EventReceiver = Receiver<(SocketAddr, Message<Vec<u8>>)>;

#[derive(Clone)]
pub struct Node {
    pub manager: Arc<RwLock<Manager>>,
    pub event_rx: Arc<RwLock<EventReceiver>>,
    pub bootstrap_test_addr: Option<SocketAddr>,
}

impl Node {
    pub async fn new(
        schultz_addr: SocketAddr,
        bootnodes_addrs: Vec<SocketAddr>,
        chainspec_path: PathBuf,
    ) -> Result<Self> {
        let (event_tx, event_rx) = tokio::sync::mpsc::channel(CHANNEL_SIZE);
        let chainspec = Chainspec::from_path(&chainspec_path).expect("Failed to load chainspec");

        let manager = Manager::new(schultz_addr, event_tx, chainspec).await?;

        let mut node = Self {
            manager: Arc::new(RwLock::new(manager)),
            event_rx: Arc::new(RwLock::new(event_rx)),
            bootstrap_test_addr: None,
        };

        Ok(node)
    }
}
