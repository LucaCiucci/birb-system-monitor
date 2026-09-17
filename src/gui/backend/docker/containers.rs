use std::sync::Arc;

use egui::{Color32, Grid, Ui, WidgetText, mutex::Mutex};

use crate::gui::{BackendPanel, backend::docker::DockerSharedState};

pub struct ContainersPanel {
    state: Arc<Mutex<DockerSharedState>>,
}

impl ContainersPanel {
    pub fn new(state: Arc<Mutex<DockerSharedState>>) -> Self {
        Self { state }
    }

    fn data(&self) -> egui::mutex::MutexGuard<'_, DockerSharedState> {
        self.state.lock()
    }
}

impl BackendPanel for ContainersPanel {
    fn title(&mut self) -> WidgetText {
        "Containers".into()
    }

    fn ui(&mut self, ui: &mut Ui) {
        let data = self.data();

        if let Some(ref err) = data.state.containers_error {
            ui.colored_label(Color32::LIGHT_RED, format!("⚠ {err}"));
            return;
        }

        if !data.state.connected {
            ui.spinner();
            ui.label("Connecting to Docker...");
            return;
        }

        if data.state.containers.is_empty() {
            ui.label("No containers found.");
            return;
        }

        egui::ScrollArea::both().show(ui, |ui| {
            Grid::new("docker_containers")
                .striped(true)
                .min_col_width(60.0)
                .show(ui, |ui| {
                    // Headers
                    ui.strong("ID");
                    ui.strong("Name");
                    ui.strong("Image");
                    ui.strong("Status");
                    ui.strong("State");
                    ui.strong("Ports");
                    ui.end_row();

                    for container in &data.state.containers {
                        let short_id = if container.id.len() >= 12 {
                            &container.id[..12]
                        } else {
                            &container.id
                        };

                        let state_color = match container.state.as_str() {
                            "running" => Color32::GREEN,
                            "exited" | "dead" => Color32::RED,
                            "paused" => Color32::YELLOW,
                            _ => Color32::WHITE,
                        };

                        ui.label(short_id);
                        ui.label(&container.name);
                        ui.label(&container.image);
                        ui.label(&container.status);
                        ui.colored_label(state_color, &container.state);
                        ui.label(&container.ports);
                        ui.end_row();
                    }
                });
        });
    }

    fn scroll_bars(&self) -> [bool; 2] {
        [true, true]
    }
}
