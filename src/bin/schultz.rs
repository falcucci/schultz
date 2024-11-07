use clap::Parser;
use schultz::commands::bootstrap;
use schultz::Cli;
use schultz::Commands;
use schultz::Context;

extern crate core;

#[tokio::main]
async fn main() -> miette::Result<()> {
    tracing_subscriber::fmt::init();
    let cli = Cli::parse();
    let _ = Context::for_cli(&cli)?;
    match cli.command {
        Commands::Bootstrap {
            addr,
            bootnode,
            chainspec,
        } => bootstrap::setup(addr, bootnode, chainspec).await,
    }
}
