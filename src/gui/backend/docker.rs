use std::time::SystemTime;

use crate::backend::docker::{DockerMessage, SimpleContainer, SimpleImage};
use egui::WidgetText;
use serde::{Deserialize, Serialize};

use crate::gui::{
    Panel, PanelInfo,
    backend::docker::{containers::ContainersPanel, images::ImagesPanel},
    panels::PanelId,
};

mod containers;
mod images;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct DockerConfig {
    /// Connection URI, e.g. "unix:///var/run/docker.sock"
    pub socket_path: String,
    /// Update interval in seconds
    pub update_interval_secs: u64,
}

impl Default for DockerConfig {
    fn default() -> Self {
        Self {
            socket_path: "unix:///var/run/docker.sock".into(),
            update_interval_secs: 2,
        }
    }
}

#[derive(Debug, Clone, Default)]
pub(super) struct DockerState {
    pub containers: Vec<SimpleContainer>,
    pub images: Vec<SimpleImage>,
    pub connected: bool,
    pub containers_error: Option<String>,
    pub images_error: Option<String>,
    pub last_updated: Option<SystemTime>,
}

pub struct DockerFrontend {
    pub(crate) state: DockerSharedState,
}

impl DockerFrontend {
    pub fn new() -> Self {
        Self {
            state: DockerSharedState::new(),
        }
    }
}

impl DockerFrontend {
    pub fn name(&self) -> WidgetText {
        "Docker".into()
    }

    pub fn panels(&self) -> Vec<PanelInfo> {
        vec![
            PanelInfo {
                id: PanelId::Containers,
                title: "Containers".into(),
                description: "Lists running containers".into(),
            },
            PanelInfo {
                id: PanelId::Images,
                title: "Images".into(),
                description: "Lists docker images".into(),
            },
        ]
    }

    pub fn new_panel(&self, panel_id: &PanelId) -> Box<dyn Panel> {
        match panel_id {
            PanelId::Containers => Box::new(ContainersPanel::new()),
            PanelId::Images => Box::new(ImagesPanel::new()),
            _ => panic!("Unknown panel id: {}", panel_id),
        }
    }
}

pub(crate) struct DockerSharedState {
    pub config: DockerConfig,
    pub(super) state: DockerState,
}

impl DockerSharedState {
    fn new() -> Self {
        Self {
            config: DockerConfig::default(),
            state: DockerState::default(),
        }
    }
}

impl DockerSharedState {
    pub(super) fn receive(&mut self, message: DockerMessage) {
        let DockerMessage::Snapshot(snapshot) = message;
        if snapshot.socket_path != self.config.socket_path {
            return;
        }
        self.state.connected = snapshot.containers.is_ok() || snapshot.images.is_ok();
        self.state.containers_error = match snapshot.containers {
            Ok(value) => {
                self.state.containers = value;
                None
            }
            Err(error) => Some(error),
        };
        self.state.images_error = match snapshot.images {
            Ok(value) => {
                self.state.images = value;
                None
            }
            Err(error) => Some(error),
        };
        self.state.last_updated = Some(snapshot.captured_at);
    }
}
