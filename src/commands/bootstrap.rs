use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use crate::dirs;
use crate::node::Node;

pub async fn setup(
    addr: String,
    bootnode_addr: Option<String>,
    chainspec: Option<String>,
) -> miette::Result<()> {
    let schultz_addr = SocketAddr::from_str(&addr).expect("Invalid Schultz address");

    let mut bootnodes = vec![];
    if let Some(peer_addr) = &bootnode_addr {
        bootnodes.push(SocketAddr::from_str(peer_addr).expect("Invalid bootnode address"));
    }

    let chainspec_path = chainspec.unwrap_or_else(|| {
        dirs::ensure_root_dir(None)
            .expect("No home directory")
            .join(".casper-node/chainspec/chainspec.toml")
            .to_string_lossy()
            .to_string()
    });

    let node = Node::new(schultz_addr, bootnodes, PathBuf::from(chainspec_path));
    match node.await {
        Ok(instance) => {
            instance.keepalive().await;
        }
        Err(e) => eprintln!("Node failed: {}", e),
    }

    Ok(())
}
