use std::{
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use bollard::{
    Docker, API_DEFAULT_VERSION,
};
use egui::{mutex::Mutex, WidgetText};
use serde::{Deserialize, Serialize};

use crate::gui::{
    backend::docker::{
        containers::ContainersPanel,
        images::ImagesPanel,
    },
    Backend, BackendPanel, BackendPanelId, BackendPanelInfo,
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

#[derive(Debug, Clone)]
pub(super) struct SimpleContainer {
    pub id: String,
    pub name: String,
    pub image: String,
    pub status: String,
    pub state: String,
    pub created: i64,
    pub ports: String,
}

#[derive(Debug, Clone)]
pub(super) struct SimpleImage {
    pub id: String,
    pub repo_tags: Vec<String>,
    pub created: i64,
    pub size: i64,
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
            updater.join().expect("Failed to join docker updater thread");
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

impl Backend for DockerBackend {
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
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("Failed to build tokio runtime");

    rt.block_on(async move {
        loop {
                    let (socket_path, update_interval, should_stop) = {
                let data = state.lock();
                if data.should_stop {
                    return;
                }
                (
                    data.config.socket_path.clone(),
                    data.config.update_interval_secs,
                    data.should_stop,
                )
            };

            if should_stop {
                return;
            }

            // Reconnect every cycle (cheap for unix sockets)
            let docker = Docker::connect_with_socket(
                &socket_path,
                120,
                API_DEFAULT_VERSION,
            );

            match docker {
                Ok(docker) => {
                    let containers = list_containers(&docker).await;
                    let images = list_images(&docker).await;

                    let mut data = state.lock();
                    data.state.connected = true;
                    data.state.error = None;
                    data.state.last_updated = Some(Instant::now());

                    if let Ok(containers) = containers {
                        data.state.containers = containers;
                    } else if let Err(ref e) = containers {
                        data.state.error = Some(format!("Failed to list containers: {e}"));
                    }

                    if let Ok(images) = images {
                        data.state.images = images;
                    } else if let Err(ref e) = images {
                        data.state.error = Some(format!("Failed to list images: {e}"));
                    }

                    // Request repaint
                    data.cx.request_repaint();
                }
                Err(e) => {
                    let mut data = state.lock();
                    data.state.connected = false;
                    data.state.error = Some(format!("Failed to connect: {e}"));
                    data.cx.request_repaint();
                }
            }

            // Sleep for the update interval
            let sleep_duration = Duration::from_secs(update_interval.max(1));
            tokio::time::sleep(sleep_duration).await;
        }
    });
}

async fn list_containers(docker: &Docker) -> anyhow::Result<Vec<SimpleContainer>> {
    use bollard::query_parameters::ListContainersOptionsBuilder;

    let options = ListContainersOptionsBuilder::default()
        .all(true)
        .build();

    let containers = docker.list_containers(Some(options)).await?;

    Ok(containers
        .into_iter()
        .map(|c| {
            let names = c.names.unwrap_or_default();
            let name = names
                .first()
                .cloned()
                .unwrap_or_default()
                .trim_start_matches('/')
                .to_string();

            let ports_str = c
                .ports
                .unwrap_or_default()
                .iter()
                .map(|p| {
                    let typ = p.typ.map(|t| t.to_string()).unwrap_or_else(|| "tcp".into());
                    match p.public_port {
                        Some(pub_port) => format!("{}:{}->{}/{}", p.ip.as_deref().unwrap_or("0.0.0.0"), pub_port, p.private_port, typ),
                        None => format!("{}/{}", p.private_port, typ),
                    }
                })
                .collect::<Vec<_>>()
                .join(", ");

            SimpleContainer {
                id: c.id.unwrap_or_default().to_string(),
                name,
                image: c.image.unwrap_or_default(),
                status: c.status.unwrap_or_default(),
                state: c.state.map(|s| s.to_string()).unwrap_or_default(),
                created: c.created.unwrap_or(0),
                ports: ports_str,
            }
        })
        .collect())
}

async fn list_images(docker: &Docker) -> anyhow::Result<Vec<SimpleImage>> {
    use bollard::query_parameters::ListImagesOptionsBuilder;

    let options = ListImagesOptionsBuilder::default()
        .all(true)
        .build();

    let images = docker.list_images(Some(options)).await?;

    Ok(images
        .into_iter()
        .map(|i| SimpleImage {
            id: i.id,
            repo_tags: i.repo_tags,
            created: i.created,
            size: i.size,
        })
        .collect())
}
