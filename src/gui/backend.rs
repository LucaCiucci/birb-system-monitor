use super::{BackendId, BackendPanel, BackendPanelId, BackendPanelInfo};
use birb_monitor::{
    backend::Systems,
    backend::docker::DockerCommand,
    message::{Command, Message, SystemId},
};
use egui::{Context, mutex::Mutex};
use std::{collections::HashMap, sync::Arc, thread::JoinHandle, time::Duration};

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
pub struct LocalConnection {
    systems: Systems,
    commands: tokio::sync::mpsc::Sender<Command>,
    receiver: Option<JoinHandle<()>>,
    sent: HashMap<SystemId, Duration>,
    socket: Option<String>,
    pub status: Arc<Mutex<ConnectionStatus>>,
}

#[derive(Default)]
pub struct ConnectionStatus {
    pub intervals: HashMap<SystemId, Duration>,
    pub error: Option<String>,
}

impl LocalConnection {
    pub fn new(cx: Context, groups: &HashMap<BackendId, FrontendGroup>) -> Self {
        let (systems, commands, mut messages) = Systems::new().expect("Failed to start backend");
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
        });
        let mut result = Self {
            systems,
            commands,
            receiver: Some(receiver),
            sent: HashMap::new(),
            socket: None,
            status,
        };
        result.sync_config(groups);
        result
    }

    pub fn sync_config(&mut self, groups: &HashMap<BackendId, FrontendGroup>) {
        for group in groups.values() {
            let values = match group {
                FrontendGroup::Sysinfo(view) => {
                    let config = view.state.lock().config.clone();
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

impl Drop for LocalConnection {
    fn drop(&mut self) {
        self.systems.shutdown();
        if let Some(receiver) = self.receiver.take() {
            let _ = receiver.join();
        }
    }
}
