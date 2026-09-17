use std::sync::Arc;

use egui::{Color32, WidgetText, mutex::Mutex};
use egui_plot::{AxisHints, Corner, Legend, Line, Plot, PlotPoints};
use human_units::FormatSize;

use crate::gui::{
    BackendPanel,
    backend::sysinfo::{SnapshotData, SysinfoSharedState},
};

const MIN_TIME_SECONDS: f64 = 0.0;

pub(super) struct DiskIoPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl DiskIoPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for DiskIoPanel {
    fn title(&mut self) -> WidgetText {
        "Disk I/O".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        if let Some(data_latest) = data.data.last() {
            // Current totals
            ui.horizontal(|ui| {
                ui.label(format!(
                    "Read: {}",
                    data_latest.disk_io_stats.total_read_bytes.format_size()
                ));
                ui.separator();
                ui.label(format!(
                    "Written: {}",
                    data_latest.disk_io_stats.total_written_bytes.format_size()
                ));
            });

            let plot_height = ui.available_height().clamp(100.0, 600.0);
            let min_window = data.config.min_plot_window_secs();
            disk_io_plot(ui, &data.data, plot_height, min_window);
        } else {
            ui.label("Loading...");
        }
    }
}

fn disk_io_plot(ui: &mut egui::Ui, snapshots: &[SnapshotData], plot_height: f32, min_window: f64) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest, min_window);

    // Compute rates (bytes per second) from cumulative totals
    let rate_points: Vec<([f64; 2], [f64; 2])> = snapshots
        .windows(2)
        .map(|pair| {
            let prev = &pair[0];
            let curr = &pair[1];
            let dt = curr
                .captured_at
                .duration_since(prev.captured_at)
                .unwrap()
                .as_secs_f64()
                .max(0.001);
            let read_rate = (curr
                .disk_io_stats
                .total_read_bytes
                .saturating_sub(prev.disk_io_stats.total_read_bytes))
                as f64
                / dt;
            let write_rate = (curr
                .disk_io_stats
                .total_written_bytes
                .saturating_sub(prev.disk_io_stats.total_written_bytes))
                as f64
                / dt;
            let seconds_ago = latest
                .captured_at
                .duration_since(curr.captured_at)
                .unwrap()
                .as_secs_f64();
            ([seconds_ago, read_rate], [seconds_ago, write_rate])
        })
        .collect();

    let read_points: PlotPoints = rate_points.iter().map(|(r, _)| *r).collect();
    let write_points: PlotPoints = rate_points.iter().map(|(_, w)| *w).collect();

    // Determine y-axis range
    let max_rate = rate_points
        .iter()
        .flat_map(|(r, w)| [r[1], w[1]])
        .fold(0.0_f64, f64::max)
        .max(1.0);

    Plot::new("sysinfo_disk_io_plot")
        .height(plot_height)
        .invert_x(true)
        .default_x_bounds(MIN_TIME_SECONDS, max_time_seconds)
        .default_y_bounds(0.0, max_rate * 1.1)
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
            AxisHints::new_y().formatter(|mark, _| format_bytes_per_sec(mark.value)),
        ])
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time_seconds);
            plot_ui.set_plot_bounds_y(0.0..=(max_rate * 1.1));

            plot_ui.line(
                Line::new("Read", read_points)
                    .color(Color32::from_rgb(255, 183, 77))
                    .width(1.5),
            );
            plot_ui.line(
                Line::new("Write", write_points)
                    .color(Color32::from_rgb(244, 67, 54))
                    .width(1.5),
            );
        });
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

fn format_bytes_per_sec(value: f64) -> String {
    if value >= 1_000_000_000.0 {
        format!("{:.1} GB/s", value / 1_000_000_000.0)
    } else if value >= 1_000_000.0 {
        format!("{:.1} MB/s", value / 1_000_000.0)
    } else if value >= 1_000.0 {
        format!("{:.1} KB/s", value / 1_000.0)
    } else {
        format!("{:.0} B/s", value)
    }
}
