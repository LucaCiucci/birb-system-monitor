use crate::gui::{
    Panel,
    backend::{FrontendConfig, docker, sysinfo},
    panels::PanelId,
};

/// Display data and preferences for both domains; collectors live in Systems.
pub struct FrontendState {
    pub sysinfo: sysinfo::SysinfoFrontend,
    pub docker: docker::DockerFrontend,
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
            sysinfo: self.sysinfo.state.config.clone(),
            docker: self.docker.state.config.clone(),
        }
    }

    pub fn apply_config(&mut self, config: &FrontendConfig) {
        self.sysinfo.state.config = config.sysinfo.clone();
        self.docker.state.config = config.docker.clone();
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
