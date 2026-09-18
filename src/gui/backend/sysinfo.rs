use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    time::Duration,
};

pub(super) use crate::backend::sysinfo::SnapshotData;
use crate::backend::sysinfo::{
    ComponentsSnapshot, ProcessDiskUsage, ProcessSnapshot, SysinfoMessage,
};
use egui::{WidgetText, mutex::Mutex};
use serde::{Deserialize, Serialize};
use sysinfo::Pid;
use ustr::Ustr;

use crate::gui::{
    BackendPanel, BackendPanelId, BackendPanelInfo,
    backend::sysinfo::{
        cpu::CpuPanel, dashboard::DashboardPanel, disk_io::DiskIoPanel, memory::MemoryPanel,
        network::NetworkPanel, proc_list::ProcessesPanel, selected_process::SelectedProcessPanel,
        settings::SettingsPanel, temperature::TemperaturePanel,
        temperature_chart::TemperatureChartPanel,
    },
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
    pub(super) state: Arc<Mutex<SysinfoSharedState>>,
}

impl SysinfoFrontend {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(SysinfoSharedState::new())),
        }
    }
}

impl SysinfoFrontend {
    pub fn name(&self) -> WidgetText {
        "Sysinfo".into()
    }

    pub fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.state.lock().config)?)
    }

    pub fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.state.lock().config = serde_json::from_value(config.clone())?;
        Ok(())
    }

    pub fn panels(&self) -> Vec<BackendPanelInfo> {
        vec![
            BackendPanelInfo {
                id: BackendPanelId("cpu".into()),
                title: "CPU".into(),
                description: "Shows CPU usage".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("memory".into()),
                title: "Memory".into(),
                description: "Shows memory usage".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("processes".into()),
                title: "Processes".into(),
                description: "Shows process information".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("selected-process".into()),
                title: "Selected Process".into(),
                description: "Shows details for a selected process".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("network".into()),
                title: "Network".into(),
                description: "Shows network I/O usage".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("disk-io".into()),
                title: "Disk I/O".into(),
                description: "Shows disk I/O usage".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("dashboard".into()),
                title: "Dashboard".into(),
                description: "Shows all graphs in a responsive grid".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("settings".into()),
                title: "Sysinfo Settings".into(),
                description: "Configure sysinfo backend settings".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("temperature".into()),
                title: "Temperatures".into(),
                description: "Shows component temperatures".into(),
            },
            BackendPanelInfo {
                id: BackendPanelId("temperature-chart".into()),
                title: "Temperature Chart".into(),
                description: "Shows temperature history chart".into(),
            },
        ]
    }

    pub fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
        match panel_id.0.as_str() {
            "cpu" => Box::new(CpuPanel::new(self.state.clone())),
            "memory" => Box::new(MemoryPanel::new(self.state.clone())),
            "processes" => Box::new(ProcessesPanel::new(self.state.clone())),
            "selected-process" => Box::new(SelectedProcessPanel::new(self.state.clone())),
            "network" => Box::new(NetworkPanel::new(self.state.clone())),
            "disk-io" => Box::new(DiskIoPanel::new(self.state.clone())),
            "dashboard" => Box::new(DashboardPanel::new(self.state.clone())),
            "settings" => Box::new(SettingsPanel::new(self.state.clone())),
            "temperature" => Box::new(TemperaturePanel::new(self.state.clone())),
            "temperature-chart" => Box::new(TemperatureChartPanel::new(self.state.clone())),
            _ => panic!("Unknown panel id: {}", panel_id.0),
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

pub(super) struct SysinfoSharedState {
    pub(super) applied_update_interval: Option<Duration>,
    pub(super) applied_temperature_interval: Option<Duration>,
    pub(super) config: SysinfoConfig,
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

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::sysinfo::{
        ComponentStats, CpuStats, DiskIoStats, GeneralStats, NetworkStats, PidV,
    };

    fn sample(pid: u32, start_time: u64) -> SnapshotData {
        let process: ProcessSnapshot = serde_json::from_value(serde_json::json!({
            "pid": pid, "parent": null, "name": "test", "cmd": [], "exe": null,
            "environ": [], "cwd": null, "root": null, "cpu_usage": 25.0,
            "memory": 100, "virtual_memory": 200, "accumulated_cpu_time": {"secs": 1, "nanos": 0},
            "disk_usage": {"read_bytes": 0, "written_bytes": 0, "total_read_bytes": 0, "total_written_bytes": 0},
            "status": "running", "user_id": null, "effective_user_id": null,
            "group_id": null, "effective_group_id": null, "start_time": start_time,
            "run_time": 1, "session_id": null, "open_files": null, "open_files_limit": null,
            "thread_kind": null
        })).unwrap();
        SnapshotData {
            captured_at: std::time::SystemTime::now(),
            general_stats: GeneralStats {
                total_memory: 1000,
                used_memory: 100,
                total_swap: 0,
                used_swap: 0,
            },
            cpu_stats: CpuStats {
                global_usage: 25.0,
                per_cpu_usage: vec![25.0],
            },
            network_stats: NetworkStats::take_default(),
            disk_io_stats: DiskIoStats::take_default(),
            pids: vec![PidV(pid)],
            processes: vec![process],
        }
    }

    #[test]
    fn histories_align_and_pid_reuse_starts_fresh() {
        let mut state = SysinfoSharedState::new();
        state.config.max_readings = 2;
        state.config.limit_processes_to_selection = false;
        state.receive(SysinfoMessage::Snapshot(sample(1, 1)));
        state.receive(SysinfoMessage::Snapshot(sample(2, 1)));
        assert!(!state.process_info.contains_key(&Pid::from_u32(1)));
        let info = &state.process_info[&Pid::from_u32(2)];
        assert_eq!(info.metrics.len(), 2);
        assert_eq!(info.metrics[0].memory, 0);
        assert_eq!(info.metrics[1].memory, 100);
        state.receive(SysinfoMessage::Snapshot(sample(2, 2)));
        let info = &state.process_info[&Pid::from_u32(2)];
        assert_eq!(state.data.len(), 2);
        assert_eq!(info.metrics.len(), 2);
        assert_eq!(info.metrics[0].memory, 0);
        assert_eq!(info.detail.start_time, 2);
        for _ in 0..3 {
            state.receive(SysinfoMessage::Components(ComponentsSnapshot {
                captured_at: std::time::SystemTime::now(),
                component_stats: ComponentStats::take_default(),
            }));
        }
        assert_eq!(state.temperatures.len(), 2);
        assert_eq!(state.data.len(), 2);
        assert_eq!(state.process_info[&Pid::from_u32(2)].metrics.len(), 2);
    }

    #[test]
    fn unselected_processes_keep_only_their_latest_metrics() {
        let mut state = SysinfoSharedState::new();
        state.process_selection.select_process(Pid::from_u32(1));

        for _ in 0..2 {
            let mut snapshot = sample(1, 1);
            let mut second = snapshot.processes[0].clone();
            second.pid = PidV(2);
            snapshot.pids.push(PidV(2));
            snapshot.processes.push(second);
            state.receive(SysinfoMessage::Snapshot(snapshot));
        }

        assert_eq!(state.process_info[&Pid::from_u32(1)].metrics.len(), 2);
        assert_eq!(state.process_info[&Pid::from_u32(2)].metrics.len(), 1);
    }

    #[test]
    fn no_selection_keeps_only_current_process_metrics() {
        let mut state = SysinfoSharedState::new();
        state.receive(SysinfoMessage::Snapshot(sample(1, 1)));
        state.receive(SysinfoMessage::Snapshot(sample(1, 1)));

        assert_eq!(state.process_info[&Pid::from_u32(1)].metrics.len(), 1);
    }

    #[test]
    fn disabling_selected_only_retains_history_for_all_processes() {
        let mut state = SysinfoSharedState::new();
        state.config.limit_processes_to_selection = false;
        state.process_selection.select_process(Pid::from_u32(1));

        for _ in 0..2 {
            let mut snapshot = sample(1, 1);
            let mut second = snapshot.processes[0].clone();
            second.pid = PidV(2);
            snapshot.pids.push(PidV(2));
            snapshot.processes.push(second);
            state.receive(SysinfoMessage::Snapshot(snapshot));
        }

        assert_eq!(state.process_info[&Pid::from_u32(1)].metrics.len(), 2);
        assert_eq!(state.process_info[&Pid::from_u32(2)].metrics.len(), 2);
    }

    #[test]
    fn local_connection_updates_frontend_data_and_acknowledges_intervals() {
        use crate::gui::{
            BackendId,
            backend::{Connection, FrontendGroup, init_frontend_groups},
        };
        let cx = egui::Context::default();
        let groups = init_frontend_groups(&cx);
        let state = match &groups[&BackendId("sysinfo".into())] {
            FrontendGroup::Sysinfo(view) => view.state.clone(),
            _ => unreachable!(),
        };
        {
            let mut data = state.lock();
            data.config.update_interval = Duration::from_millis(100);
            data.config.temperature_interval = Duration::from_millis(50);
        }
        let connection = Connection::new(cx, &groups);
        let deadline = std::time::Instant::now() + Duration::from_secs(5);
        loop {
            let data = state.lock();
            if !data.data.is_empty() && !data.temperatures.is_empty() {
                assert_eq!(
                    data.applied_temperature_interval,
                    Some(Duration::from_millis(50))
                );
                break;
            }
            drop(data);
            assert!(
                std::time::Instant::now() < deadline,
                "frontend did not receive samples"
            );
            std::thread::sleep(Duration::from_millis(10));
        }
        drop(connection);
    }
}
