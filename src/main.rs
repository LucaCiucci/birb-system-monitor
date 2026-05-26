use birb_system_monitor_gui::gui_main;
use clap::{Parser, builder::{Styles, styling::AnsiColor}};

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt::init();
    cli.command().run()
}

/// Birb System Monitor
///
/// A cross-platform system monitoring tool with a focus on aesthetics and usability.
///
/// If no command is provided, defaults to `gui --detach`.
#[derive(Clone, Parser)]
#[clap(styles = CLAP_STYLES)]
struct Cli {
    #[clap(subcommand)]
    command: Option<Command>,
}

impl Cli {
    pub fn command(&self) -> Command {
        self.command.clone().unwrap_or(Command::Gui(Gui { detach: true }))
    }
}

#[derive(Clone, Parser)]
pub enum Command {
    Gui(Gui),
}

impl Command {
    pub fn run(&self) -> anyhow::Result<()> {
        match self {
            Command::Gui(gui) => gui.run(),
        }
    }
}

#[derive(Clone, Parser)]
pub struct Gui {
    #[clap(short, long)]
    detach: bool,
}

impl Gui {
    pub fn run(&self) -> anyhow::Result<()> {
        if !self.detach {
            gui_main::main()
        } else {
            let this_exe = std::env::current_exe()?;
            std::process::Command::new(this_exe)
                .arg("gui")
                .spawn()?;
            Ok(())
        }
    }
}

pub const CLAP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::BrightCyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());
