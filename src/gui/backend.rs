use super::{Panel, panels::PanelId};
use crate::{
    backend::Systems,
    backend::docker::DockerCommand,
    backend::sysinfo::{PidV, SysinfoCommand},
    message::{Command, Message, SystemId},
};
use egui::{Context, mutex::Mutex};
use std::{
    collections::{HashMap, HashSet},
    sync::Arc,
    thread::JoinHandle,
    time::Duration,
};

pub mod docker;
pub mod sysinfo;

/// Display data and preferences for both domains; collectors live in Systems.
pub struct FrontendState {
    pub sysinfo: sysinfo::SysinfoFrontend,
    pub docker: docker::DockerFrontend,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct FrontendConfig {
    pub sysinfo: sysinfo::SysinfoConfig,
    pub docker: docker::DockerConfig,
}

impl FrontendState {
    pub fn new() -> Self {
        Self {
            sysinfo: sysinfo::SysinfoFrontend::new(),
            docker: docker::DockerFrontend::new(),
        }
    }

    pub fn config(&self) -> FrontendConfig {
        FrontendConfig {
            sysinfo: self.sysinfo.state.lock().config.clone(),
            docker: self.docker.state.lock().config.clone(),
        }
    }

    pub fn apply_config(&self, config: &FrontendConfig) {
        self.sysinfo.state.lock().config = config.sysinfo.clone();
        self.docker.state.lock().config = config.docker.clone();
    }

    pub fn new_panel(&self, id: &PanelId) -> Box<dyn Panel> {
        match id {
            PanelId::Containers | PanelId::Images => self.docker.new_panel(id),
            PanelId::Cpu
            | PanelId::Memory
            | PanelId::Processes
            | PanelId::SelectedProcess
            | PanelId::Network
            | PanelId::DiskIo
            | PanelId::Dashboard
            | PanelId::Settings
            | PanelId::Temperature
            | PanelId::TemperatureChart => self.sysinfo.new_panel(id),
        }
    }
}

/// Frontend adapter: receives backend messages, maintains display data, and
/// wakes egui. The backend sees only the two channel endpoints.
pub struct Connection {
    transport: Transport,
    commands: tokio::sync::mpsc::Sender<Command>,
    receiver: Option<JoinHandle<()>>,
    sent: HashMap<SystemId, Duration>,
    process_detail_selection: Option<(HashSet<PidV>, bool)>,
    socket: Option<String>,
    pub status: Arc<Mutex<ConnectionStatus>>,
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
    #[cfg(test)]
    pub fn new(cx: Context, frontend: &FrontendState) -> Self {
        Self::connect(cx, frontend, None, "birb-monitor").expect("Failed to start backend")
    }

    pub fn connect(
        cx: Context,
        frontend: &FrontendState,
        host: Option<&str>,
        ssh_bin: &str,
    ) -> anyhow::Result<Self> {
        let (transport, commands, mut messages) = match host {
            Some(host) => {
                let (remote, commands, messages) = crate::transport::Remote::ssh(host, ssh_bin)?;
                (Transport::Ssh(remote), commands, messages)
            }
            None => {
                let (systems, commands, messages) = Systems::new()?;
                (Transport::Local(systems), commands, messages)
            }
        };
        let sysinfo = frontend.sysinfo.state.clone();
        let docker = frontend.docker.state.clone();
        let status = Arc::new(Mutex::new(ConnectionStatus::default()));
        let received_status = status.clone();
        let receiver = std::thread::spawn(move || {
            while let Some(message) = messages.blocking_recv() {
                match message {
                    Message::Sysinfo(value) => sysinfo.lock().receive(value),
                    Message::Docker(value) => docker.lock().receive(value),
                    Message::IntervalChanged { target, interval } => {
                        match target {
                            SystemId::System => {
                                sysinfo.lock().applied_update_interval = Some(interval)
                            }
                            SystemId::Components => {
                                sysinfo.lock().applied_temperature_interval = Some(interval)
                            }
                            SystemId::Docker => {}
                        }
                        received_status.lock().intervals.insert(target, interval);
                    }
                    Message::CommandError(error) => received_status.lock().error = Some(error),
                }
                cx.request_repaint();
            }
            received_status.lock().disconnected = true;
            cx.request_repaint();
        });
        let mut result = Self {
            transport,
            commands,
            receiver: Some(receiver),
            sent: HashMap::new(),
            process_detail_selection: None,
            socket: None,
            status,
        };
        result.sync_config(frontend);
        Ok(result)
    }

    pub fn sync_config(&mut self, frontend: &FrontendState) {
        if self.status.lock().disconnected {
            return;
        }
        let (config, selected_processes) = {
            let state = frontend.sysinfo.state.lock();
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
        let docker = frontend.docker.state.lock().config.clone();
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
        self.transport.shutdown();
        if let Some(receiver) = self.receiver.take() {
            let _ = receiver.join();
        }
    }
}
