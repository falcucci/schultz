pub mod commands;
pub mod dirs;
pub mod error;
pub mod network;
pub mod node;
pub mod primitives;
pub mod utils;

use std::path::PathBuf;

use clap::Parser;
use clap::Subcommand;
use clap::ValueEnum;

#[derive(ValueEnum, Clone)]
pub enum OutputFormat {
    Json,
    Table,
}

#[derive(clap::Parser)]
pub struct Args {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
pub enum Commands {
    #[command(author, version, about = "Bootstrap a Schultz node for Casper network", long_about = None)]
    Bootstrap {
        #[arg(
            short,
            long,
            value_name = "addr",
            help = "Schultz SocketAddr to bind to",
            env = "ADDR"
        )]
        addr: String,

        #[arg(
            short,
            long,
            value_name = "bootnode",
            help = "Casper SocketAddr to bootstrap from",
            env = "BOOTNODE"
        )]
        bootnode: String,

        #[arg(
            short,
            long,
            value_name = "chainspec",
            help = "Path to the chainspec file",
            env = "CHAINSPEC_PATH"
        )]
        chainspec: Option<String>,
    },
    // Config,
}

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
pub struct Cli {
    #[command(subcommand)]
    pub command: Commands,

    #[arg(
        short,
        long,
        global = true,
        help = "root dir for config and data",
        env = "Schultz_ROOT_DIR"
    )]
    root_dir: Option<PathBuf>,

    #[arg(
        short,
        long,
        global = true,
        help = "output format for command response",
        env = "Schultz_OUTPUT_FORMAT"
    )]
    output_format: Option<OutputFormat>,
}

pub struct Context {
    pub dirs: dirs::Dirs,
    pub output_format: OutputFormat,
}

impl Context {
    pub fn for_cli(cli: &Cli) -> miette::Result<Self> {
        let dirs = dirs::Dirs::try_new(cli.root_dir.as_deref())?;
        let output_format = cli.output_format.clone().unwrap_or(OutputFormat::Table);

        Ok(Context {
            dirs,
            output_format,
        })
    }
}
