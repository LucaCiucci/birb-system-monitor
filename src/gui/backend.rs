use crate::gui::app::state::FrontendState;
use crate::{
    backend::Systems,
    backend::docker::DockerCommand,
    backend::sysinfo::{PidV, SysinfoCommand},
    message::{Command, Message, SystemId},
};
use std::{
    collections::{HashMap, HashSet},
    thread::JoinHandle,
    time::Duration,
};

pub mod docker;
pub mod sysinfo;

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FrontendConfig {
    pub sysinfo: sysinfo::SysinfoConfig,
    pub docker: docker::DockerConfig,
}

/// Frontend adapter: applies backend messages when polled by the app. The backend sees only the two channel endpoints.
pub struct Connection {
    transport: Transport,
    commands: tokio::sync::mpsc::Sender<Command>,
    messages: tokio::sync::mpsc::Receiver<Message>,
    relay: Option<JoinHandle<()>>,
    cx: egui::Context,
    sent: HashMap<SystemId, Duration>,
    process_detail_selection: Option<(HashSet<PidV>, bool)>,
    socket: Option<String>,
    pub status: ConnectionStatus,
}

#[derive(Default)]
pub struct ConnectionStatus {
    pub intervals: HashMap<SystemId, Duration>,
    pub error: Option<String>,
    pub disconnected: bool,
}

enum Transport {
    Local(Systems),
    Ssh(crate::transport::Remote),
}

impl Transport {
    fn shutdown(&mut self) {
        match self {
            Self::Local(value) => value.shutdown(),
            Self::Ssh(value) => value.shutdown(),
        }
    }
}

impl Connection {
    pub fn connect(
        cx: egui::Context,
        frontend: &FrontendState,
        host: Option<&str>,
        ssh_bin: &str,
    ) -> anyhow::Result<Self> {
        let (transport, commands, messages) = match host {
            Some(host) => {
                let (remote, commands, messages) = crate::transport::Remote::ssh(host, ssh_bin)?;
                (Transport::Ssh(remote), commands, messages)
            }
            None => {
                let (systems, commands, messages) = Systems::new()?;
                (Transport::Local(systems), commands, messages)
            }
        };
        let (messages, relay) = relay_messages(messages, cx.clone())?;
        let mut result = Self {
            transport,
            commands,
            messages,
            relay: Some(relay),
            cx,
            sent: HashMap::new(),
            process_detail_selection: None,
            socket: None,
            status: ConnectionStatus::default(),
        };
        result.sync_config(frontend);
        Ok(result)
    }

    /// Apply messages on the UI thread before panels borrow the state.
    /// Limit each batch so a busy backend cannot starve rendering.
    pub fn receive(&mut self, frontend: &mut FrontendState) {
        for _ in 0..64 {
            match self.messages.try_recv() {
                Ok(message) => match message {
                    Message::Sysinfo(value) => frontend.sysinfo.state.receive(value),
                    Message::Docker(value) => frontend.docker.state.receive(value),
                    Message::IntervalChanged { target, interval } => {
                        match target {
                            SystemId::System => {
                                frontend.sysinfo.state.applied_update_interval = Some(interval)
                            }
                            SystemId::Components => {
                                frontend.sysinfo.state.applied_temperature_interval = Some(interval)
                            }
                            SystemId::Docker => {}
                        }
                        self.status.intervals.insert(target, interval);
                    }
                    Message::CommandError(error) => self.status.error = Some(error),
                },
                Err(tokio::sync::mpsc::error::TryRecvError::Empty) => return,
                Err(tokio::sync::mpsc::error::TryRecvError::Disconnected) => {
                    self.status.disconnected = true;
                    return;
                }
            }
        }
        // The batch limit may leave queued messages after the relay's last wakeup.
        self.cx.request_repaint();
    }

    pub fn sync_config(&mut self, frontend: &FrontendState) {
        if self.status.disconnected {
            return;
        }
        let (config, selected_processes) = {
            let state = &frontend.sysinfo.state;
            (
                state.config.clone(),
                state
                    .selected_pids()
                    .map(PidV::from)
                    .collect::<HashSet<_>>(),
            )
        };
        let detail_selection = (selected_processes, config.limit_processes_to_selection);
        if self.process_detail_selection.as_ref() != Some(&detail_selection)
            && self
                .commands
                .try_send(Command::Sysinfo(
                    SysinfoCommand::SetProcessDetailSelection {
                        pids: detail_selection.0.iter().copied().collect(),
                        selected_only: detail_selection.1,
                    },
                ))
                .is_ok()
        {
            self.process_detail_selection = Some(detail_selection);
        }
        let docker = frontend.docker.state.config.clone();
        if self.socket.as_ref() != Some(&docker.socket_path)
            && self
                .commands
                .try_send(Command::Docker(DockerCommand::SetSocketPath(
                    docker.socket_path.clone(),
                )))
                .is_ok()
        {
            self.socket = Some(docker.socket_path);
        }
        for (target, interval) in [
            (SystemId::System, config.update_interval),
            (SystemId::Components, config.temperature_interval),
            (
                SystemId::Docker,
                Duration::from_secs(docker.update_interval_secs),
            ),
        ] {
            if self.sent.get(&target) != Some(&interval)
                && self
                    .commands
                    .try_send(Command::SetInterval { target, interval })
                    .is_ok()
            {
                self.sent.insert(target, interval);
            }
        }
    }
}

impl Drop for Connection {
    fn drop(&mut self) {
        // Unblock pending sends before waiting for the backend to stop.
        self.messages.close();
        self.transport.shutdown();
        if let Some(relay) = self.relay.take() {
            let _ = relay.join();
        }
    }
}

// The relay owns only channels and an egui wakeup handle, never frontend state.
fn relay_messages(
    mut source: tokio::sync::mpsc::Receiver<Message>,
    cx: egui::Context,
) -> std::io::Result<(tokio::sync::mpsc::Receiver<Message>, JoinHandle<()>)> {
    let (tx, rx) = tokio::sync::mpsc::channel(64);
    let relay = std::thread::Builder::new()
        .name("frontend-messages".into())
        .spawn(move || {
            while let Some(message) = source.blocking_recv() {
                if tx.blocking_send(message).is_err() {
                    return;
                }
                // Publish before waking the UI so it can read the message immediately.
                cx.request_repaint();
            }
            // Make channel closure visible before waking the UI to report disconnect.
            drop(tx);
            cx.request_repaint();
        })?;
    Ok((rx, relay))
}
