use std::net::SocketAddr;
use std::path::PathBuf;
use std::str::FromStr;

use crate::dirs;
use crate::node::Node;
use crate::Context;

pub async fn run(
    _ctx: Context,
    addr: String,
    bootnodes: String,
    chainspec: Option<String>,
) -> miette::Result<()> {
    println!("Bootstrap Schultz node");
    let schultz_addr = SocketAddr::from_str(&addr).expect("Invalid Schultz address");
    println!("Schultz address: {}", schultz_addr);

    let bootnodes_addrs = SocketAddr::from_str(&bootnodes).expect("Invalid bootnode address");
    println!("Bootnode address: {}", bootnodes_addrs);

    let chainspec_path = chainspec.unwrap_or_else(|| {
        dirs::ensure_root_dir(None)
            .expect("No home directory")
            .join(".casper-node/chainspec/chainspec.toml")
            .to_string_lossy()
            .to_string()
    });
    println!("Chainspec path: {}", chainspec_path);

    let node = Node::new(
        schultz_addr,
        vec![bootnodes_addrs],
        PathBuf::from(chainspec_path),
    )
    .await
    .expect("Failed to create Schultz node");

    Ok(())
}
