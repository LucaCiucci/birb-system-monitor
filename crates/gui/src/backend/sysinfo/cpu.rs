use std::{collections::HashMap, collections::HashSet, sync::Arc};

use egui::{mutex::Mutex, Color32, Grid, ProgressBar, Stroke, WidgetText};
use egui_plot::{AxisHints, Corner, FilledArea, Legend, Line, Plot, PlotPoints};
use serde::{Deserialize, Serialize};
use sysinfo::Pid;

use crate::{backend::sysinfo::{ProcessInfo, SysinfoSharedState}, BackendPanel};

const MIN_USAGE_PERCENT: f64 = 0.0;
const MAX_USAGE_PERCENT: f64 = 100.0;
const MIN_TIME_SECONDS: f64 = 0.0;

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CpuPanelConfig {
    show_per_cpu: bool,
}

impl Default for CpuPanelConfig {
    fn default() -> Self {
        Self {
            show_per_cpu: true,
        }
    }
}

pub(super) struct CpuPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
    config: CpuPanelConfig,
}

impl CpuPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            state,
            config: CpuPanelConfig::default(),
        }
    }
}

impl BackendPanel for CpuPanel {
    fn title(&mut self) -> WidgetText {
        "CPU".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        let Some(latest) = data.data.last() else {
            ui.label("Loading...");
            return;
        };

        ui.horizontal(|ui| {
            ui.label(format!("Global CPU: {:.1}%", latest.cpu_stats.global_usage));
            ui.add(
                ProgressBar::new((latest.cpu_stats.global_usage / 100.0).clamp(0.0, 1.0))
                    .desired_width(180.0)
                    .text(format!("{:.1}%", latest.cpu_stats.global_usage)),
            );
            ui.checkbox(&mut self.config.show_per_cpu, "Per core");
        });

        let plot_height = ui.available_height().clamp(100.0, 600.0);
        let min_window = data.config.min_plot_window_secs();

        cpu_plot(
            ui,
            &data.data,
            &data.process_info,
            self.config.show_per_cpu,
            &data.process_selection.selected_processes,
            plot_height,
            min_window,
        );

        ui.separator();
        ui.collapsing("CPU Cores", |ui| {
            Grid::new("sysinfo_cpu_cores")
                .num_columns(4)
                .spacing([12.0, 4.0])
                .show(ui, |ui| {
                    for (i, usage) in latest.cpu_stats.per_cpu_usage.iter().enumerate() {
                        ui.label(format!("CPU {i}"));
                        ui.add(
                            ProgressBar::new((*usage / 100.0).clamp(0.0, 1.0))
                                .desired_width(90.0)
                                .text(format!("{usage:.0}%")),
                        );
                        if i % 2 == 1 {
                            ui.end_row();
                        }
                    }
                });
        });

    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.config)?)
    }

    fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.config = serde_json::from_value(config.clone())?;
        Ok(())
    }
}

fn cpu_plot(
    ui: &mut egui::Ui,
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    process_info: &HashMap<Pid, ProcessInfo>,
    show_per_cpu: bool,
    selected_processes: &HashSet<Pid>,
    plot_height: f32,
    min_window: f64,
) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest, min_window);

    let points: PlotPoints = snapshots
        .iter()
        .map(|snapshot| {
            let seconds_ago = latest
                .captured_at
                .duration_since(snapshot.captured_at)
                .as_secs_f64();
            [
                seconds_ago,
                clamp_percent(snapshot.cpu_stats.global_usage as f64),
            ]
        })
        .collect();
    let selected_points = selected_cpu_points(snapshots, latest, selected_processes, process_info);

    let plot = Plot::new("sysinfo_cpu_plot")
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
        ]);

    plot.show(ui, |plot_ui| {
        plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time_seconds);
        plot_ui.set_plot_bounds_y(MIN_USAGE_PERCENT..=MAX_USAGE_PERCENT);

        if show_per_cpu {
            let layers = stacked_cpu_layers(snapshots, latest);
            for layer in layers {
                plot_ui.add(
                    FilledArea::new(
                        format!("CPU {}", layer.cpu_index),
                        &layer.xs,
                        &layer.lower,
                        &layer.upper,
                    )
                    .fill_color(cpu_fill_color(layer.cpu_index))
                    .stroke(Stroke::new(1.0, cpu_color(layer.cpu_index))),
                );
            }
        } else {
            let total_layer = total_cpu_layer(snapshots, latest);
            plot_ui.add(
                FilledArea::new(
                    "Total",
                    &total_layer.xs,
                    &total_layer.lower,
                    &total_layer.upper,
                )
                .fill_color(Color32::from_rgba_unmultiplied(100, 181, 246, 150))
                .stroke(Stroke::new(1.0, Color32::LIGHT_BLUE)),
            );
        }

        plot_ui.line(Line::new("Total", points).color(Color32::WHITE));
        if !selected_processes.is_empty() {
            plot_ui.line(
                Line::new("Selected", selected_points)
                    .color(Color32::YELLOW)
                    .width(3.0),
            );
        }
    });
}

fn selected_cpu_points(
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    latest: &crate::backend::sysinfo::SnapshotData,
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

struct CpuLayer {
    cpu_index: usize,
    xs: Vec<f64>,
    lower: Vec<f64>,
    upper: Vec<f64>,
}

fn stacked_cpu_layers(
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    latest: &crate::backend::sysinfo::SnapshotData,
) -> Vec<CpuLayer> {
    let cpu_count = latest.cpu_stats.per_cpu_usage.len();
    let mut layers = (0..cpu_count)
        .map(|cpu_index| CpuLayer {
            cpu_index,
            xs: Vec::with_capacity(snapshots.len()),
            lower: Vec::with_capacity(snapshots.len()),
            upper: Vec::with_capacity(snapshots.len()),
        })
        .collect::<Vec<_>>();

    for snapshot in snapshots {
        let seconds_ago = latest
            .captured_at
            .duration_since(snapshot.captured_at)
            .as_secs_f64();
        let raw_sum = snapshot
            .cpu_stats
            .per_cpu_usage
            .iter()
            .map(|usage| clamp_percent(*usage as f64))
            .sum::<f64>();
        let total = clamp_percent(snapshot.cpu_stats.global_usage as f64);
        let mut stack_top = MIN_USAGE_PERCENT;

        for (cpu_index, layer) in layers.iter_mut().enumerate() {
            let raw_usage = snapshot
                .cpu_stats
                .per_cpu_usage
                .get(cpu_index)
                .map(|usage| clamp_percent(*usage as f64))
                .unwrap_or_default();
            let scaled_usage = if raw_sum > 0.0 {
                raw_usage / raw_sum * total
            } else {
                0.0
            };
            let next_stack_top = clamp_percent(stack_top + scaled_usage);

            layer.xs.push(seconds_ago);
            layer.lower.push(stack_top);
            layer.upper.push(next_stack_top);
            stack_top = next_stack_top;
        }
    }

    layers
}

fn total_cpu_layer(
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    latest: &crate::backend::sysinfo::SnapshotData,
) -> CpuLayer {
    let mut layer = CpuLayer {
        cpu_index: 0,
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
        layer
            .upper
            .push(clamp_percent(snapshot.cpu_stats.global_usage as f64));
    }

    layer
}

fn max_time_seconds(
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    latest: &crate::backend::sysinfo::SnapshotData,
    min_window: f64,
) -> f64 {
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

fn format_seconds_ago(seconds_ago: f64) -> String {
    if seconds_ago == 0.0 {
        "now".into()
    } else {
        format!("{:.0}s", seconds_ago)
    }
}

fn cpu_color(index: usize) -> Color32 {
    const COLORS: [Color32; 8] = [
        Color32::from_rgb(255, 183, 77),
        Color32::from_rgb(129, 199, 132),
        Color32::from_rgb(186, 104, 200),
        Color32::from_rgb(77, 182, 172),
        Color32::from_rgb(240, 98, 146),
        Color32::from_rgb(174, 213, 129),
        Color32::from_rgb(100, 181, 246),
        Color32::from_rgb(255, 138, 101),
    ];
    COLORS[index % COLORS.len()]
}

fn cpu_fill_color(index: usize) -> Color32 {
    let color = cpu_color(index);
    Color32::from_rgba_unmultiplied(color.r(), color.g(), color.b(), 170)
}
