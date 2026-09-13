use std::{collections::HashMap, collections::HashSet, sync::Arc};

use egui::{Color32, Grid, ProgressBar, WidgetText, mutex::Mutex};
use egui_plot::{AxisHints, Corner, Legend, Line, Plot, PlotPoints};
use human_units::FormatSize;
use sysinfo::Pid;

use crate::gui::{
    backend::sysinfo::{ProcessInfo, SnapshotData, SysinfoSharedState},
    BackendPanel,
};

const MIN_USAGE_PERCENT: f64 = 0.0;
const MAX_USAGE_PERCENT: f64 = 100.0;
const MIN_TIME_SECONDS: f64 = 0.0;

pub(super) struct MemoryPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

impl MemoryPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for MemoryPanel {
    fn title(&mut self) -> WidgetText {
        "Memory".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        if let Some(data_latest) = data.data.last() {
            Grid::new("memory_overview")
                .num_columns(2)
                .striped(true)
                .spacing([16.0, 6.0])
                .show(ui, |ui| {
                    ui.label(format!(
                        "Memory: {} / {}",
                        data_latest.general_stats.used_memory.format_size(),
                        data_latest.general_stats.total_memory.format_size()
                    ));
                    let memory_percent = percent(
                        data_latest.general_stats.used_memory,
                        data_latest.general_stats.total_memory,
                    );
                    ui.add(
                        ProgressBar::new(memory_percent as f32 / 100.0)
                            .text(format!("Memory {:.1}%", memory_percent)),
                    );
                    ui.end_row();
                    ui.label(format!(
                        "Swap: {} / {}",
                        data_latest.general_stats.used_swap.format_size(),
                        data_latest.general_stats.total_swap.format_size()
                    ));
                    let swap_percent = percent(
                        data_latest.general_stats.used_swap,
                        data_latest.general_stats.total_swap,
                    );
                    ui.add(
                        ProgressBar::new(swap_percent as f32 / 100.0)
                            .text(format!("Swap {:.1}%", swap_percent)),
                    );
                    ui.end_row();
                });

            let plot_height = ui.available_height().clamp(100.0, 600.0);
            let min_window = data.config.min_plot_window_secs();
            memory_plot(ui, &data.data, &data.process_info, &data.process_selection.selected_processes, plot_height, min_window);
        } else {
            ui.label("Loading...");
        }

    }
}

fn memory_plot(ui: &mut egui::Ui, snapshots: &[SnapshotData], process_info: &HashMap<Pid, ProcessInfo>, selected_processes: &HashSet<Pid>, plot_height: f32, min_window: f64) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest, min_window);

    let memory_points: PlotPoints = snapshots
        .iter()
        .map(|snapshot| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            [
                seconds_ago,
                clamp_percent(percent(
                    snapshot.general_stats.used_memory,
                    snapshot.general_stats.total_memory,
                )),
            ]
        })
        .collect();
    let swap_points: PlotPoints = snapshots
        .iter()
        .map(|snapshot| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            [
                seconds_ago,
                clamp_percent(percent(
                    snapshot.general_stats.used_swap,
                    snapshot.general_stats.total_swap,
                )),
            ]
        })
        .collect();
    let selected_points = selected_memory_points(snapshots, latest, selected_processes, process_info);

    Plot::new("sysinfo_memory_plot")
        .height(plot_height)
        .invert_x(true)
        .default_x_bounds(MIN_TIME_SECONDS, max_time_seconds)
        .default_y_bounds(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
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
            AxisHints::new_y().formatter(|mark, _| format!("{:.0}%", mark.value))
        ])
        .show(ui, |plot_ui| {
            plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time_seconds);
            plot_ui.set_plot_bounds_y(MIN_USAGE_PERCENT..=MAX_USAGE_PERCENT);
            plot_ui.line(Line::new("Memory", memory_points).color(Color32::LIGHT_BLUE));
            plot_ui.line(Line::new("Swap", swap_points).color(Color32::LIGHT_GREEN));
            if !selected_processes.is_empty() {
                plot_ui.line(
                    Line::new("Selected", selected_points)
                        .color(Color32::YELLOW)
                        .width(3.0),
                );
            }
        });
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
            [
                seconds_ago,
                clamp_percent(percent(
                    selected_memory,
                    snapshot.general_stats.total_memory,
                )),
            ]
        })
        .collect()
}

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

fn percent(used: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        used as f64 / total as f64 * 100.0
    }
}

fn clamp_percent(value: f64) -> f64 {
    value.clamp(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
}

fn format_seconds_ago(seconds_ago: f64) -> String {
    if seconds_ago == 0.0 {
        "now".into()
    } else {
        format!("{:.0}s", seconds_ago)
    }
}
