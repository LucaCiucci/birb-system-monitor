use clap::Parser;

fn main() -> anyhow::Result<()> {
    birb_monitor::Cli::parse().main()
}
