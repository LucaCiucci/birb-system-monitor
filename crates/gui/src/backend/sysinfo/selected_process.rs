use std::{collections::HashSet, fmt::Display, sync::Arc};

use egui::{mutex::Mutex, Grid, RichText, WidgetText};
use human_units::{FormatDuration, FormatSize};
use serde::{Deserialize, Serialize};
use sysinfo::Pid;

use crate::{
    backend::sysinfo::{ProcessDetail, ProcessMetrics, SysinfoSharedState},
    BackendPanel,
};

#[derive(Serialize, Deserialize)]
struct SelectedProcessPanelConfig {
    selected_index: usize,
}

impl Default for SelectedProcessPanelConfig {
    fn default() -> Self {
        Self { selected_index: 0 }
    }
}

pub(super) struct SelectedProcessPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
    config: SelectedProcessPanelConfig,
}

impl SelectedProcessPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            state,
            config: SelectedProcessPanelConfig::default(),
        }
    }
}

impl BackendPanel for SelectedProcessPanel {
    fn title(&mut self) -> WidgetText {
        "Selected Process".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let state = self.state.lock();
        let Some(_) = state.data.last() else {
            ui.label("Loading...");
            return;
        };

        let existing_pids: HashSet<Pid> = state.process_info.keys().copied().collect();
        drop(state);
        let mut state = self.state.lock();
        state.process_selection.retain_existing_pids(&existing_pids);

        let mut selected_pids: Vec<Pid> = state
            .process_selection
            .selected_processes
            .iter()
            .copied()
            .filter(|pid| state.process_info.contains_key(pid))
            .collect();
        selected_pids.sort();

        if selected_pids.is_empty() {
            drop(state);
            ui.label("No selected process.");
            return;
        }

        let Some(pid) = selected_pids.get(self.config.selected_index).copied() else {
            drop(state);
            ui.label("No selected process.");
            return;
        };

        let Some(info) = state.process_info.get(&pid) else {
            drop(state);
            ui.label("Selected process not found.");
            return;
        };

        ui.horizontal(|ui| {
            ui.label("Selected index:");
            let mut index = self.config.selected_index + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut index)
                        .range(1..=100)
                        .speed(1),
                )
                .changed()
            {
                self.config.selected_index = index.saturating_sub(1);
            }
        });

        let latest_metrics = info.metrics.back().copied().unwrap_or_default();
        process_summary(ui, &info.detail, &latest_metrics);
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.config)?)
    }

    fn load_config(&mut self, _config: &serde_json::Value) -> anyhow::Result<()> {
        self.config = serde_json::from_value(_config.clone())?;
        Ok(())
    }
}

fn process_summary(ui: &mut egui::Ui, detail: &ProcessDetail, metrics: &ProcessMetrics) {
    ui.separator();
    ui.heading(detail.name.as_str());

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("PID {}", detail.pid)).strong());
        ui.label(format!("CPU {:.1}%", metrics.cpu_usage));
        ui.label(format!("Memory {}", metrics.memory.format_size()));
        if let Some(thread_kind) = detail.thread_kind {
            ui.label(format!("Thread: {thread_kind:?}"));
        }
    });

    ui.separator();
    let available_width = ui.available_rect_before_wrap();
    ui.set_max_width(available_width.width());
    Grid::new("selected_process_overview")
        .num_columns(2)
        .striped(true)
        .spacing([16.0, 6.0])
        .show(ui, |ui| {
            value_row(ui, "Name", detail.name.as_str());
            value_row(ui, "PID", detail.pid);
            value_row(ui, "Status", &format!("{:?}", detail.status));
            value_row(ui, "CPU usage", &format!("{:.1}%", metrics.cpu_usage));
            value_row(
                ui,
                "CPU time",
                &format!("{}", detail.accumulated_cpu_time.format_duration()),
            );
            value_row(ui, "Memory", metrics.memory.format_size());
            value_row(ui, "Virtual memory", metrics.virtual_memory.format_size());
            value_row(
                ui,
                "Executable",
                option_text(detail.exe.as_deref()).as_str(),
            );
            value_row(
                ui,
                "Current directory",
                option_text(detail.cwd.as_deref()).as_str(),
            );
            value_row(
                ui,
                "Root directory",
                option_text(detail.root.as_deref()).as_str(),
            );
            value_row(
                ui,
                "User",
                &detail
                    .user_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Effective user",
                &detail
                    .effective_user_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Group",
                &detail
                    .group_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Effective group",
                &detail
                    .effective_group_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Start time",
                &format_time(detail.start_time),
            );
            value_row(
                ui,
                "Run time",
                &format_duration_secs(detail.run_time),
            );
            value_row(
                ui,
                "Session ID",
                &detail
                    .session_id
                    .map(|id| format!("{id}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Open files",
                &format_open_files(detail.open_files, detail.open_files_limit),
            );
            value_row(ui, "Disk read", detail.du.read_bytes.format_size());
            value_row(ui, "Disk written", detail.du.written_bytes.format_size());
            value_row(
                ui,
                "Total disk read",
                detail.du.total_read_bytes.format_size(),
            );
            value_row(
                ui,
                "Total disk written",
                detail.du.total_written_bytes.format_size(),
            );
            {
                ui.label(format!("cmd ({})", detail.cmd.len()));
                ui.collapsing("args", |ui| {
                    Grid::new("cmd_grid")
                        .num_columns(1)
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            for arg in &detail.cmd {
                                ui.monospace(arg.as_str());
                                ui.end_row();
                            }
                        });
                });
                ui.end_row();
            }
            {
                ui.label(format!("environ ({})", detail.environ.len()));
                ui.collapsing("vars", |ui| {
                    Grid::new("environ_grid")
                        .num_columns(1)
                        .striped(true)
                        .spacing([16.0, 4.0])
                        .show(ui, |ui| {
                            for var in &detail.environ {
                                ui.monospace(var.as_str());
                                ui.end_row();
                            }
                        });
                });
                ui.end_row();
            }
        });

    ui.collapsing("Command", |ui| {
        if detail.cmd.is_empty() {
            ui.label("No command line available.");
        } else {
            let command = detail
                .cmd
                .iter()
                .map(|arg| arg.as_str())
                .collect::<Vec<_>>()
                .join(" ");
            ui.monospace(command);
        }
    });
}

fn value_row(ui: &mut egui::Ui, label: &str, value: impl Display) {
    ui.label(RichText::new(label).strong());
    //ui.label(value.to_string());
    ui.horizontal(|ui| {
        ui.add(egui::Label::new(value.to_string()).truncate());
        ui.set_min_width(ui.available_rect_before_wrap().width());
    });
    ui.end_row();
}

fn option_text(value: Option<&str>) -> String {
    value.unwrap_or("unknown").into()
}

fn format_time(epoch_secs: u64) -> String {
    // Convert epoch seconds to a readable date/time
    let secs_per_day = 86400u64;
    let secs_per_hour = 3600u64;
    let secs_per_min = 60u64;

    let days = epoch_secs / secs_per_day;
    let remaining = epoch_secs % secs_per_day;
    let hours = remaining / secs_per_hour;
    let remaining = remaining % secs_per_hour;
    let minutes = remaining / secs_per_min;
    let seconds = remaining % secs_per_min;

    // days since epoch
    // Approximate year/month/day from days since epoch (1970-01-01)
    // Good enough for display purposes
    let mut y = 1970i64;
    let mut remaining_days = days as i64;
    loop {
        let days_in_year = if is_leap(y) { 366 } else { 365 };
        if remaining_days < days_in_year {
            break;
        }
        remaining_days -= days_in_year;
        y += 1;
    }
    let months_days = if is_leap(y) {
        [31, 29, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    } else {
        [31, 28, 31, 30, 31, 30, 31, 31, 30, 31, 30, 31]
    };
    let mut m = 1usize;
    for &md in &months_days {
        if remaining_days < md {
            break;
        }
        remaining_days -= md;
        m += 1;
    }
    let d = remaining_days + 1;
    format!(
        "{:04}-{:02}-{:02} {:02}:{:02}:{:02}",
        y, m, d, hours, minutes, seconds
    )
}

fn is_leap(year: i64) -> bool {
    (year % 4 == 0 && year % 100 != 0) || year % 400 == 0
}

fn format_duration_secs(total_secs: u64) -> String {
    let days = total_secs / 86400;
    let remaining = total_secs % 86400;
    let hours = remaining / 3600;
    let remaining = remaining % 3600;
    let minutes = remaining / 60;
    let seconds = remaining % 60;
    if days > 0 {
        format!("{days}d {hours}h {minutes}m {seconds}s")
    } else if hours > 0 {
        format!("{hours}h {minutes}m {seconds}s")
    } else if minutes > 0 {
        format!("{minutes}m {seconds}s")
    } else {
        format!("{seconds}s")
    }
}

fn format_open_files(current: Option<usize>, limit: Option<usize>) -> String {
    match (current, limit) {
        (Some(c), Some(l)) => format!("{c} / {l}"),
        (Some(c), None) => format!("{c}"),
        (None, Some(l)) => format!("? / {l}"),
        (None, None) => "unknown".into(),
    }
}
