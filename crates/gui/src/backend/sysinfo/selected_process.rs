use std::{collections::HashSet, fmt::Display, sync::Arc};

use egui::{mutex::Mutex, Grid, RichText, WidgetText};
use human_units::{FormatDuration, FormatSize};
use serde::{Deserialize, Serialize};

use crate::{
    backend::sysinfo::{ProcessSnapshot, SysinfoSharedState},
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
        let (selected_pids, process) = {
            let mut state = self.state.lock();
            let Some(data) = state.data.last() else {
                ui.label("Loading...");
                return;
            };

            let existing_pids = data.processes.keys().copied().collect::<HashSet<_>>();
            state.process_selection.retain_existing_pids(&existing_pids);

            let Some(data) = state.data.last() else {
                ui.label("Loading...");
                return;
            };
            let mut selected_pids = state
                .process_selection
                .selected_processes
                .iter()
                .copied()
                .filter(|pid| data.processes.contains_key(pid))
                .collect::<Vec<_>>();
            selected_pids.sort();

            if selected_pids.is_empty() {
                (selected_pids, None)
            } else {
                let p = selected_pids.get(self.config.selected_index).and_then(|pid| data.processes.get(&pid).cloned());
                (selected_pids, p)
            }
        };

        if selected_pids.is_empty() {
            ui.label("No selected process.");
            return;
        }

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

        let Some(process) = process else {
            ui.label("Selected process is not present in the latest snapshot.");
            return;
        };

        process_summary(ui, &process);
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.config)?)
    }

    fn load_config(&mut self, _config: &serde_json::Value) -> anyhow::Result<()> {
        self.config = serde_json::from_value(_config.clone())?;
        Ok(())
    }
}

fn process_summary(ui: &mut egui::Ui, process: &ProcessSnapshot) {
    ui.separator();
    ui.heading(process.name.as_str());

    ui.horizontal_wrapped(|ui| {
        ui.label(RichText::new(format!("PID {}", process.pid)).strong());
        ui.label(format!("CPU {:.1}%", process.cpu_usage));
        ui.label(format!("Memory {}", process.memory.format_size()));
        if let Some(thread_kind) = process.thread_kind {
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
            value_row(ui, "Name", process.name.as_str());
            value_row(ui, "PID", process.pid);
            value_row(ui, "CPU usage", &format!("{:.1}%", process.cpu_usage));
            value_row(
                ui,
                "CPU time",
                &format!("{}", process.accumulated_cpu_time.format_duration()),
            );
            value_row(ui, "Memory", process.memory.format_size());
            value_row(ui, "Virtual memory", process.virtual_memory.format_size());
            value_row(
                ui,
                "Current directory",
                option_text(process.cwd.as_deref()).as_str(),
            );
            value_row(
                ui,
                "Effective user",
                &process
                    .effective_user_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(
                ui,
                "Effective group",
                &process
                    .effective_group_id
                    .as_ref()
                    .map(|id| format!("{id:?}"))
                    .unwrap_or_else(|| "unknown".into()),
            );
            value_row(ui, "Disk read", process.du.read_bytes.format_size());
            value_row(ui, "Disk written", process.du.written_bytes.format_size());
            value_row(
                ui,
                "Total disk read",
                process.du.total_read_bytes.format_size(),
            );
            value_row(
                ui,
                "Total disk written",
                process.du.total_written_bytes.format_size(),
            );
            {
                ui.label(format!("cmd ({})", process.cmd.len()));
                ui.collapsing("args", |ui| {
                    Grid::new("cmd_grid")
                        .num_columns(1)
                        .striped(true)
                        .spacing([16.0, 6.0])
                        .show(ui, |ui| {
                            for arg in &process.cmd {
                                ui.monospace(arg.as_str());
                                ui.end_row();
                            }
                        });
                });
                ui.end_row();
            }
        });

    ui.collapsing("Command", |ui| {
        if process.cmd.is_empty() {
            ui.label("No command line available.");
        } else {
            let command = process
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
