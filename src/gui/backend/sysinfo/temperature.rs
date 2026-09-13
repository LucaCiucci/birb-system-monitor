use std::{sync::Arc, time::Duration};

use egui::{Color32, Grid, ProgressBar, WidgetText, mutex::Mutex};

use crate::gui::{BackendPanel, backend::sysinfo::SysinfoSharedState};

pub(super) struct TemperaturePanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl TemperaturePanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for TemperaturePanel {
    fn title(&mut self) -> WidgetText {
        "Temperatures".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        let Some(latest) = data.data.last() else {
            ui.label("Loading...");
            return;
        };

        let components = &latest.component_stats.components;

        if components.is_empty() {
            ui.label("No temperature sensors detected.");
            ui.label("(May require additional kernel modules or hardware support.)");
            return;
        }

        ui.horizontal(|ui| {
            ui.label(format!("{} sensors", components.len()));
            ui.separator();
            let avg_temp = components.iter().filter_map(|c| c.temperature).sum::<f32>()
                / components
                    .iter()
                    .filter(|c| c.temperature.is_some())
                    .count()
                    .max(1) as f32;
            if avg_temp > 0.0 {
                ui.label(format!("Average: {avg_temp:.0}°C"));
            }
        });

        ui.separator();

        Grid::new("temperature_grid")
            .num_columns(3)
            .striped(true)
            .spacing([16.0, 6.0])
            .show(ui, |ui| {
                ui.strong("Component");
                ui.strong("Temperature");
                ui.strong("Max / Critical");
                ui.end_row();

                for component in components {
                    ui.label(&component.label);

                    // Temperature with color-coded bar
                    if let Some(temp) = component.temperature {
                        let critical = component.critical.unwrap_or(100.0);
                        let fraction = (temp / critical).clamp(0.0, 1.0) as f32;
                        let color = temp_color(temp, component.critical);
                        ui.horizontal(|ui| {
                            ui.add(
                                ProgressBar::new(fraction)
                                    .desired_width(80.0)
                                    .fill(color)
                                    .text(format!("{temp:.1}°C")),
                            );
                        });
                    } else {
                        ui.label("—");
                    }

                    // Max / Critical
                    let max_str = component
                        .max
                        .map(|m| format!("{m:.0}°C"))
                        .unwrap_or_else(|| "—".into());
                    let crit_str = component
                        .critical
                        .map(|c| format!("{c:.0}°C"))
                        .unwrap_or_else(|| "—".into());
                    ui.label(format!("{max_str} / {crit_str}"));

                    ui.end_row();
                }
            });

        // Settings
        ui.collapsing("Settings", |ui| {
            let mut config = data.config.clone();
            ui.horizontal(|ui| {
                ui.label("Update interval:");
                let mut interval_secs = config.update_interval.as_secs_f32();
                if ui
                    .add(egui::Slider::new(&mut interval_secs, 0.05..=5.0).logarithmic(true))
                    .changed()
                {
                    config.update_interval = Duration::from_secs_f32(interval_secs);
                }
            });
            if config != data.config {
                drop(data);
                self.state.lock().config = config;
            }
        });
    }
}

fn temp_color(temperature: f32, critical: Option<f32>) -> Color32 {
    let threshold = critical.unwrap_or(100.0);
    let ratio = temperature / threshold;
    if ratio > 0.9 {
        Color32::from_rgb(244, 67, 54) // red (critical)
    } else if ratio > 0.7 {
        Color32::from_rgb(255, 193, 7) // yellow (warm)
    } else if ratio > 0.4 {
        Color32::from_rgb(100, 181, 246) // blue (moderate)
    } else {
        Color32::from_rgb(76, 175, 80) // green (cool)
    }
}
