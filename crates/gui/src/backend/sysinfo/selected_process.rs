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
                "Current directory",
                option_text(detail.cwd.as_deref()).as_str(),
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
                "Effective group",
                &detail
                    .effective_group_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
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
