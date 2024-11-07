<div align="center">

# SCHULTZ

_The Schultz node - a handshake peer aware of its own identity._

</div>

![CleanShot 2024-11-07 at 04 51 33@2x](https://github.com/user-attachments/assets/e55d4461-d287-4588-8998-746992853588)

A [Carper](https://github.com/casper-network/casper-node)-based blockchain node written in Rust, to be used as a peer in the Casper network. The Schultz node is a handshake peer that is aware of its own identity.

### Setup

Clone the Casper repository:

```bash
git clone git@github.com:casper-network/casper-node.git
```

Clone the Schultz repository:

```bash
git clone git@github.com:falcucci/schultz.git
```

The Casper node must be compiled first so before building the Casper node, prepare your Rust build environment:

```bash
cd casper-node && make setup-rs
```

The node software can be compiled afterwards:

```bash
cargo build -p casper-node --release
```

## The very first handshake trial

### Running a node as bootstraped validator

To run a validator node you will need to specify a config file and launch the validator subcommand:

```
sudo RUST_LOG=trace ./target/release/casper-node validator <SCHULTZ-PATH>/examples/config.toml
```

The Carper node should now be running so we can handle the incoming connections.

<div align="center">
    <img src="https://github.com/user-attachments/assets/5c1db361-26c0-4e24-8dfb-8433f2f3d05a" alt="Terminal">
    <em>Casper node running as an initiator</em>
</div>

### Running the Schultz node

```bash
cd schultz && cargo build --release
```

The Schultz node can be run with the following command:

```bash
RUST_LOG=trace ./target/release/schultz bootstrap --addr 127.0.0.1:5001 --bootnode 127.0.0.1:34553 --chainspec ./examples
```

And the similar handshake trace can be found:

```bash
2024-11-07T05:09:51.320354Z  INFO schultz::node: Starting node at 127.0.0.1:5001
2024-11-07T05:09:51.326674Z  INFO schultz::network::manager: Starting network communications...
2024-11-07T05:09:51.326770Z  INFO schultz::network::tls: Generating new keys and certificates
2024-11-07T05:09:51.330164Z  INFO schultz::network::manager: Starting to listen on TCP Endpoint for incoming connections
2024-11-07T05:09:51.330212Z  INFO schultz::network::manager: Starting connection pool listener thread
2024-11-07T05:09:51.330266Z  INFO schultz::network::manager: Network communications started!
2024-11-07T05:09:51.330275Z TRACE schultz::network::manager: Waiting for incoming connections...
2024-11-07T05:09:51.330282Z  INFO schultz::network::manager: Connecting to 127.0.0.1:34553
2024-11-07T05:09:51.338439Z TRACE schultz::network::manager: 1.Trying to send a Handshake to 127.0.0.1:34553
2024-11-07T05:09:51.338454Z  INFO schultz::network::manager: Sending message to 127.0.0.1:34553
2024-11-07T05:09:51.338479Z TRACE tokio_util::codec::framed_impl: flushing framed transport
2024-11-07T05:09:51.338485Z TRACE tokio_util::codec::framed_impl: writing; remaining=103
2024-11-07T05:09:51.338502Z TRACE tokio_util::codec::framed_impl: framed transport flushed
2024-11-07T05:09:51.338520Z  INFO schultz::network::manager: Sent a handshake to 127.0.0.1:34553
2024-11-07T05:09:51.338528Z  INFO schultz::node: Started node at 127.0.0.1:5001
2024-11-07T05:09:51.338535Z  INFO schultz::node: Starting keepalive task
2024-11-07T05:09:51.341658Z TRACE tokio_util::codec::framed_impl: attempting to decode a frame
2024-11-07T05:09:51.341681Z TRACE tokio_util::codec::framed_impl: frame decoded from buffer
2024-11-07T05:09:51.341855Z  INFO schultz::network::manager: Received handshake from the contacted peer
2024-11-07T05:09:51.341966Z  INFO schultz::network::manager: Handshake complete! Successfully connected to peer 127.0.0.1:34553
2024-11-07T05:09:51.341985Z TRACE tokio_util::codec::framed_impl: attempting to decode a frame
2024-11-07T05:09:52.326362Z  INFO schultz::network::manager: New connection received!
2024-11-07T05:09:52.326396Z  INFO schultz::network::manager: Setting up TLS with connected peer
2024-11-07T05:09:52.326407Z  INFO schultz::network::manager: Setting up TLS with connected peer
2024-11-07T05:09:52.326417Z  INFO schultz::network::manager: Creating TLS acceptor for incoming connections
2024-11-07T05:09:52.326690Z  INFO schultz::network::manager: Performing TLS handshake with connected peer
2024-11-07T05:09:52.326708Z  INFO schultz::network::manager: Starting TLS level handshake
2024-11-07T05:09:52.338368Z  INFO schultz::network::manager: Receiving peer Ssl certificates
2024-11-07T05:09:52.338414Z  INFO schultz::network::manager: Verifying peer's certificates for sanity
2024-11-07T05:09:52.339834Z  INFO schultz::network::manager: Framing the stream to match Casper's encoding
2024-11-07T05:09:52.339854Z  INFO schultz::network::manager: Inserting stream into schultz connection pool
2024-11-07T05:09:52.342400Z TRACE tokio_util::codec::framed_impl: attempting to decode a frame
2024-11-07T05:09:52.342436Z TRACE tokio_util::codec::framed_impl: frame decoded from buffer
2024-11-07T05:09:52.342905Z  INFO schultz::node: Received handshake from 127.0.0.1:52434
2024-11-07T05:09:52.342928Z  INFO schultz::network::manager: Sending a ping to 127.0.0.1:34553
2024-11-07T05:09:52.343039Z  INFO schultz::network::manager: Sending message to 127.0.0.1:34553
2024-11-07T05:09:52.343054Z  INFO schultz::network::manager: Sending Handshake to Casper
2024-11-07T05:09:52.343065Z TRACE schultz::network::manager: Handshake { network_name: "casper", public_addr: 127.0.0.1:5001, protocol_version: ProtocolVersion(SemVer { major: 1, minor: 5, patch: 2 }), consensus_certificate: None, is_syncing: false, chainspec_hash: Some(8b0c9bd3559fc2574a7aa76c26ebaabe50a9ff372d38bcaa5d6ad7e963aeff28) }
```

### Generating keys using OpenSSL

The keys are already generated at `./examples` path but just in case you want just generate new ones by generating the secret_key.pem file

```bash
openssl genpkey -algorithm ed25519 -out secret_key.pem
```

Generating public keys from the secret_key.pem file

```bash
openssl pkey -in secret_key.pem -pubout -out public_key.pem

{ echo -n 01; openssl pkey -outform DER -pubout -in "secret_key.pem" | tail -c +13 | openssl base64 | openssl base64 -d | hexdump -ve '/1 "%02x" ' | tr -d "/n"; } > public_key_hex
```
