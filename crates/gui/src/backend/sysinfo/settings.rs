use std::{sync::Arc, time::Duration};

use egui::{Grid, WidgetText, mutex::Mutex};

use crate::{backend::sysinfo::SysinfoSharedState, BackendPanel};

pub(super) struct SettingsPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl SettingsPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for SettingsPanel {
    fn title(&mut self) -> WidgetText {
        "Sysinfo Settings".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        ui.heading("Sysinfo Backend Settings");
        ui.separator();

        let config = data.config.clone();
        let mut interval_secs = config.update_interval.as_secs_f32();

        Grid::new("sysinfo_settings_grid")
            .num_columns(2)
            .striped(true)
            .spacing([16.0, 8.0])
            .show(ui, |ui| {
                // Update interval
                ui.label("Update interval:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut interval_secs, 0.05..=5.0)
                            .logarithmic(true)
                            .suffix(" s"),
                    );
                });
                ui.end_row();

                // Current value display
                ui.label("Current interval:");
                ui.label(format!("{:.2} s", interval_secs));
                ui.end_row();
            });

        let new_config = crate::backend::sysinfo::SysinfoConfig {
            update_interval: Duration::from_secs_f32(interval_secs),
        };

        if new_config != config {
            drop(data);
            self.state.lock().config = new_config;
        }

        ui.separator();
        ui.label("These settings apply to all sysinfo panels (CPU, Memory, Network, Disk I/O).");
    }
}
