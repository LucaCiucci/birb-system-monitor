use std::{collections::HashMap, collections::HashSet, sync::Arc};

use egui::{Color32, ProgressBar, Stroke, Ui, WidgetText, mutex::Mutex};
use egui_extras::{Column, TableBuilder};
use egui_plot::{Corner, FilledArea, Legend, Line, Plot, PlotPoints};
use human_units::FormatSize;
use sysinfo::Pid;

use crate::gui::{
    backend::sysinfo::{ProcessInfo, SnapshotData, SysinfoSharedState},
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
        let cols = ((available_width / 320.0).floor() as usize).max(1).min(2); // Max 2 columns

        // -- Summary grid (CPU, Memory, Swap) --
        egui::Grid::new("dashboard_summary")
            .num_columns(3)
            .striped(true)
            .spacing([16.0, 4.0])
            .show(ui, |ui| {
                // CPU row
                ui.label(format!("CPU: {:.1}%", latest.cpu_stats.global_usage));
                ui.add(
                    ProgressBar::new((latest.cpu_stats.global_usage / 100.0).clamp(0.0, 1.0))
                        .desired_width(120.0)
                        .text(format!("{:.1}%", latest.cpu_stats.global_usage)),
                );
                ui.end_row();

                // Memory row
                ui.label(format!(
                    "Memory: {} / {}",
                    latest.general_stats.used_memory.format_size(),
                    latest.general_stats.total_memory.format_size(),
                ));
                let memory_percent = percent(latest.general_stats.used_memory, latest.general_stats.total_memory);
                ui.add(
                    ProgressBar::new(memory_percent as f32 / 100.0)
                        .desired_width(120.0)
                        .text(format!("{:.1}%", memory_percent)),
                );
                ui.end_row();

                // Swap row
                ui.label(format!(
                    "Swap: {} / {}",
                    latest.general_stats.used_swap.format_size(),
                    latest.general_stats.total_swap.format_size(),
                ));
                let swap_percent = percent(latest.general_stats.used_swap, latest.general_stats.total_swap);
                ui.add(
                    ProgressBar::new(swap_percent as f32 / 100.0)
                        .desired_width(120.0)
                        .text(format!("{:.1}%", swap_percent)),
                );
                ui.end_row();
            });

        ui.separator();

        // Compute dynamic height for mini plots based on remaining space.
        // Remaining items after the plots: separator + per-core bars (~40px total).
        let plot_rows = if cols >= 2 { 3 } else { 5 };
        let plot_area = (ui.available_height() - 40.0).max(0.0);
        let mini_plot_height = (plot_area / plot_rows as f32 - 20.0).clamp(80.0, 250.0);
        let min_window = data.config.min_plot_window_secs();

        // -- Responsive grid of mini graphs (equal-width columns via Column::remainder) --
        let row_height = mini_plot_height + 20.0; // plot + label
        TableBuilder::new(ui)
            .columns(Column::remainder(), cols)
            .striped(false)
            .cell_layout(egui::Layout::top_down_justified(egui::Align::LEFT))
            .body(|mut body| {
                // Row 1
                body.row(row_height, |mut row| {
                    row.col(|ui| {
                        mini_cpu_plot(ui, latest, &data.data, &data.process_info, &data.process_selection.selected_processes, mini_plot_height, min_window);
                    });
                    if cols >= 2 {
                        row.col(|ui| {
                            mini_memory_plot(ui, latest, &data.data, &data.process_info, &data.process_selection.selected_processes, mini_plot_height, min_window);
                        });
                    }
                });
                // Row 2
                body.row(row_height, |mut row| {
                    row.col(|ui| {
                        mini_network_plot(ui, latest, &data.data, mini_plot_height, min_window);
                    });
                    if cols >= 2 {
                        row.col(|ui| {
                            mini_disk_io_plot(ui, latest, &data.data, mini_plot_height, min_window);
                        });
                    }
                });
                // Row 3: Temperature chart
                body.row(row_height, |mut row| {
                    row.col(|ui| {
                        mini_temperature_chart(ui, latest, &data.data, mini_plot_height, min_window);
                    });
                    if cols >= 2 {
                        row.col(|_ui| {});
                    }
                });
            });

        ui.separator();

        // Compact per-core bars in a grid
        let total_cores = latest.cpu_stats.per_cpu_usage.len();
        let core_cols = total_cores.min(8); // up to 8 per row
        egui::Grid::new("dashboard_cores")
            .num_columns(core_cols)
            .spacing([6.0, 4.0])
            .show(ui, |ui| {
                for (i, usage) in latest.cpu_stats.per_cpu_usage.iter().enumerate() {
                    let color = cpu_bar_color(*usage);
                    ui.add(
                        ProgressBar::new((*usage / 100.0).clamp(0.0, 1.0))
                            .desired_width(60.0)
                            .fill(color)
                            .text(format!("CPU{i} {usage:.0}%")),
                    );
                    if (i + 1) % core_cols == 0 {
                        ui.end_row();
                    }
                }
            });

    }
}

// ── CPU mini plot ──

fn mini_cpu_plot(
    ui: &mut Ui,
    latest: &SnapshotData,
    snapshots: &[SnapshotData],
    process_info: &HashMap<Pid, ProcessInfo>,
    selected_processes: &HashSet<Pid>,
    plot_height: f32,
    min_window: f64,
) {
    ui.vertical(|ui| {
        ui.label("CPU");
        let max_time = max_time_seconds(snapshots, latest, min_window);

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

        let selected_points = selected_cpu_points(snapshots, latest, selected_processes, process_info);

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
    process_info: &HashMap<Pid, ProcessInfo>,
    selected_processes: &HashSet<Pid>,
    plot_height: f32,
    min_window: f64,
) {
    ui.vertical(|ui| {
        ui.label("Memory");
        let max_time = max_time_seconds(snapshots, latest, min_window);

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
        let selected_points = selected_memory_points(snapshots, latest, selected_processes, process_info);

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

fn mini_network_plot(ui: &mut Ui, latest: &SnapshotData, snapshots: &[SnapshotData], plot_height: f32, min_window: f64) {
    ui.vertical(|ui| {
        ui.label("Network");
        let max_time = max_time_seconds(snapshots, latest, min_window);

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

fn mini_disk_io_plot(ui: &mut Ui, latest: &SnapshotData, snapshots: &[SnapshotData], plot_height: f32, min_window: f64) {
    ui.vertical(|ui| {
        ui.label("Disk I/O");
        let max_time = max_time_seconds(snapshots, latest, min_window);

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

// ── Temperature mini chart ──

fn mini_temperature_chart(ui: &mut Ui, latest: &SnapshotData, snapshots: &[SnapshotData], plot_height: f32, min_window: f64) {
    if latest.component_stats.components.is_empty() {
        ui.vertical(|ui| {
            ui.label("Temperature");
            ui.label("No sensors");
        });
        return;
    }

    ui.vertical(|ui| {
        ui.label("Temperature");
        let max_time = max_time_seconds(snapshots, latest, min_window);

        // Compute y range
        let mut max_temp = 0.0_f64;
        for snapshot in snapshots {
            for c in &snapshot.component_stats.components {
                if let Some(t) = c.temperature {
                    max_temp = max_temp.max(t as f64);
                }
            }
        }
        let y_max = (max_temp * 1.15).max(50.0);

        Plot::new("dash_temperature")
            .height(plot_height)
            .invert_x(true)
            .default_x_bounds(MIN_TIME_SECONDS, max_time)
            .default_y_bounds(0.0, y_max)
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
                plot_ui.set_plot_bounds_y(0.0..=y_max);

                for (i, component) in latest.component_stats.components.iter().enumerate() {
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
                                .find(|c| c.label == component.label)
                                .and_then(|c| c.temperature)
                                .unwrap_or(0.0) as f64;
                            [seconds_ago, temp]
                        })
                        .collect();

                    let truncated = truncate_label(&component.label);
                    plot_ui.line(
                        Line::new(truncated, points)
                            .color(temp_line_color(i))
                            .width(1.5),
                    );
                }
            });
    });
}

fn truncate_label(label: &str) -> String {
    const MAX_LEN: usize = 22;
    if label.len() > MAX_LEN {
        let mut s = label.chars().take(MAX_LEN.saturating_sub(1)).collect::<String>();
        s.push('…');
        s
    } else {
        label.to_string()
    }
}

// ── Helper functions ──

fn max_time_seconds(snapshots: &[SnapshotData], latest: &SnapshotData, min_window: f64) -> f64 {
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
        .max(min_window)
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
    process_info: &HashMap<Pid, ProcessInfo>,
) -> PlotPoints<'static> {
    snapshots
        .iter()
        .enumerate()
        .map(|(i, snapshot)| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            let selected_usage = selected_processes
                .iter()
                .filter_map(|pid| process_info.get(pid))
                .map(|info| info.metrics.get(i).map(|m| m.cpu_usage as f64).unwrap_or(0.0))
                .sum::<f64>();
            [seconds_ago, clamp_percent(selected_usage)]
        })
        .collect()
}

fn selected_memory_points(
    snapshots: &[SnapshotData],
    latest: &SnapshotData,
    selected_processes: &HashSet<Pid>,
    process_info: &HashMap<Pid, ProcessInfo>,
) -> PlotPoints<'static> {
    snapshots
        .iter()
        .enumerate()
        .map(|(i, snapshot)| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            let selected_memory = selected_processes
                .iter()
                .filter_map(|pid| process_info.get(pid))
                .map(|info| info.metrics.get(i).map(|m| m.memory).unwrap_or(0))
                .sum::<u64>();
            [seconds_ago, clamp_percent(percent(selected_memory, snapshot.general_stats.total_memory))]
        })
        .collect()
}

fn temp_line_color(index: usize) -> Color32 {
    const COLORS: [Color32; 10] = [
        Color32::from_rgb(244, 67, 54),
        Color32::from_rgb(255, 152, 0),
        Color32::from_rgb(255, 235, 59),
        Color32::from_rgb(76, 175, 80),
        Color32::from_rgb(33, 150, 243),
        Color32::from_rgb(156, 39, 176),
        Color32::from_rgb(0, 188, 212),
        Color32::from_rgb(233, 30, 99),
        Color32::from_rgb(96, 125, 139),
        Color32::from_rgb(121, 85, 72),
    ];
    COLORS[index % COLORS.len()]
}
