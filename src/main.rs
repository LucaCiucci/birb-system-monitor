use birb_monitor::cli::{Cli, init_logging};
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    init_logging();
    cli.command().run()
}
