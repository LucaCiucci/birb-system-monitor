use std::sync::Arc;

use egui::{Color32, WidgetText, mutex::Mutex};
use egui_plot::{AxisHints, Corner, Legend, Line, Plot, PlotPoints};

use crate::{
    backend::sysinfo::{SnapshotData, SysinfoSharedState},
    BackendPanel,
};

const MIN_TIME_SECONDS: f64 = 0.0;

pub(super) struct TemperatureChartPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl TemperatureChartPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for TemperatureChartPanel {
    fn title(&mut self) -> WidgetText {
        "Temperature Chart".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        let Some(latest) = data.data.last() else {
            ui.label("Loading...");
            return;
        };

        if latest.component_stats.components.is_empty() {
            ui.label("No temperature sensors detected.");
            return;
        }

        let plot_height = ui.available_height().clamp(100.0, 600.0);
        temperature_plot(ui, &data.data, plot_height);
    }
}

fn temperature_plot(ui: &mut egui::Ui, snapshots: &[SnapshotData], plot_height: f32) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest);

    // Build a line for each unique component label
    let labels: Vec<&str> = latest
        .component_stats
        .components
        .iter()
        .map(|c| c.label.as_str())
        .collect();

    // Determine y-axis range
    let mut max_temp = 0.0_f64;
    let mut min_temp = f64::MAX;
    for snapshot in snapshots {
        for component in &snapshot.component_stats.components {
            if let Some(temp) = component.temperature {
                let t = temp as f64;
                if t > max_temp { max_temp = t; }
                if t < min_temp { min_temp = t; }
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
            AxisHints::new_x().formatter(|mark, _| format_seconds_ago(mark.value))
        ])
        .custom_y_axes(vec![
            AxisHints::new_y().formatter(|mark, _| format!("{:.0}°C", mark.value))
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

            plot_ui.line(
                Line::new(*label, points)
                    .color(temp_line_color(i))
                    .width(1.5),
            );
        }
    });
}

fn max_time_seconds(snapshots: &[SnapshotData], latest: &SnapshotData) -> f64 {
    snapshots
        .first()
        .map(|oldest| {
            latest
                .captured_at
                .duration_since(oldest.captured_at)
                .as_secs_f64()
                .max(1.0)
        })
        .unwrap_or(1.0)
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
        Color32::from_rgb(244, 67, 54),    // red
        Color32::from_rgb(255, 152, 0),    // orange
        Color32::from_rgb(255, 235, 59),   // yellow
        Color32::from_rgb(76, 175, 80),    // green
        Color32::from_rgb(33, 150, 243),   // blue
        Color32::from_rgb(156, 39, 176),   // purple
        Color32::from_rgb(0, 188, 212),    // cyan
        Color32::from_rgb(233, 30, 99),    // pink
        Color32::from_rgb(96, 125, 139),   // bluegrey
        Color32::from_rgb(121, 85, 72),    // brown
    ];
    COLORS[index % COLORS.len()]
}
