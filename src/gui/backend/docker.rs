use std::{
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use birb_monitor::backend::docker::{SimpleContainer, SimpleImage};
use egui::{WidgetText, mutex::Mutex};
use serde::{Deserialize, Serialize};

use crate::gui::{
    BackendOLD, BackendPanel, BackendPanelId, BackendPanelInfo,
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
    pub error: Option<String>,
    pub last_updated: Option<Instant>,
}

pub struct DockerBackend {
    state: Arc<Mutex<DockerSharedState>>,
    updater: Option<JoinHandle<()>>,
}

impl Drop for DockerBackend {
    fn drop(&mut self) {
        let mut data = self.state.lock();
        data.should_stop = true;
        drop(data);
        if let Some(updater) = self.updater.take() {
            updater
                .join()
                .expect("Failed to join docker updater thread");
        }
    }
}

impl DockerBackend {
    pub fn new(cx: egui::Context) -> Self {
        let state = DockerSharedState::new(cx);
        let state = Arc::new(Mutex::new(state));
        let updater = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || worker_thread(state))
        };
        Self {
            state,
            updater: Some(updater),
        }
    }
}

impl BackendOLD for DockerBackend {
    fn name(&self) -> WidgetText {
        "Docker".into()
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.state.lock().config)?)
    }

    fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.state.lock().config = serde_json::from_value(config.clone())?;
        Ok(())
    }

    fn panels(&self) -> Vec<BackendPanelInfo> {
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

    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
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
    cx: egui::Context,
    should_stop: bool,
}

impl DockerSharedState {
    fn new(cx: egui::Context) -> Self {
        Self {
            config: DockerConfig::default(),
            state: DockerState::default(),
            cx,
            should_stop: false,
        }
    }
}

fn worker_thread(state: Arc<Mutex<DockerSharedState>>) {
    // Reads now live in backend::docker::DockerHandler. Message consumption
    // will be wired into the frontend in a subsequent refactor.
    loop {
        if state.lock().should_stop {
            return;
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}
