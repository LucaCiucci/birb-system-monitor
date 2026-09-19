use std::{
    collections::{HashMap, HashSet, VecDeque},
    time::Duration,
};

pub(super) use crate::backend::sysinfo::SnapshotData;
use crate::backend::sysinfo::{
    ComponentsSnapshot, ProcessDiskUsage, ProcessSnapshot, SysinfoMessage,
};
use egui::WidgetText;
use serde::{Deserialize, Serialize};
use sysinfo::Pid;
use ustr::Ustr;

use crate::gui::{
    Panel, PanelInfo,
    backend::sysinfo::{
        cpu::CpuPanel, dashboard::DashboardPanel, disk_io::DiskIoPanel, memory::MemoryPanel,
        network::NetworkPanel, proc_list::ProcessesPanel, selected_process::SelectedProcessPanel,
        settings::SettingsPanel, temperature::TemperaturePanel,
        temperature_chart::TemperatureChartPanel,
    },
    panels::PanelId,
};

mod cpu;
mod dashboard;
mod disk_io;
mod memory;
mod network;
mod proc_list;
mod selected_process;
mod settings;
mod temperature;
mod temperature_chart;

pub struct SysinfoFrontend {
    pub(crate) state: SysinfoSharedState,
}

impl SysinfoFrontend {
    pub fn new() -> Self {
        Self {
            state: SysinfoSharedState::new(),
        }
    }
}

impl SysinfoFrontend {
    pub fn name(&self) -> WidgetText {
        "Sysinfo".into()
    }

    pub fn panels(&self) -> Vec<PanelInfo> {
        vec![
            PanelInfo {
                id: PanelId::Cpu,
                title: "CPU".into(),
                description: "Shows CPU usage".into(),
            },
            PanelInfo {
                id: PanelId::Memory,
                title: "Memory".into(),
                description: "Shows memory usage".into(),
            },
            PanelInfo {
                id: PanelId::Processes,
                title: "Processes".into(),
                description: "Shows process information".into(),
            },
            PanelInfo {
                id: PanelId::SelectedProcess,
                title: "Selected Process".into(),
                description: "Shows details for a selected process".into(),
            },
            PanelInfo {
                id: PanelId::Network,
                title: "Network".into(),
                description: "Shows network I/O usage".into(),
            },
            PanelInfo {
                id: PanelId::DiskIo,
                title: "Disk I/O".into(),
                description: "Shows disk I/O usage".into(),
            },
            PanelInfo {
                id: PanelId::Dashboard,
                title: "Dashboard".into(),
                description: "Shows all graphs in a responsive grid".into(),
            },
            PanelInfo {
                id: PanelId::Settings,
                title: "Sysinfo Settings".into(),
                description: "Configure sysinfo backend settings".into(),
            },
            PanelInfo {
                id: PanelId::Temperature,
                title: "Temperatures".into(),
                description: "Shows component temperatures".into(),
            },
            PanelInfo {
                id: PanelId::TemperatureChart,
                title: "Temperature Chart".into(),
                description: "Shows temperature history chart".into(),
            },
        ]
    }

    pub fn new_panel(&self, panel_id: &PanelId) -> Box<dyn Panel> {
        match panel_id {
            PanelId::Cpu => Box::new(CpuPanel::new()),
            PanelId::Memory => Box::new(MemoryPanel::new()),
            PanelId::Processes => Box::new(ProcessesPanel::new()),
            PanelId::SelectedProcess => Box::new(SelectedProcessPanel::new()),
            PanelId::Network => Box::new(NetworkPanel::new()),
            PanelId::DiskIo => Box::new(DiskIoPanel::new()),
            PanelId::Dashboard => Box::new(DashboardPanel::new()),
            PanelId::Settings => Box::new(SettingsPanel::new()),
            PanelId::Temperature => Box::new(TemperaturePanel::new()),
            PanelId::TemperatureChart => Box::new(TemperatureChartPanel::new()),
            _ => panic!("Unknown panel id: {}", panel_id),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct SysinfoConfig {
    pub(super) update_interval: Duration,
    #[serde(default = "default_temperature_interval")]
    pub(super) temperature_interval: Duration,
    /// Number of historical readings to keep and display on plots.
    /// 0 = keep up to 600 (full range).
    pub max_readings: usize,
    /// Collect details and retain history only for selected PIDs.
    #[serde(default = "default_limit_processes_to_selection")]
    pub limit_processes_to_selection: bool,
}

impl SysinfoConfig {
    /// Minimum time window to show on plots, in seconds.
    /// Returns 0 if `max_readings` is 0 (use full data range).
    pub fn min_plot_window_secs(&self) -> f64 {
        if self.max_readings == 0 {
            0.0
        } else {
            self.update_interval.as_secs_f64() * self.max_readings as f64
        }
    }

    /// Number of historical snapshots to keep in memory.
    pub fn max_history_readings(&self) -> usize {
        if self.max_readings == 0 {
            600
        } else {
            self.max_readings
        }
    }
}

fn default_temperature_interval() -> Duration {
    Duration::from_secs(5)
}

fn default_limit_processes_to_selection() -> bool {
    true
}

impl Default for SysinfoConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_secs(1),
            temperature_interval: default_temperature_interval(),
            max_readings: 60,
            limit_processes_to_selection: default_limit_processes_to_selection(),
        }
    }
}

pub(crate) struct SysinfoSharedState {
    pub(crate) applied_update_interval: Option<Duration>,
    pub(crate) applied_temperature_interval: Option<Duration>,
    pub(crate) config: SysinfoConfig,
    process_selection: ProcessSelection,
    process_info: HashMap<Pid, ProcessInfo>,
    data: Vec<SnapshotData>,
    temperatures: Vec<ComponentsSnapshot>,
}

impl SysinfoSharedState {
    fn new() -> Self {
        Self {
            applied_update_interval: None,
            applied_temperature_interval: None,
            config: Default::default(),
            process_selection: Default::default(),
            process_info: HashMap::new(),
            data: Vec::new(),
            temperatures: Vec::new(),
        }
    }

    pub(super) fn selected_pids(&self) -> impl Iterator<Item = Pid> + '_ {
        self.process_selection.selected_processes.iter().copied()
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub(super) struct ProcessSelection {
    pub(super) selected_processes: HashSet<Pid>,
    pub(super) multiple_selection: bool,
}

impl ProcessSelection {
    pub(super) fn select_process(&mut self, pid: Pid) {
        if self.multiple_selection {
            if !self.selected_processes.insert(pid) {
                self.selected_processes.remove(&pid);
            }
        } else {
            self.selected_processes.clear();
            self.selected_processes.insert(pid);
        }
    }

    pub(super) fn clear(&mut self) {
        self.selected_processes.clear();
    }

    pub(super) fn retain_existing_pids(&mut self, pids: &HashSet<Pid>) {
        self.selected_processes.retain(|pid| pids.contains(pid));
    }

    pub(super) fn retain_single_selection(&mut self) {
        if let Some(pid) = self.selected_processes.iter().next().copied() {
            self.selected_processes.clear();
            self.selected_processes.insert(pid);
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ProcessMetrics {
    pub(super) cpu_usage: f32,
    pub(super) memory: u64,
    pub(super) virtual_memory: u64,
}

impl Default for ProcessMetrics {
    fn default() -> Self {
        Self {
            cpu_usage: 0.0,
            memory: 0,
            virtual_memory: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProcessDetail {
    pub(super) pid: Pid,
    pub(super) parent: Option<Pid>,
    pub(super) name: Ustr,
    pub(super) cmd: VecDeque<Ustr>,
    pub(super) exe: Option<Ustr>,
    pub(super) environ: Vec<Ustr>,
    pub(super) cwd: Option<Ustr>,
    pub(super) root: Option<Ustr>,
    pub(super) accumulated_cpu_time: Duration,
    pub(super) du: ProcessDiskUsage,
    pub(super) status: String,
    pub(super) user_id: Option<String>,
    pub(super) effective_user_id: Option<String>,
    pub(super) group_id: Option<String>,
    pub(super) effective_group_id: Option<String>,
    pub(super) start_time: u64,
    pub(super) run_time: u64,
    pub(super) session_id: Option<Pid>,
    pub(super) open_files: Option<usize>,
    pub(super) open_files_limit: Option<usize>,
    pub(super) thread_kind: Option<String>,
}

impl From<ProcessSnapshot> for ProcessDetail {
    fn from(p: ProcessSnapshot) -> Self {
        Self {
            pid: p.pid.to_pid(),
            parent: p.parent.map(|p| p.to_pid()),
            name: p.name.as_str().into(),
            cmd: p.cmd.iter().map(|s| s.as_str().into()).collect(),
            exe: p.exe.as_deref().map(Into::into),
            environ: p.environ.iter().map(|s| s.as_str().into()).collect(),
            cwd: p.cwd.as_deref().map(Into::into),
            root: p.root.as_deref().map(Into::into),
            accumulated_cpu_time: p.accumulated_cpu_time,
            du: p.disk_usage,
            status: p.status,
            user_id: p.user_id,
            effective_user_id: p.effective_user_id,
            group_id: p.group_id,
            effective_group_id: p.effective_group_id,
            start_time: p.start_time,
            run_time: p.run_time,
            session_id: p.session_id.map(|p| p.to_pid()),
            open_files: p.open_files,
            open_files_limit: p.open_files_limit,
            thread_kind: p.thread_kind,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProcessInfo {
    pub(super) detail: ProcessDetail,
    /// Retained metrics for selected processes. Unselected processes keep only their
    /// latest reading for the process list.
    pub(super) metrics: VecDeque<ProcessMetrics>,
}

impl SysinfoSharedState {
    pub(super) fn receive(&mut self, message: SysinfoMessage) {
        let max = self.config.max_history_readings().min(10000);
        match message {
            SysinfoMessage::Components(snapshot) => {
                self.temperatures.push(snapshot);
                let excess = self.temperatures.len().saturating_sub(max);
                self.temperatures.drain(..excess);
            }
            SysinfoMessage::Snapshot(mut snapshot) => {
                let count = self.data.len();
                let pids: HashSet<_> = snapshot.pids.iter().map(|p| p.to_pid()).collect();
                for process in std::mem::take(&mut snapshot.processes) {
                    let pid = process.pid.to_pid();
                    let retain_history = !self.config.limit_processes_to_selection
                        || self.process_selection.selected_processes.contains(&pid);
                    let metrics = ProcessMetrics {
                        cpu_usage: process.cpu_usage,
                        memory: process.memory,
                        virtual_memory: process.virtual_memory,
                    };
                    // PID reuse starts a new history even if the old PID never disappeared.
                    if self
                        .process_info
                        .get(&pid)
                        .is_some_and(|p| p.detail.start_time != process.start_time)
                    {
                        self.process_info.remove(&pid);
                    }
                    let detail = ProcessDetail::from(process);
                    let info = self.process_info.entry(pid).or_insert_with(|| ProcessInfo {
                        detail: detail.clone(),
                        metrics: if retain_history {
                            VecDeque::from(vec![ProcessMetrics::default(); count])
                        } else {
                            VecDeque::new()
                        },
                    });
                    info.detail = detail;
                    if retain_history {
                        if info.metrics.len() <= 1 {
                            info.metrics = VecDeque::from(vec![ProcessMetrics::default(); count]);
                        }
                        info.metrics.push_back(metrics);
                    } else {
                        info.metrics.clear();
                        info.metrics.push_back(metrics);
                    }
                }
                self.process_info.retain(|pid, _| pids.contains(pid));
                self.process_selection.retain_existing_pids(&pids);
                self.data.push(snapshot);
                let excess = self.data.len().saturating_sub(max);
                self.data.drain(..excess);
                for info in self.process_info.values_mut() {
                    if info.metrics.len() > 1 {
                        info.metrics.drain(..excess);
                    }
                }
            }
        }
    }
}
