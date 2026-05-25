use std::{ops::Not, sync::Arc};

use egui::{Align, Button, Checkbox, Color32, ComboBox, Layout, RichText, Sense, Ui, WidgetText, mutex::Mutex};
use egui_extras::{Column, TableBuilder};
use human_units::{FormatDuration, FormatSize};
use serde::{Deserialize, Serialize};
use sysinfo::Pid;

use crate::{BackendPanel, backend::sysinfo::{ProcessSnapshot, SnapshotData, SysinfoSharedState}};


pub(super) struct ProcessesPanel {
    config: Config,
    state: Arc<Mutex<SysinfoSharedState>>,
}

struct Config {
    filter: String,
    columns: Vec<ProcessColumn>,
    sort_by: ProcessColumn,
    sort_direction: SortDirection,
    show_threads: bool,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            filter: String::new(),
            columns: vec![
                ProcessColumn::Pid,
                ProcessColumn::Name,
                ProcessColumn::CpuUsage,
                ProcessColumn::MemoryUsage,
            ],
            sort_by: ProcessColumn::CpuUsage,
            sort_direction: SortDirection::Descending,
            show_threads: false,
        }
    }
}

impl ProcessesPanel {
    pub fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { config: Config::default(), state }
    }
}

impl BackendPanel for ProcessesPanel {
    fn title(&mut self) -> WidgetText {
        "Processes".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let state = self.state.clone();
        let mut state = state.lock();
        let (pids, existing_pids, process_count) = {
            let Some(data) = state.data.last() else {
                ui.label("Loading...");
                return;
            };
            (
                self.list_pids(data),
                data.processes.keys().copied().collect(),
                data.processes.len(),
            )
        };
        state.process_selection.retain_existing_pids(&existing_pids);

        ui.horizontal(|ui| {
            ui.add(Checkbox::new(&mut self.config.show_threads, "Show threads"));
            if ui
                .add(Checkbox::new(&mut state.process_selection.multiple_selection, "Multiple selection"))
                .changed()
                && !state.process_selection.multiple_selection
            {
                state.process_selection.retain_single_selection();
            }
            if ui.button("Clear selection").clicked() {
                state.process_selection.clear();
            }
            ui.label(format!("{} selected", state.process_selection.selected_processes.len()));
        });
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.config.filter);
            ui.label(format!("{} / {}", pids.len(), process_count));
        });
        ui.collapsing("columns", |ui| ui.vertical(|ui| {
            for column in self.config.columns.clone() {
                ui.horizontal(|ui| {
                    ui.label(column.text());
                    if ui.add(Button::new("-").fill(Color32::DARK_RED)).clicked() {
                        self.config.columns.retain(|c| c != &column);
                    }
                });
            }
            ui.horizontal(|ui| {
                ComboBox::from_label("Add column")
                    .selected_text("Add column")
                    .show_ui(ui, |ui| {
                        for column in &[ProcessColumn::Pid, ProcessColumn::Name, ProcessColumn::CpuUsage, ProcessColumn::CpuTime, ProcessColumn::MemoryUsage] {
                            if self.config.columns.contains(column) {
                                continue;
                            }
                            if ui.button(column.text()).clicked() {
                                self.config.columns.push(*column);
                            }
                        }
                    });
            })
        }));
        let clicked_pid = {
            let data = state.data.last().expect("snapshot disappeared while rendering");
            self.table(data, ui, &pids, &state.process_selection.selected_processes)
        };
        if let Some(clicked_pid) = clicked_pid {
            state.process_selection.select_process(clicked_pid);
        }
    }
}

impl ProcessesPanel {
    fn list_pids(&self, data: &SnapshotData) -> Vec<Pid> {
        let mut pids = data
            .processes
            .iter()
            .filter(|(_, p)| !p.thread_kind.is_some() || self.config.show_threads)
            .map(|(pid, _)| pid)
            .cloned()
            .collect::<Vec<_>>();
        if !self.config.filter.is_empty() {
            pids.retain(|pid| {
                let process = &data.processes[pid];
                process.name.contains(&self.config.filter)
                    || process.cmd.iter().any(|arg| arg.contains(&self.config.filter))
            });
        }
        pids.sort();

        match self.config.sort_by {
            ProcessColumn::Pid => {
                pids.sort_by_key(|pid| *pid);
            }
            ProcessColumn::Name => {
                pids.sort_by_key(|pid| data.processes[pid].name.clone());
            }
            ProcessColumn::CpuUsage => {
                pids.sort_by(|a, b| data.processes[b].cpu_usage.partial_cmp(&data.processes[a].cpu_usage).unwrap());
            }
            ProcessColumn::CpuTime => {
                pids.sort_by(|a, b| data.processes[b].accumulated_cpu_time.partial_cmp(&data.processes[a].accumulated_cpu_time).unwrap());
            }
            ProcessColumn::MemoryUsage => {
                pids.sort_by_key(|pid| data.processes[pid].memory);
            }
        }

        if self.config.sort_direction == SortDirection::Ascending {
            pids.reverse();
        }

        pids
    }

    fn table(
        &mut self,
        data: &SnapshotData,
        ui: &mut Ui,
        pids: &[Pid],
        selected_processes: &std::collections::HashSet<Pid>,
    ) -> Option<Pid> {
        let available_height = ui.available_height();
        let text_height = egui::TextStyle::Body
            .resolve(ui.style())
            .size
            .max(ui.spacing().interact_size.y);
        let total_rows = pids.len();

        let mut table = TableBuilder::new(ui)
            .striped(true)
            .resizable(true)
            .auto_shrink(false)
            .cell_layout(Layout::left_to_right(Align::Center));

        for column in &self.config.columns {
            table = table.column(if column == &ProcessColumn::Name {
                Column::remainder()
                    .at_least(40.0)
                    .clip(true)
                    .resizable(true)
            } else {
                Column::auto()
            });
        }

        let mut clicked_pid = None;

        table
            .min_scrolled_height(0.0)
            .max_scroll_height(available_height)
            .sense(Sense::click())
            .header(40.0, |mut header| {
                for column in &self.config.columns {
                    header.col(|ui| {
                        let sorted = if self.config.sort_by == *column {
                            Some(&mut self.config.sort_direction)
                        } else {
                            None
                        };
                        column.show_header(ui, &mut self.config.sort_by, sorted);
                    });
                }
            })
            .body(|body| {
                body.rows(text_height, total_rows, |mut row| {
                    let i = row.index();
                    let pid = pids[i];
                    let process = &data.processes[&pid];
                    let selected = selected_processes.contains(&pid);
                    row.set_selected(selected);
                    let mut clicked = false;
                    for column in &self.config.columns {
                        let (_, response) = row.col(|ui| {
                            column.show(process, ui);
                        });
                        clicked |= response.clicked();
                    }
                    if clicked {
                        clicked_pid = Some(pid);
                    }
                });
            });
        clicked_pid
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum SortDirection {
    Ascending,
    Descending,
}

impl Not for SortDirection {
    type Output = Self;

    fn not(self) -> Self::Output {
        match self {
            SortDirection::Ascending => SortDirection::Descending,
            SortDirection::Descending => SortDirection::Ascending,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
enum ProcessColumn {
    Pid,
    Name,
    CpuUsage,
    CpuTime,
    MemoryUsage,
}

impl ProcessColumn {
    fn text(&self) -> &'static str {
        match self {
            ProcessColumn::Pid => "PID",
            ProcessColumn::Name => "Name",
            ProcessColumn::CpuUsage => "CPU %",
            ProcessColumn::CpuTime => "CPU Time",
            ProcessColumn::MemoryUsage => "Memory",
        }
    }

    fn show_header(&self, ui: &mut Ui, sort_by: &mut ProcessColumn, sorted: Option<&mut SortDirection>) {
        let text = RichText::new(self.text()).strong();
        if ui.label(text).clicked() {
            *sort_by = *self;
        }

        if let Some(direction) = sorted {
            let symbol = match direction {
                SortDirection::Ascending => "⬆", // "▲⬆"
                SortDirection::Descending => "⬇", // "▼⬇"
            };
            if ui.label(symbol).clicked() {
                *direction = !(*direction);
            }
        }
    }

    fn show(&self, process: &ProcessSnapshot, ui: &mut Ui) {
        match self {
            ProcessColumn::Pid => {
                ui.label(format!("{}", process.pid));
            }
            ProcessColumn::Name => {
                ui.label(process.name.as_str());
            }
            ProcessColumn::CpuUsage => {
                ui.label(format!("{:.1}%", process.cpu_usage));
            }
            ProcessColumn::CpuTime => {
                ui.label(format!("{:.1}s", process.accumulated_cpu_time.format_duration()));
            }
            ProcessColumn::MemoryUsage => {
                ui.label(format!("{}", process.memory.format_size()));
            }
        }
    }
}
