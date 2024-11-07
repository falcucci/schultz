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

### Running a node as validator

To run a validator node you will need to specify a config file and launch the validator subcommand:

```
sudo RUST_LOG=trace ./target/release/casper-node validator <SCHULTZ-PATH>/examples/config.toml
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
