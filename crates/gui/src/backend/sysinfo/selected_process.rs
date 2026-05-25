use std::{collections::HashSet, fmt::Display, sync::Arc};

use egui::{mutex::Mutex, Grid, RichText, WidgetText};
use human_units::{FormatDuration, FormatSize};

use crate::{
    backend::sysinfo::{ProcessSnapshot, SysinfoSharedState},
    BackendPanel,
};

pub(super) struct SelectedProcessPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
    selected_index: usize,
}

impl SelectedProcessPanel {
    pub(super) fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            state,
            selected_index: 0,
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
                let p = selected_pids.get(self.selected_index).and_then(|pid| data.processes.get(&pid).cloned());
                (selected_pids, p)
            }
        };

        if selected_pids.is_empty() {
            ui.label("No selected process.");
            return;
        }

        ui.horizontal(|ui| {
            ui.label("Selected index:");
            let mut index = self.selected_index + 1;
            if ui
                .add(
                    egui::DragValue::new(&mut index)
                        .range(1..=100)
                        .speed(1),
                )
                .changed()
            {
                self.selected_index = index.saturating_sub(1);
            }
        });

        let Some(process) = process else {
            ui.label("Selected process is not present in the latest snapshot.");
            return;
        };

        process_summary(ui, &process);
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
    ui.label(value.to_string());
    ui.end_row();
}

fn option_text(value: Option<&str>) -> String {
    value.unwrap_or("unknown").into()
}
