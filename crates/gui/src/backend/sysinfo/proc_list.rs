use std::{ops::Not, sync::Arc};

use egui::{
    mutex::Mutex, Align, Button, Checkbox, Color32, ComboBox, Layout, RichText, Sense, Ui,
    WidgetText,
};
use egui_extras::{Column, TableBuilder};
use human_units::{FormatDuration, FormatSize};
use serde::{Deserialize, Serialize};
use sysinfo::Pid;

use std::collections::{HashMap, HashSet};

use crate::{
    backend::sysinfo::{ProcessInfo, SysinfoSharedState},
    BackendPanel,
};

pub(super) struct ProcessesPanel {
    config: Config,
    state: Arc<Mutex<SysinfoSharedState>>,
}

#[derive(Serialize, Deserialize)]
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
        Self {
            config: Config::default(),
            state,
        }
    }
}

impl BackendPanel for ProcessesPanel {
    fn title(&mut self) -> WidgetText {
        "Processes".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let state = self.state.clone();
        let mut state = state.lock();
        let Some(_) = state.data.last() else {
            ui.label("Loading...");
            return;
        };
        let existing_pids: HashSet<Pid> = state.process_info.keys().copied().collect();
        state.process_selection.retain_existing_pids(&existing_pids);
        let pids = self.list_pids(&state.process_info);
        let process_count = state.process_info.len();

        ui.horizontal(|ui| {
            ui.add(Checkbox::new(&mut self.config.show_threads, "Show threads"));
            if ui
                .add(Checkbox::new(
                    &mut state.process_selection.multiple_selection,
                    "Multiple selection",
                ))
                .changed()
                && !state.process_selection.multiple_selection
            {
                state.process_selection.retain_single_selection();
            }
            if ui.button("Clear selection").clicked() {
                state.process_selection.clear();
            }
            ui.label(format!(
                "{} selected",
                state.process_selection.selected_processes.len()
            ));
        });
        ui.horizontal(|ui| {
            ui.label("Filter:");
            ui.text_edit_singleline(&mut self.config.filter);
            ui.label(format!("{} / {}", pids.len(), process_count));
        });
        ui.collapsing("columns", |ui| {
            ui.vertical(|ui| {
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
                            for column in &[
                                ProcessColumn::Pid,
                                ProcessColumn::Name,
                                ProcessColumn::CpuUsage,
                                ProcessColumn::CpuTime,
                                ProcessColumn::MemoryUsage,
                            ] {
                                if self.config.columns.contains(column) {
                                    continue;
                                }
                                if ui.button(column.text()).clicked() {
                                    self.config.columns.push(*column);
                                }
                            }
                        });
                })
            })
        });
        let clicked_pid = {
            self.table(&state.process_info, ui, &pids, &state.process_selection.selected_processes)
        };
        if let Some(clicked_pid) = clicked_pid {
            state.process_selection.select_process(clicked_pid);
        }
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.config)?)
    }

    fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.config = serde_json::from_value(config.clone())?;
        Ok(())
    }
}

impl ProcessesPanel {
    fn list_pids(&self, process_info: &HashMap<Pid, ProcessInfo>) -> Vec<Pid> {
        let mut pids = process_info
            .iter()
            .filter(|(_, info)| !info.detail.thread_kind.is_some() || self.config.show_threads)
            .map(|(pid, _)| *pid)
            .collect::<Vec<_>>();
        if !self.config.filter.is_empty() {
            pids.retain(|pid| {
                if let Some(info) = process_info.get(pid) {
                    info.detail.name.contains(&self.config.filter)
                        || info.detail.cmd.iter().any(|arg| arg.contains(&self.config.filter))
                } else {
                    false
                }
            });
        }
        pids.sort();

        match self.config.sort_by {
            ProcessColumn::Pid => {
                pids.sort_by_key(|pid| *pid);
            }
            ProcessColumn::Name => {
                pids.sort_by_key(|pid| process_info[pid].detail.name.clone());
            }
            ProcessColumn::CpuUsage => {
                pids.sort_by(|a, b| {
                    let latest_a = process_info[a].metrics.back().map(|m| m.cpu_usage).unwrap_or(0.0);
                    let latest_b = process_info[b].metrics.back().map(|m| m.cpu_usage).unwrap_or(0.0);
                    latest_b.partial_cmp(&latest_a).unwrap()
                });
            }
            ProcessColumn::CpuTime => {
                pids.sort_by(|a, b| {
                    process_info[b]
                        .detail
                        .accumulated_cpu_time
                        .partial_cmp(&process_info[a].detail.accumulated_cpu_time)
                        .unwrap()
                });
            }
            ProcessColumn::MemoryUsage => {
                pids.sort_by(|a, b| {
                    let latest_a = process_info[a].metrics.back().map(|m| m.memory).unwrap_or(0);
                    let latest_b = process_info[b].metrics.back().map(|m| m.memory).unwrap_or(0);
                    latest_b.cmp(&latest_a)
                });
            }
        }

        if self.config.sort_direction == SortDirection::Ascending {
            pids.reverse();
        }

        pids
    }

    fn table(
        &mut self,
        process_info: &HashMap<Pid, ProcessInfo>,
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
                    if let Some(info) = process_info.get(&pid) {
                        let selected = selected_processes.contains(&pid);
                        row.set_selected(selected);
                        let mut clicked = false;
                        for column in &self.config.columns {
                            let (_, response) = row.col(|ui| {
                                column.show(info, ui);
                            });
                            clicked |= response.clicked();
                        }
                        if clicked {
                            clicked_pid = Some(pid);
                        }
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

    fn show_header(
        &self,
        ui: &mut Ui,
        sort_by: &mut ProcessColumn,
        sorted: Option<&mut SortDirection>,
    ) {
        let text = RichText::new(self.text()).strong();
        if ui.label(text).clicked() {
            *sort_by = *self;
        }

        if let Some(direction) = sorted {
            let symbol = match direction {
                SortDirection::Ascending => "⬆",  // "▲⬆"
                SortDirection::Descending => "⬇", // "▼⬇"
            };
            if ui.label(symbol).clicked() {
                *direction = !(*direction);
            }
        }
    }

    fn show(&self, info: &ProcessInfo, ui: &mut Ui) {
        let latest_metrics = info.metrics.back().copied().unwrap_or_default();
        match self {
            ProcessColumn::Pid => {
                ui.label(format!("{}", info.detail.pid));
            }
            ProcessColumn::Name => {
                ui.label(info.detail.name.as_str());
            }
            ProcessColumn::CpuUsage => {
                ui.label(format!("{:.1}%", latest_metrics.cpu_usage));
            }
            ProcessColumn::CpuTime => {
                ui.label(format!(
                    "{:.2}",
                    info.detail.accumulated_cpu_time.format_duration()
                ));
            }
            ProcessColumn::MemoryUsage => {
                ui.label(format!("{}", latest_metrics.memory.format_size()));
            }
        }
    }
}
