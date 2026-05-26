use std::{collections::HashSet, sync::Arc, time::Duration};

use egui::{Color32, ProgressBar, Stroke, Ui, WidgetText, mutex::Mutex};
use egui_plot::{Corner, FilledArea, Legend, Line, Plot, PlotPoints};
use human_units::FormatSize;
use sysinfo::Pid;

use crate::{
    backend::sysinfo::{SnapshotData, SysinfoSharedState},
    BackendPanel,
};

const MIN_USAGE_PERCENT: f64 = 0.0;
const MAX_USAGE_PERCENT: f64 = 100.0;
const MIN_TIME_SECONDS: f64 = 0.0;

pub(super) struct DashboardPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl DashboardPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for DashboardPanel {
    fn title(&mut self) -> WidgetText {
        "Dashboard".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        let Some(latest) = data.data.last() else {
            ui.label("Loading...");
            return;
        };

        // Determine number of columns based on available width
        let available_width = ui.available_width();
        let min_col_width = 320.0;
        let max_cols = (available_width / min_col_width).floor() as usize;
        let cols = max_cols.max(1).min(2); // Max 2 columns for readability

        // -- Summary bar --
        ui.horizontal(|ui| {
            ui.label(format!(
                "CPU: {:.1}%  |  Memory: {} / {} ({:.1}%)  |  Swap: {} / {} ({:.1}%)",
                latest.cpu_stats.global_usage,
                latest.general_stats.used_memory.format_size(),
                latest.general_stats.total_memory.format_size(),
                percent(latest.general_stats.used_memory, latest.general_stats.total_memory),
                latest.general_stats.used_swap.format_size(),
                latest.general_stats.total_swap.format_size(),
                percent(latest.general_stats.used_swap, latest.general_stats.total_swap),
            ));
        });

        ui.separator();

        // Compute dynamic height for mini plots based on available space.
        // The remaining elements after the grid are: separator + per-core bars (~35px) + settings.
        // Settings is collapsing, so when closed it's ~25px, when open it can grow.
        // We target filling available height without scrolling.
        let rows = if cols >= 2 { 2 } else { 4 };
        let remaining_after_grid = 70.0; // separator + core bars + settings header estimate
        let mini_plot_height = ((ui.available_height() - remaining_after_grid) / rows as f32)
            .clamp(80.0, 250.0);

        // -- Responsive grid of mini graphs --
        egui::Grid::new("dashboard_grid")
            .num_columns(cols)
            .min_col_width(min_col_width)
            .spacing([8.0, 8.0])
            .show(ui, |ui| {
                // CPU
                mini_cpu_plot(ui, latest, &data.data, &data.process_selection.selected_processes, mini_plot_height);
                if cols >= 2 { ui.end_row(); }

                // Memory
                mini_memory_plot(ui, latest, &data.data, &data.process_selection.selected_processes, mini_plot_height);
                if cols >= 2 { ui.end_row(); }

                // Network
                mini_network_plot(ui, latest, &data.data, mini_plot_height);
                if cols >= 2 { ui.end_row(); }

                // Disk I/O
                mini_disk_io_plot(ui, latest, &data.data, mini_plot_height);
                if cols >= 2 { ui.end_row(); }
            });

        ui.separator();

        // Compact per-core bars
        ui.horizontal_wrapped(|ui| {
            ui.label("Cores:");
            for (i, usage) in latest.cpu_stats.per_cpu_usage.iter().enumerate() {
                let color = cpu_bar_color(*usage);
                let bar = ProgressBar::new((*usage / 100.0).clamp(0.0, 1.0))
                    .desired_width(60.0)
                    .fill(color)
                    .text(format!("CPU{i} {usage:.0}%"));
                ui.add(bar);
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

// ── CPU mini plot ──

fn mini_cpu_plot(
    ui: &mut Ui,
    latest: &SnapshotData,
    snapshots: &[SnapshotData],
    selected_processes: &HashSet<Pid>,
    plot_height: f32,
) {
    ui.vertical(|ui| {
        ui.label("CPU");
        let max_time = max_time_seconds(snapshots, latest);

        let points: PlotPoints = snapshots
            .iter()
            .map(|snapshot| {
                let seconds_ago = latest
                    .captured_at
                    .duration_since(snapshot.captured_at)
                    .as_secs_f64();
                [seconds_ago, clamp_percent(snapshot.cpu_stats.global_usage as f64)]
            })
            .collect();

        let selected_points = selected_cpu_points(snapshots, latest, selected_processes);

        Plot::new("dash_cpu")
            .height(plot_height)
            .invert_x(true)
            .default_x_bounds(MIN_TIME_SECONDS, max_time)
            .default_y_bounds(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
            .auto_bounds(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_axes(false)
            .legend(Legend::default().position(Corner::LeftTop))
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time);
                plot_ui.set_plot_bounds_y(MIN_USAGE_PERCENT..=MAX_USAGE_PERCENT);

                let total_layer = total_cpu_layer(snapshots, latest);
                plot_ui.add(
                    FilledArea::new("Total", &total_layer.xs, &total_layer.lower, &total_layer.upper)
                        .fill_color(Color32::from_rgba_unmultiplied(100, 181, 246, 120))
                        .stroke(Stroke::new(1.0, Color32::LIGHT_BLUE)),
                );
                plot_ui.line(Line::new("Total", points).color(Color32::WHITE));
                if !selected_processes.is_empty() {
                    plot_ui.line(
                        Line::new("Selected", selected_points).color(Color32::YELLOW).width(2.0),
                    );
                }
            });
    });
}

// ── Memory mini plot ──

fn mini_memory_plot(
    ui: &mut Ui,
    latest: &SnapshotData,
    snapshots: &[SnapshotData],
    selected_processes: &HashSet<Pid>,
    plot_height: f32,
) {
    ui.vertical(|ui| {
        ui.label("Memory");
        let max_time = max_time_seconds(snapshots, latest);

        let memory_points: PlotPoints = snapshots
            .iter()
            .map(|snapshot| {
                let seconds_ago = latest
                    .captured_at
                    .duration_since(snapshot.captured_at)
                    .as_secs_f64();
                [seconds_ago, clamp_percent(percent(snapshot.general_stats.used_memory, snapshot.general_stats.total_memory))]
            })
            .collect();
        let swap_points: PlotPoints = snapshots
            .iter()
            .map(|snapshot| {
                let seconds_ago = latest
                    .captured_at
                    .duration_since(snapshot.captured_at)
                    .as_secs_f64();
                [seconds_ago, clamp_percent(percent(snapshot.general_stats.used_swap, snapshot.general_stats.total_swap))]
            })
            .collect();
        let selected_points = selected_memory_points(snapshots, latest, selected_processes);

        Plot::new("dash_memory")
            .height(plot_height)
            .invert_x(true)
            .default_x_bounds(MIN_TIME_SECONDS, max_time)
            .default_y_bounds(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
            .auto_bounds(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_axes(false)
            .legend(Legend::default().position(Corner::LeftTop))
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time);
                plot_ui.set_plot_bounds_y(MIN_USAGE_PERCENT..=MAX_USAGE_PERCENT);
                plot_ui.line(Line::new("Memory", memory_points).color(Color32::LIGHT_BLUE));
                plot_ui.line(Line::new("Swap", swap_points).color(Color32::LIGHT_GREEN));
                if !selected_processes.is_empty() {
                    plot_ui.line(
                        Line::new("Selected", selected_points).color(Color32::YELLOW).width(2.0),
                    );
                }
            });
    });
}

// ── Network mini plot ──

fn mini_network_plot(ui: &mut Ui, latest: &SnapshotData, snapshots: &[SnapshotData], plot_height: f32) {
    ui.vertical(|ui| {
        ui.label("Network");
        let max_time = max_time_seconds(snapshots, latest);

        let rate_points: Vec<([f64; 2], [f64; 2])> = snapshots
            .windows(2)
            .map(|pair| {
                let prev = &pair[0];
                let curr = &pair[1];
                let dt = curr.captured_at.duration_since(prev.captured_at).as_secs_f64().max(0.001);
                let rx = (curr.network_stats.total_received.saturating_sub(prev.network_stats.total_received)) as f64 / dt;
                let tx = (curr.network_stats.total_transmitted.saturating_sub(prev.network_stats.total_transmitted)) as f64 / dt;
                let seconds_ago = latest.captured_at.duration_since(curr.captured_at).as_secs_f64();
                ([seconds_ago, rx], [seconds_ago, tx])
            })
            .collect();

        let rx_points: PlotPoints = rate_points.iter().map(|(r, _)| *r).collect();
        let tx_points: PlotPoints = rate_points.iter().map(|(_, t)| *t).collect();

        let max_rate = rate_points.iter().flat_map(|(r, t)| [r[1], t[1]]).fold(0.0_f64, f64::max).max(1.0);

        Plot::new("dash_network")
            .height(plot_height)
            .invert_x(true)
            .default_x_bounds(MIN_TIME_SECONDS, max_time)
            .default_y_bounds(0.0, max_rate * 1.1)
            .auto_bounds(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_axes(false)
            .legend(Legend::default().position(Corner::LeftTop))
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time);
                plot_ui.set_plot_bounds_y(0.0..=(max_rate * 1.1));
                plot_ui.line(Line::new("RX", rx_points).color(Color32::from_rgb(76, 175, 80)).width(1.5));
                plot_ui.line(Line::new("TX", tx_points).color(Color32::from_rgb(66, 165, 245)).width(1.5));
            });
    });
}

// ── Disk I/O mini plot ──

fn mini_disk_io_plot(ui: &mut Ui, latest: &SnapshotData, snapshots: &[SnapshotData], plot_height: f32) {
    ui.vertical(|ui| {
        ui.label("Disk I/O");
        let max_time = max_time_seconds(snapshots, latest);

        let rate_points: Vec<([f64; 2], [f64; 2])> = snapshots
            .windows(2)
            .map(|pair| {
                let prev = &pair[0];
                let curr = &pair[1];
                let dt = curr.captured_at.duration_since(prev.captured_at).as_secs_f64().max(0.001);
                let read = (curr.disk_io_stats.total_read_bytes.saturating_sub(prev.disk_io_stats.total_read_bytes)) as f64 / dt;
                let write = (curr.disk_io_stats.total_written_bytes.saturating_sub(prev.disk_io_stats.total_written_bytes)) as f64 / dt;
                let seconds_ago = latest.captured_at.duration_since(curr.captured_at).as_secs_f64();
                ([seconds_ago, read], [seconds_ago, write])
            })
            .collect();

        let read_points: PlotPoints = rate_points.iter().map(|(r, _)| *r).collect();
        let write_points: PlotPoints = rate_points.iter().map(|(_, w)| *w).collect();

        let max_rate = rate_points.iter().flat_map(|(r, w)| [r[1], w[1]]).fold(0.0_f64, f64::max).max(1.0);

        Plot::new("dash_disk_io")
            .height(plot_height)
            .invert_x(true)
            .default_x_bounds(MIN_TIME_SECONDS, max_time)
            .default_y_bounds(0.0, max_rate * 1.1)
            .auto_bounds(false)
            .allow_drag(false)
            .allow_zoom(false)
            .allow_scroll(false)
            .allow_boxed_zoom(false)
            .allow_double_click_reset(false)
            .show_axes(false)
            .legend(Legend::default().position(Corner::LeftTop))
            .show(ui, |plot_ui| {
                plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time);
                plot_ui.set_plot_bounds_y(0.0..=(max_rate * 1.1));
                plot_ui.line(Line::new("Read", read_points).color(Color32::from_rgb(255, 183, 77)).width(1.5));
                plot_ui.line(Line::new("Write", write_points).color(Color32::from_rgb(244, 67, 54)).width(1.5));
            });
    });
}

// ── Helper functions ──

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

fn clamp_percent(value: f64) -> f64 {
    value.clamp(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
}

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

fn cpu_bar_color(usage: f32) -> Color32 {
    if usage > 80.0 {
        Color32::from_rgb(244, 67, 54)
    } else if usage > 50.0 {
        Color32::from_rgb(255, 193, 7)
    } else {
        Color32::from_rgb(76, 175, 80)
    }
}

// ── Reusable CPU layer helpers (adapted from cpu.rs) ──

struct CpuLayer {
    xs: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
}

fn total_cpu_layer(
    snapshots: &[SnapshotData],
    latest: &SnapshotData,
) -> CpuLayer {
    let mut layer = CpuLayer {
        xs: Vec::with_capacity(snapshots.len()),
        lower: Vec::with_capacity(snapshots.len()),
        upper: Vec::with_capacity(snapshots.len()),
    };

    for snapshot in snapshots {
        let seconds_ago = latest
            .captured_at
            .duration_since(snapshot.captured_at)
            .as_secs_f64();

        layer.xs.push(seconds_ago);
        layer.lower.push(MIN_USAGE_PERCENT);
        layer.upper.push(clamp_percent(snapshot.cpu_stats.global_usage as f64));
    }

    layer
}

fn selected_cpu_points(
    snapshots: &[SnapshotData],
    latest: &SnapshotData,
    selected_processes: &HashSet<Pid>,
) -> PlotPoints<'static> {
    snapshots
        .iter()
        .map(|snapshot| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            let selected_usage = selected_processes
                .iter()
                .filter_map(|pid| snapshot.processes.get(pid))
                .map(|process| process.cpu_usage as f64)
                .sum::<f64>();
            [seconds_ago, clamp_percent(selected_usage)]
        })
        .collect()
}

fn selected_memory_points(
    snapshots: &[SnapshotData],
    latest: &SnapshotData,
    selected_processes: &HashSet<Pid>,
) -> PlotPoints<'static> {
    snapshots
        .iter()
        .map(|snapshot| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            let selected_memory = selected_processes
                .iter()
                .filter_map(|pid| snapshot.processes.get(pid))
                .map(|process| process.memory)
                .sum::<u64>();
            [seconds_ago, clamp_percent(percent(selected_memory, snapshot.general_stats.total_memory))]
        })
        .collect()
}
