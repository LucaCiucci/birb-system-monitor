use std::{sync::Arc, time::SystemTime};

use birb_monitor::backend::docker::{DockerMessage, SimpleContainer, SimpleImage};
use egui::{WidgetText, mutex::Mutex};
use serde::{Deserialize, Serialize};

use crate::gui::{
    BackendPanel, BackendPanelId, BackendPanelInfo,
    backend::docker::{containers::ContainersPanel, images::ImagesPanel},
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
    pub(super) state: Arc<Mutex<DockerSharedState>>,
}

impl DockerFrontend {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(DockerSharedState::new())),
        }
    }
}

impl DockerFrontend {
    pub fn name(&self) -> WidgetText {
        "Docker".into()
    }

    pub fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.state.lock().config)?)
    }

    pub fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.state.lock().config = serde_json::from_value(config.clone())?;
        Ok(())
    }

    pub fn panels(&self) -> Vec<BackendPanelInfo> {
        vec![
            BackendPanelInfo {
                id: BackendPanelId("containers".into()),
                title: "Containers".into(),
                description: "Lists running containers".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("images".into()),
                title: "Images".into(),
                description: "Lists docker images".into(),
            },
        ]
    }

    pub fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
        match panel_id.0.as_str() {
            "containers" => Box::new(ContainersPanel::new(self.state.clone())),
            "images" => Box::new(ImagesPanel::new(self.state.clone())),
            _ => panic!("Unknown panel id: {}", panel_id.0),
        }
    }
}

pub(super) struct DockerSharedState {
    pub config: DockerConfig,
    pub state: DockerState,
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
