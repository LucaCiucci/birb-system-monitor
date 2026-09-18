use birb_monitor::cli::Cli;
use clap::Parser;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    birb_monitor::logging::init();
    cli.command().run()
}
