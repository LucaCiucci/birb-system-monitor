use crate::backend::sysinfo::ComponentsSnapshot as SnapshotData;
use std::collections::BTreeSet;
use std::sync::Arc;

use egui::{Color32, WidgetText, mutex::Mutex};
use egui_plot::{AxisHints, Corner, Legend, Line, Plot, PlotPoints};
use serde::{Deserialize, Serialize};

use crate::gui::{Panel, backend::sysinfo::SysinfoSharedState};

const MIN_TIME_SECONDS: f64 = 0.0;
const MAX_LEGEND_LEN: usize = 22;

pub(super) struct TemperatureChartPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
    config: Config,
}

#[derive(Clone, Serialize, Deserialize)]
struct Config {
    enabled: BTreeSet<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: BTreeSet::new(),
        }
    }
}

impl TemperatureChartPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            state,
            config: Config::default(),
        }
    }
}

impl Panel for TemperatureChartPanel {
    fn title(&mut self) -> WidgetText {
        "Temperature Chart".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        let Some(latest) = data.temperatures.last() else {
            ui.label("Loading...");
            return;
        };

        if latest.component_stats.components.is_empty() {
            ui.label("No temperature sensors detected.");
            return;
        }

        // Populate default selection on first run
        if self.config.enabled.is_empty() {
            self.config.enabled = latest
                .component_stats
                .components
                .iter()
                .map(|c| c.label.clone())
                .collect();
        }

        // Filter to only labels that still exist
        let available: BTreeSet<String> = latest
            .component_stats
            .components
            .iter()
            .map(|c| c.label.clone())
            .collect();
        self.config.enabled.retain(|l| available.contains(l));

        // Collapsible sensor selector
        ui.collapsing("Sensors", |ui| {
            let changed = ui
                .checkbox(&mut false, &format!("{} sensors", available.len()))
                .changed();
            // The all/none toggle is a bit tricky with checkbox, let's just show per-item
            // Actually let's just check if changed
            let _ = changed;

            let mut any_change = false;
            for label in &available {
                let mut checked = self.config.enabled.contains(label);
                let trunc = truncate_label(label);
                if ui.checkbox(&mut checked, &trunc).changed() {
                    if checked {
                        self.config.enabled.insert(label.clone());
                    } else {
                        self.config.enabled.remove(label);
                    }
                    any_change = true;
                }
            }
            if any_change {
                // If nothing is selected, re-select all
                if self.config.enabled.is_empty() {
                    self.config.enabled = available.clone();
                }
            }
        });

        let plot_height = ui.available_height().clamp(100.0, 600.0);
        let min_window =
            data.config.temperature_interval.as_secs_f64() * data.config.max_readings as f64;
        temperature_plot(
            ui,
            &data.temperatures,
            &self.config.enabled,
            plot_height,
            min_window,
        );
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.config)?)
    }

    fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.config = serde_json::from_value(config.clone())?;
        Ok(())
    }
}

fn temperature_plot(
    ui: &mut egui::Ui,
    snapshots: &[SnapshotData],
    enabled: &BTreeSet<String>,
    plot_height: f32,
    min_window: f64,
) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest, min_window);

    // Collect only enabled labels
    let labels: Vec<&str> = latest
        .component_stats
        .components
        .iter()
        .filter(|c| enabled.contains(&c.label))
        .map(|c| c.label.as_str())
        .collect();

    if labels.is_empty() {
        ui.label("No sensors selected. Open the Sensors section above to enable some.");
        return;
    }

    // Determine y-axis range from enabled components only
    let mut max_temp = 0.0_f64;
    let mut min_temp = f64::MAX;
    for snapshot in snapshots {
        for component in &snapshot.component_stats.components {
            if !enabled.contains(&component.label) {
                continue;
            }
            if let Some(temp) = component.temperature {
                let t = temp as f64;
                if t > max_temp {
                    max_temp = t;
                }
                if t < min_temp {
                    min_temp = t;
                }
            }
        }
    }
    if max_temp <= 0.0 {
        max_temp = 100.0;
    }
    if min_temp == f64::MAX {
        min_temp = 0.0;
    }
    let y_margin = ((max_temp - min_temp) * 0.1).max(5.0);
    let y_min = (min_temp - y_margin).min(0.0).max(0.0);
    let y_max = max_temp + y_margin;

    let plot = Plot::new("sysinfo_temperature_chart")
        .height(plot_height)
        .invert_x(true)
        .default_x_bounds(MIN_TIME_SECONDS, max_time_seconds)
        .default_y_bounds(y_min, y_max)
        .auto_bounds(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .legend(Legend::default().position(Corner::LeftTop))
        .custom_x_axes(vec![
            AxisHints::new_x().formatter(|mark, _| format_seconds_ago(mark.value)),
        ])
        .custom_y_axes(vec![
            AxisHints::new_y().formatter(|mark, _| format!("{:.0}°C", mark.value)),
        ]);

    plot.show(ui, |plot_ui| {
        plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time_seconds);
        plot_ui.set_plot_bounds_y(y_min..=y_max);

        for (i, label) in labels.iter().enumerate() {
            let points: PlotPoints = snapshots
                .iter()
                .map(|snapshot| {
                    let seconds_ago = latest
                        .captured_at
                        .duration_since(snapshot.captured_at)
                        .unwrap()
                        .as_secs_f64();
                    let temp = snapshot
                        .component_stats
                        .components
                        .iter()
                        .find(|c| c.label == *label)
                        .and_then(|c| c.temperature)
                        .unwrap_or(0.0) as f64;
                    [seconds_ago, temp]
                })
                .collect();

            let legend_label = truncate_label(label);
            plot_ui.line(
                Line::new(legend_label, points)
                    .color(temp_line_color(i))
                    .width(1.5),
            );
        }
    });
}

fn truncate_label(label: &str) -> String {
    if label.len() > MAX_LEGEND_LEN {
        let mut s = label
            .chars()
            .take(MAX_LEGEND_LEN.saturating_sub(1))
            .collect::<String>();
        s.push('…');
        s
    } else {
        label.to_string()
    }
}

fn max_time_seconds(snapshots: &[SnapshotData], latest: &SnapshotData, min_window: f64) -> f64 {
    snapshots
        .first()
        .map(|oldest| {
            latest
                .captured_at
                .duration_since(oldest.captured_at)
                .unwrap()
                .as_secs_f64()
                .max(1.0)
        })
        .unwrap_or(1.0)
        .max(min_window)
}

fn format_seconds_ago(seconds_ago: f64) -> String {
    if seconds_ago == 0.0 {
        "now".into()
    } else {
        format!("{:.0}s", seconds_ago)
    }
}

fn temp_line_color(index: usize) -> Color32 {
    const COLORS: [Color32; 10] = [
        Color32::from_rgb(244, 67, 54),  // red
        Color32::from_rgb(255, 152, 0),  // orange
        Color32::from_rgb(255, 235, 59), // yellow
        Color32::from_rgb(76, 175, 80),  // green
        Color32::from_rgb(33, 150, 243), // blue
        Color32::from_rgb(156, 39, 176), // purple
        Color32::from_rgb(0, 188, 212),  // cyan
        Color32::from_rgb(233, 30, 99),  // pink
        Color32::from_rgb(96, 125, 139), // bluegrey
        Color32::from_rgb(121, 85, 72),  // brown
    ];
    COLORS[index % COLORS.len()]
}
