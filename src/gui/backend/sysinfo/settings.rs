use std::time::Duration;

use egui::{Checkbox, Grid, WidgetText};

use crate::gui::{Panel, app::state::FrontendState, backend::sysinfo::SysinfoConfig};

pub(super) struct SettingsPanel;

impl SettingsPanel {
    pub(super) fn new() -> Self {
        Self
    }
}

impl Panel for SettingsPanel {
    fn title(&mut self) -> WidgetText {
        "Sysinfo Settings".into()
    }

    fn ui(&mut self, data: &mut FrontendState, ui: &mut egui::Ui) {
        let data = &mut data.sysinfo.state;

        ui.heading("Sysinfo Backend Settings");
        ui.separator();

        let config = data.config.clone();
        let mut interval_secs = config.update_interval.as_secs_f32();
        let mut readings = config.max_readings;
        let mut limit_processes_to_selection = config.limit_processes_to_selection;

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

                ui.label("Process collection:");
                ui.add(Checkbox::new(
                    &mut limit_processes_to_selection,
                    "Only retain details and history for selected processes",
                ))
                .on_hover_text(
                    "Only selected processes receive full details and metric history. With no selection, the process list keeps lightweight current data only. Disable this to retain them for all processes.");
                ui.end_row();

                // Readings
                ui.label("History / plot window:");
                ui.horizontal(|ui| {
                    ui.add(
                        egui::Slider::new(&mut readings, 0..=600)
                            .logarithmic(true)
                            .suffix(" readings"),
                    );
                    if readings == 0 {
                        ui.label("(full range)");
                    } else {
                        ui.label(format!("≈ {:.0}s", interval_secs * readings as f32));
                    }
                });
                ui.end_row();

                // Current interval display
                ui.label("Applied interval:");
                ui.label(
                    data.applied_update_interval
                        .map(|v| format!("{:.2} s", v.as_secs_f32()))
                        .unwrap_or_else(|| "Waiting for backend...".into()),
                );
                ui.end_row();
            });

        let new_config = SysinfoConfig {
            update_interval: Duration::from_secs_f32(interval_secs),
            max_readings: readings,
            temperature_interval: config.temperature_interval,
            limit_processes_to_selection,
        };

        if new_config != config {
            data.config = new_config;
        }

        ui.separator();
        ui.label("These settings apply to all sysinfo panels (CPU, Memory, Network, Disk I/O).");
    }
}
