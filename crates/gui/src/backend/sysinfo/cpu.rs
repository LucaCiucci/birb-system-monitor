use std::{sync::Arc, time::Duration};

use egui::{mutex::Mutex, Color32, Grid, ProgressBar, WidgetText};
use egui_plot::{AxisHints, Legend, Line, Plot, PlotPoints};

use crate::{backend::sysinfo::SysinfoSharedState, BackendPanel};

const MIN_USAGE_PERCENT: f64 = 0.0;
const MAX_USAGE_PERCENT: f64 = 100.0;
const MIN_TIME_SECONDS: f64 = 0.0;

pub(super) struct CpuPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
    show_per_cpu: bool,
}

impl CpuPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            state,
            show_per_cpu: true,
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
            ui.checkbox(&mut self.show_per_cpu, "Per core");
        });

        cpu_plot(ui, &data.data, self.show_per_cpu);

        ui.separator();
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

fn cpu_plot(
    ui: &mut egui::Ui,
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    show_per_cpu: bool,
) {
    let Some(latest) = snapshots.last() else {
        return;
    };
    let max_time_seconds = max_time_seconds(snapshots, latest);

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

    let plot = Plot::new("sysinfo_cpu_plot")
        .height(220.0)
        .invert_x(true)
        .default_x_bounds(MIN_TIME_SECONDS, max_time_seconds)
        .default_y_bounds(MIN_USAGE_PERCENT, MAX_USAGE_PERCENT)
        .auto_bounds(false)
        .allow_drag(false)
        .allow_zoom(false)
        .allow_scroll(false)
        .allow_boxed_zoom(false)
        .allow_double_click_reset(false)
        .legend(Legend::default())
        .custom_x_axes(vec![
            AxisHints::new_x().formatter(|mark, _| format_seconds_ago(mark.value))
        ])
        .custom_y_axes(vec![
            AxisHints::new_y().formatter(|mark, _| format!("{:.0}%", mark.value))
        ]);

    plot.show(ui, |plot_ui| {
        plot_ui.set_plot_bounds_x(MIN_TIME_SECONDS..=max_time_seconds);
        plot_ui.set_plot_bounds_y(MIN_USAGE_PERCENT..=MAX_USAGE_PERCENT);
        plot_ui.line(Line::new("Total", points).color(Color32::LIGHT_BLUE));

        if show_per_cpu {
            let cpu_count = latest.cpu_stats.per_cpu_usage.len();
            for cpu_index in 0..cpu_count {
                let points: PlotPoints = snapshots
                    .iter()
                    .filter_map(|snapshot| {
                        let usage = snapshot.cpu_stats.per_cpu_usage.get(cpu_index)?;
                        let seconds_ago = latest
                            .captured_at
                            .duration_since(snapshot.captured_at)
                            .as_secs_f64();
                        Some([seconds_ago, clamp_percent(*usage as f64)])
                    })
                    .collect();
                plot_ui.line(
                    Line::new(format!("CPU {cpu_index}"), points).color(cpu_color(cpu_index)),
                );
            }
        }
    });
}

fn max_time_seconds(
    snapshots: &[crate::backend::sysinfo::SnapshotData],
    latest: &crate::backend::sysinfo::SnapshotData,
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
