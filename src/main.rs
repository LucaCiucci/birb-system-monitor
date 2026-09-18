use clap::{
    Parser,
    builder::{Styles, styling::AnsiColor},
};

mod gui;
use gui::gui_main;

fn main() -> anyhow::Result<()> {
    let cli = Cli::parse();
    tracing_subscriber::fmt()
        .with_writer(std::io::stderr)
        .init();
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
        self.command.clone().unwrap_or(Command::Gui(Gui {
            detach: true,
            ssh: None,
            ssh_bin: "birb-monitor".into(),
        }))
    }
}

#[derive(Clone, Parser)]
pub enum Command {
    Gui(Gui),
    /// Run an attached backend using versioned JSON Lines on stdin/stdout.
    Backend {
        #[clap(long, required = true)]
        stdio: bool,
    },
}

impl Command {
    pub fn run(&self) -> anyhow::Result<()> {
        match self {
            Command::Gui(gui) => gui.run(),
            Command::Backend { .. } => birb_monitor::transport::serve_stdio(),
        }
    }
}

#[derive(Clone, Parser)]
pub struct Gui {
    #[clap(short, long)]
    detach: bool,
    /// Read from birb-monitor on an SSH host (uses local SSH config and keys).
    #[clap(long)]
    ssh: Option<String>,
    /// Remote birb-monitor executable; accepts an absolute path or `~/...`.
    #[clap(long, requires = "ssh", default_value = "birb-monitor")]
    ssh_bin: String,
}

impl Gui {
    pub fn run(&self) -> anyhow::Result<()> {
        if !self.detach {
            gui_main::main(self.ssh.clone(), self.ssh_bin.clone())
        } else {
            let this_exe = std::env::current_exe()?;
            let mut child = std::process::Command::new(this_exe);
            child.arg("gui");
            if let Some(host) = &self.ssh {
                child.arg("--ssh").arg(host);
                child.arg("--ssh-bin").arg(&self.ssh_bin);
            }
            child.spawn()?;
            Ok(())
        }
    }
}

pub const CLAP_STYLES: Styles = Styles::styled()
    .header(AnsiColor::Green.on_default().bold())
    .usage(AnsiColor::Green.on_default().bold())
    .literal(AnsiColor::BrightCyan.on_default().bold())
    .placeholder(AnsiColor::Cyan.on_default());
