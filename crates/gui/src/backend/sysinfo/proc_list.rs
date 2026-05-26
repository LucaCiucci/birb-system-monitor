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

/// Whether the process list is live, pinned (frozen PID list), or paused (frozen snapshot).
enum FreezeState {
    Live,
    /// PIDs + sort order pinned in place. Data values still update live.
    Pin(Vec<Pid>),
    /// Everything paused — both PIDs and data frozen.
    Pause(HashMap<Pid, ProcessInfo>),
}

pub(super) struct ProcessesPanel {
    config: Config,
    state: Arc<Mutex<SysinfoSharedState>>,
    freeze: FreezeState,
}

#[derive(Serialize, Deserialize)]
struct Config {
    filter: String,
    columns: Vec<ProcessColumn>,
    sort_by: ProcessColumn,
    sort_direction: SortDirection,
    show_threads: bool,
    tree_view: bool,
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
            tree_view: false,
        }
    }
}

impl ProcessesPanel {
    pub fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self {
            config: Config::default(),
            state,
            freeze: FreezeState::Live,
        }
    }

    fn is_live(&self) -> bool {
        matches!(self.freeze, FreezeState::Live)
    }

    fn freeze_pause(&mut self) {
        let info = self.state.lock().process_info.clone();
        self.freeze = FreezeState::Pause(info);
    }

    fn freeze_pin(&mut self) {
        let state = self.state.lock();
        let pids = self.list_pids(&state.process_info);
        self.freeze = FreezeState::Pin(pids);
    }
}

impl BackendPanel for ProcessesPanel {
    fn title(&mut self) -> WidgetText {
        "Processes".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let has_data = self.state.lock().data.last().is_some();
        if !has_data {
            ui.label("Loading...");
            return;
        }

        // --- Freeze mode buttons (mutually exclusive) ---
        let is_pinned = matches!(self.freeze, FreezeState::Pin(_));
        let is_paused = matches!(self.freeze, FreezeState::Pause(_));

        ui.horizontal(|ui| {
            // Pin — freezes list order, values still update
            if ui.selectable_label(is_pinned, if is_pinned { "📌 Pinned" } else { "📌 Pin" })
                .on_hover_text("Freezes the process list order. CPU% and memory values still update live.")
                .clicked()
            {
                if is_pinned {
                    self.freeze = FreezeState::Live;
                } else {
                    self.freeze_pin();
                }
            }
            // Pause — freezes everything
            if ui.selectable_label(is_paused, if is_paused { "⏸ Paused" } else { "⏸ Pause" })
                .on_hover_text("Freezes everything — process list, CPU%, memory, all values stop updating.")
                .clicked()
            {
                if is_paused {
                    self.freeze = FreezeState::Live;
                } else {
                    self.freeze_pause();
                }
            }
            if is_paused && ui.button("Refresh").clicked() {
                self.freeze_pause();
            }
            if is_pinned && ui.button("Refresh").clicked() {
                self.freeze_pin();
            }
        });

        // --- Gather data ---
        let process_count;
        let (pids, depth_map, selected) = match &self.freeze {
            FreezeState::Pin(frozen_pids) => {
                // Frozen PID list + frozen sort order. Only filter out dead processes.
                let state = self.state.lock();
                let alive: HashSet<Pid> = state.process_info.keys().copied().collect();
                let mut pids: Vec<Pid> = frozen_pids.iter().filter(|p| alive.contains(p)).copied().collect();
                process_count = state.process_info.len();
                let selected = state.process_selection.selected_processes.clone();
                let depth_map = self.reorder_to_tree(&mut pids, &state.process_info);
                (pids, depth_map, selected)
            }
            FreezeState::Pause(held) => {
                process_count = held.len();
                let mut pids = self.list_pids(held);
                let selected: HashSet<Pid> = held.keys().copied().collect();
                let depth_map = self.reorder_to_tree(&mut pids, held);
                (pids, depth_map, selected)
            }
            FreezeState::Live => {
                let mut state = self.state.lock();
                let existing_pids: HashSet<Pid> = state.process_info.keys().copied().collect();
                state.process_selection.retain_existing_pids(&existing_pids);
                process_count = state.process_info.len();
                let mut pids = self.list_pids(&state.process_info);
                let selected = state.process_selection.selected_processes.clone();
                let depth_map = self.reorder_to_tree(&mut pids, &state.process_info);
                (pids, depth_map, selected)
            }
        };

        // --- UI controls ---
        ui.horizontal(|ui| {
            ui.add(Checkbox::new(&mut self.config.show_threads, "Show threads"));
            ui.add(Checkbox::new(&mut self.config.tree_view, "Tree view"));
            let mut ms = self.state.lock().process_selection.multiple_selection;
            if ui.add(Checkbox::new(&mut ms, "Multiple selection")).changed() {
                let mut state = self.state.lock();
                state.process_selection.multiple_selection = ms;
                if !ms {
                    state.process_selection.retain_single_selection();
                }
            }
            if ui.button("Clear selection").clicked() {
                self.state.lock().process_selection.clear();
            }
            ui.label(format!("{} selected", selected.len()));
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

        // --- Table ---
        let state_arc = self.state.clone();
        let clicked_pid = match &self.freeze {
            FreezeState::Pause(_) => {
                // Take, use, and restore to avoid borrow conflict
                if let FreezeState::Pause(held) = std::mem::replace(&mut self.freeze, FreezeState::Live) {
                    let result = self.table(&held, ui, &pids, &selected, &depth_map);
                    self.freeze = FreezeState::Pause(held);
                    result
                } else {
                    unreachable!()
                }
            }
            _ => {
                // Live or Pin: read live data from state
                let state = state_arc.lock();
                self.table(&state.process_info, ui, &pids, &selected, &depth_map)
            }
        };
        if let Some(clicked_pid) = clicked_pid {
            if self.is_live() {
                let s = self.state.clone();
                s.lock().process_selection.select_process(clicked_pid);
            }
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
    /// If tree_view is enabled, reorders `pids` into tree order (parents before children)
    /// and returns a map of pid → depth. Otherwise returns an empty map.
    fn reorder_to_tree(&self, pids: &mut Vec<Pid>, process_info: &HashMap<Pid, ProcessInfo>) -> HashMap<Pid, usize> {
        if !self.config.tree_view {
            return HashMap::new();
        }

        let pid_set: HashSet<Pid> = pids.iter().copied().collect();

        // Build parent → children map
        let mut children: HashMap<Pid, Vec<Pid>> = HashMap::new();
        let mut roots = Vec::new();

        for &pid in pids.iter() {
            let info = &process_info[&pid];
            if let Some(parent) = info.detail.parent {
                if pid_set.contains(&parent) {
                    children.entry(parent).or_default().push(pid);
                    continue;
                }
            }
            roots.push(pid);
        }

        // Sort within each parent by current sort strategy
        for siblings in children.values_mut() {
            self.sort_pids(siblings, process_info);
        }
        // Also sort roots
        self.sort_pids(&mut roots, process_info);

        // Flatten tree into PID order and build depth map
        let mut ordered = Vec::with_capacity(pids.len());
        let mut depth_map = HashMap::new();

        fn flatten(
            pid: Pid,
            depth: usize,
            children: &HashMap<Pid, Vec<Pid>>,
            ordered: &mut Vec<Pid>,
            depth_map: &mut HashMap<Pid, usize>,
        ) {
            ordered.push(pid);
            depth_map.insert(pid, depth);
            if let Some(kids) = children.get(&pid) {
                for &child in kids {
                    flatten(child, depth + 1, children, ordered, depth_map);
                }
            }
        }

        for &root in &roots {
            flatten(root, 0, &children, &mut ordered, &mut depth_map);
        }

        // Remaining PIDs that weren't reached (orphans / cycles)
        let remaining: HashSet<Pid> = pids.iter().copied().collect::<HashSet<_>>()
            .difference(&depth_map.keys().copied().collect::<HashSet<_>>())
            .copied().collect();
        for &pid in &remaining {
            ordered.push(pid);
            depth_map.insert(pid, 0);
        }

        *pids = ordered;
        depth_map
    }

    fn list_pids(&self, process_info: &HashMap<Pid, ProcessInfo>) -> Vec<Pid> {
        let mut pids: Vec<Pid> = process_info
            .iter()
            .filter(|(_, info)| !info.detail.thread_kind.is_some() || self.config.show_threads)
            .map(|(pid, _)| *pid)
            .collect();
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

        self.sort_pids(&mut pids, process_info);

        if self.config.sort_direction == SortDirection::Ascending {
            pids.reverse();
        }

        pids
    }

    fn sort_pids(&self, pids: &mut [Pid], process_info: &HashMap<Pid, ProcessInfo>) {
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
    }

    fn table(
        &mut self,
        process_info: &HashMap<Pid, ProcessInfo>,
        ui: &mut Ui,
        pids: &[Pid],
        selected_processes: &std::collections::HashSet<Pid>,
        depth_map: &HashMap<Pid, usize>,
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
                        let depth = depth_map.get(&pid).copied().unwrap_or(0);
                        row.set_selected(selected);
                        let mut clicked = false;
                        for column in &self.config.columns {
                            let (_, response) = row.col(|ui| {
                                column.show(info, ui, depth);
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

    fn show(&self, info: &ProcessInfo, ui: &mut Ui, depth: usize) {
        let latest_metrics = info.metrics.back().copied().unwrap_or_default();
        let indent = "  ".repeat(depth);
        match self {
            ProcessColumn::Pid => {
                ui.label(format!("{}", info.detail.pid));
            }
            ProcessColumn::Name => {
                let label = if depth > 0 {
                    format!("{indent}⤷ {}", info.detail.name)
                } else {
                    info.detail.name.to_string()
                };
                ui.label(label);
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
