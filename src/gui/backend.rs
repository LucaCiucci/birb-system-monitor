use super::{BackendId, BackendPanel, BackendPanelId, BackendPanelInfo};
use birb_monitor::{
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

/// Frontend-only panel factories and data stores. No collector lives here.
pub enum FrontendGroup {
    Sysinfo(sysinfo::SysinfoFrontend),
    Docker(docker::DockerFrontend),
}

impl FrontendGroup {
    pub fn name(&self) -> egui::WidgetText {
        match self {
            Self::Sysinfo(v) => v.name(),
            Self::Docker(v) => v.name(),
        }
    }
    pub fn panels(&self) -> Vec<BackendPanelInfo> {
        match self {
            Self::Sysinfo(v) => v.panels(),
            Self::Docker(v) => v.panels(),
        }
    }
    pub fn new_panel(&self, id: &BackendPanelId) -> Box<dyn BackendPanel> {
        match self {
            Self::Sysinfo(v) => v.new_panel(id),
            Self::Docker(v) => v.new_panel(id),
        }
    }
    pub fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        match self {
            Self::Sysinfo(v) => v.save_config(),
            Self::Docker(v) => v.save_config(),
        }
    }
    pub fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        match self {
            Self::Sysinfo(v) => v.load_config(config),
            Self::Docker(v) => v.load_config(config),
        }
    }
}

pub fn init_frontend_groups(_: &Context) -> HashMap<BackendId, FrontendGroup> {
    HashMap::from([
        (
            BackendId("sysinfo".into()),
            FrontendGroup::Sysinfo(sysinfo::SysinfoFrontend::new()),
        ),
        (
            BackendId("docker".into()),
            FrontendGroup::Docker(docker::DockerFrontend::new()),
        ),
    ])
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
    Ssh(birb_monitor::transport::Remote),
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
    pub fn new(cx: Context, groups: &HashMap<BackendId, FrontendGroup>) -> Self {
        Self::connect(cx, groups, None, "birb-monitor").expect("Failed to start backend")
    }

    pub fn connect(
        cx: Context,
        groups: &HashMap<BackendId, FrontendGroup>,
        host: Option<&str>,
        ssh_bin: &str,
    ) -> anyhow::Result<Self> {
        let (transport, commands, mut messages) = match host {
            Some(host) => {
                let (remote, commands, messages) =
                    birb_monitor::transport::Remote::ssh(host, ssh_bin)?;
                (Transport::Ssh(remote), commands, messages)
            }
            None => {
                let (systems, commands, messages) = Systems::new()?;
                (Transport::Local(systems), commands, messages)
            }
        };
        let sysinfo = match &groups[&BackendId("sysinfo".into())] {
            FrontendGroup::Sysinfo(view) => view.state.clone(),
            _ => unreachable!(),
        };
        let docker = match &groups[&BackendId("docker".into())] {
            FrontendGroup::Docker(view) => view.state.clone(),
            _ => unreachable!(),
        };
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
        result.sync_config(groups);
        Ok(result)
    }

    pub fn sync_config(&mut self, groups: &HashMap<BackendId, FrontendGroup>) {
        if self.status.lock().disconnected {
            return;
        }
        for group in groups.values() {
            let values = match group {
                FrontendGroup::Sysinfo(view) => {
                    let state = view.state.lock();
                    let config = state.config.clone();
                    let selected_processes = state
                        .selected_pids()
                        .map(PidV::from)
                        .collect::<HashSet<_>>();
                    let detail_selection =
                        (selected_processes, config.limit_processes_to_selection);
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
                    vec![
                        (SystemId::System, config.update_interval),
                        (SystemId::Components, config.temperature_interval),
                    ]
                }
                FrontendGroup::Docker(view) => {
                    let config = view.state.lock().config.clone();
                    if self.socket.as_ref() != Some(&config.socket_path) {
                        if self
                            .commands
                            .try_send(Command::Docker(DockerCommand::SetSocketPath(
                                config.socket_path.clone(),
                            )))
                            .is_ok()
                        {
                            self.socket = Some(config.socket_path);
                        }
                    }
                    vec![(
                        SystemId::Docker,
                        Duration::from_secs(config.update_interval_secs),
                    )]
                }
            };
            for (target, interval) in values {
                if self.sent.get(&target) != Some(&interval) {
                    if self
                        .commands
                        .try_send(Command::SetInterval { target, interval })
                        .is_ok()
                    {
                        self.sent.insert(target, interval);
                    }
                }
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
