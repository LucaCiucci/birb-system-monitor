use std::{
    collections::{HashMap, HashSet, VecDeque},
    sync::Arc,
    thread::JoinHandle,
    time::{Duration, Instant},
};

use egui::{mutex::Mutex, WidgetText};
use serde::{Deserialize, Serialize};
use sysinfo::{DiskUsage, Disks, Gid, Networks, Pid, ProcessesToUpdate, System, Uid};
use ustr::Ustr;

use crate::{
    backend::sysinfo::{
        cpu::CpuPanel, dashboard::DashboardPanel, disk_io::DiskIoPanel,
        memory::MemoryPanel, network::NetworkPanel, proc_list::ProcessesPanel,
        selected_process::SelectedProcessPanel, settings::SettingsPanel,
        temperature::TemperaturePanel, temperature_chart::TemperatureChartPanel,
    },
    Backend, BackendPanel, BackendPanelId, BackendPanelInfo,
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

pub struct SysinfoBackend {
    state: Arc<Mutex<SysinfoSharedState>>,
    updater: Option<JoinHandle<()>>,
}

impl Drop for SysinfoBackend {
    fn drop(&mut self) {
        let mut data = self.state.lock();
        data.should_stop = true;
        drop(data);
        if let Some(updater) = self.updater.take() {
            updater.join().expect("Failed to join updater thread");
        }
    }
}

impl SysinfoBackend {
    pub fn new(cx: egui::Context) -> Self {
        let state = SysinfoSharedState::new(cx);
        let state = Arc::new(Mutex::new(state));
        let updater = {
            let state = Arc::clone(&state);
            std::thread::spawn(move || worker_thread(state))
        };
        Self {
            state,
            updater: Some(updater),
        }
    }
}

impl Backend for SysinfoBackend {
    fn name(&self) -> WidgetText {
        "Sysinfo".into()
    }

    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::to_value(&self.state.lock().config)?)
    }

    fn load_config(&mut self, config: &serde_json::Value) -> anyhow::Result<()> {
        self.state.lock().config = serde_json::from_value(config.clone())?;
        Ok(())
    }

    fn panels(&self) -> Vec<BackendPanelInfo> {
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

    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
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
    update_interval: Duration,
    /// Number of historical readings to keep and display on plots.
    /// 0 = keep up to 600 (full range).
    pub max_readings: usize,
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
        if self.max_readings == 0 { 600 } else { self.max_readings }
    }
}

impl Default for SysinfoConfig {
    fn default() -> Self {
        Self {
            update_interval: Duration::from_secs(1),
            max_readings: 60,
        }
    }
}

pub(super) struct SysinfoSharedState {
    config: SysinfoConfig,
    process_selection: ProcessSelection,
    process_info: HashMap<Pid, ProcessInfo>,
    data: Vec<SnapshotData>,
    cx: egui::Context,
    should_stop: bool,
}

impl SysinfoSharedState {
    fn new(cx: egui::Context) -> Self {
        Self {
            config: Default::default(),
            process_selection: Default::default(),
            process_info: HashMap::new(),
            data: Vec::new(),
            cx,
            should_stop: false,
        }
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

#[derive(Debug, Clone, PartialEq)]
pub(super) struct SnapshotData {
    pub(super) captured_at: Instant,
    pub(super) general_stats: GeneralStats,
    pub(super) cpu_stats: CpuStats,
    pub(super) network_stats: NetworkStats,
    pub(super) disk_io_stats: DiskIoStats,
    pub(super) component_stats: ComponentStats,
    pub(super) pids: Vec<Pid>,
}

impl SnapshotData {
    fn take(sys: &mut System) -> Self {
        sys.refresh_cpu_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self {
            captured_at: Instant::now(),
            general_stats: GeneralStats::take(sys),
            cpu_stats: CpuStats::take(sys),
            network_stats: NetworkStats::take_default(),
            disk_io_stats: DiskIoStats::take_default(),
            component_stats: ComponentStats::take_default(),
            pids: sys.processes().keys().copied().collect(),
        }
    }
}

pub struct Snapshot<T> {
    pub time: Instant,
    pub data: T,
}

impl<T> Snapshot<T> {
    pub fn new(data: T) -> Self {
        Self {
            time: Instant::now(),
            data,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct GeneralStats {
    pub(super) total_memory: u64,
    pub(super) used_memory: u64,
    pub(super) total_swap: u64,
    pub(super) used_swap: u64,
}

impl GeneralStats {
    fn take(sys: &mut System) -> Self {
        sys.refresh_memory();
        Self {
            total_memory: sys.total_memory(),
            used_memory: sys.used_memory(),
            total_swap: sys.total_swap(),
            used_swap: sys.used_swap(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(super) struct ComponentStats {
    pub(super) components: Vec<ComponentSnapshot>,
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(super) struct ComponentSnapshot {
    pub(super) label: String,
    pub(super) temperature: Option<f32>,
    pub(super) max: Option<f32>,
    pub(super) critical: Option<f32>,
}

impl ComponentStats {
    fn take(components: &sysinfo::Components) -> Self {
        Self {
            components: components
                .iter()
                .map(|c| ComponentSnapshot {
                    label: c.label().to_string(),
                    temperature: c.temperature(),
                    max: c.max(),
                    critical: c.critical(),
                })
                .collect(),
        }
    }

    fn take_default() -> Self {
        Self {
            components: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(super) struct CpuStats {
    pub(super) global_usage: f32,
    pub(super) per_cpu_usage: Vec<f32>,
}

impl CpuStats {
    fn take(sys: &System) -> Self {
        Self {
            global_usage: sys.global_cpu_usage(),
            per_cpu_usage: sys.cpus().iter().map(|cpu| cpu.cpu_usage()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(super) struct NetworkStats {
    pub(super) total_received: u64,
    pub(super) total_transmitted: u64,
}

impl NetworkStats {
    fn take(networks: &Networks) -> Self {
        let mut total_received = 0u64;
        let mut total_transmitted = 0u64;
        for (_name, data) in networks.iter() {
            total_received = total_received.saturating_add(data.total_received());
            total_transmitted = total_transmitted.saturating_add(data.total_transmitted());
        }
        Self {
            total_received,
            total_transmitted,
        }
    }

    fn take_default() -> Self {
        Self {
            total_received: 0,
            total_transmitted: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub(super) struct DiskIoStats {
    pub(super) total_read_bytes: u64,
    pub(super) total_written_bytes: u64,
}

impl DiskIoStats {
    fn take(disks: &Disks) -> Self {
        let mut total_read_bytes = 0u64;
        let mut total_written_bytes = 0u64;
        for disk in disks.iter() {
            let usage = disk.usage();
            total_read_bytes = total_read_bytes.saturating_add(usage.total_read_bytes);
            total_written_bytes = total_written_bytes.saturating_add(usage.total_written_bytes);
        }
        Self {
            total_read_bytes,
            total_written_bytes,
        }
    }

    fn take_default() -> Self {
        Self {
            total_read_bytes: 0,
            total_written_bytes: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub(super) struct ProcessMetrics {
    pub(super) cpu_usage: f32,
    pub(super) memory: u64,
    pub(super) virtual_memory: u64,
}

impl ProcessMetrics {
    fn from_process(process: &sysinfo::Process) -> Self {
        Self {
            cpu_usage: process.cpu_usage(),
            memory: process.memory(),
            virtual_memory: process.virtual_memory(),
        }
    }
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
    pub(super) name: Ustr,
    pub(super) cmd: VecDeque<Ustr>,
    pub(super) cwd: Option<Ustr>,
    pub(super) accumulated_cpu_time: Duration,
    pub(super) du: DiskUsage,
    pub(super) effective_group_id: Option<Gid>,
    pub(super) effective_user_id: Option<Uid>,
    pub(super) thread_kind: Option<sysinfo::ThreadKind>,
}

impl ProcessDetail {
    fn from_process(process: &sysinfo::Process) -> Self {
        Self {
            pid: process.pid(),
            name: process.name().to_string_lossy().into(),
            cmd: process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into())
                .collect(),
            cwd: process.cwd().map(|s| s.to_string_lossy().into()),
            accumulated_cpu_time: Duration::from_millis(process.accumulated_cpu_time()),
            du: process.disk_usage(),
            effective_group_id: process.effective_group_id(),
            effective_user_id: process.effective_user_id().cloned(),
            thread_kind: process.thread_kind(),
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub(super) struct ProcessInfo {
    pub(super) detail: ProcessDetail,
    /// History of metrics, length always matches `data.len()` in shared state.
    /// For processes that existed from the beginning, metrics[i] corresponds to data[i].
    /// For processes that appeared later, earlier entries are zero-filled.
    pub(super) metrics: VecDeque<ProcessMetrics>,
}

fn worker_thread(state: Arc<Mutex<SysinfoSharedState>>) {
    let mut sys = System::new_all();
    let mut networks = Networks::new_with_refreshed_list();
    let mut disks = Disks::new_with_refreshed_list();
    let mut components = sysinfo::Components::new_with_refreshed_list();

    // Refresh interval for components (not all systems support frequent updates)
    let mut last_component_refresh = Instant::now();

    loop {
        let (cx, update_interval) = {
            let data = state.lock();
            if data.should_stop {
                return;
            }
            (data.cx.clone(), data.config.update_interval)
        };

        networks.refresh(true);
        disks.refresh(false);

        // Refresh components less frequently (every ~5s or on first call)
        let refresh_components = last_component_refresh.elapsed() >= Duration::from_secs(5);
        if refresh_components {
            for c in components.iter_mut() {
                c.refresh();
            }
            last_component_refresh = Instant::now();
        }

        // Build the system-wide snapshot (just PIDs + general stats)
        let snapshot = SnapshotData::take(&mut sys);

        let mut data = state.lock();

        // Update process info before pushing snapshot
        let snapshot_pids: HashSet<Pid> = snapshot.pids.iter().copied().collect();

        let existing_snapshot_count = data.data.len();
        for pid in &snapshot.pids {
            if let Some(process) = sys.processes().get(pid) {
                let metrics = ProcessMetrics::from_process(process);
                let detail = ProcessDetail::from_process(process);

                use std::collections::hash_map::Entry;
                match data.process_info.entry(*pid) {
                    Entry::Occupied(mut e) => {
                        let info = e.get_mut();
                        info.detail = detail;
                        info.metrics.push_back(metrics);
                    }
                    Entry::Vacant(e) => {
                        let mut metrics_deque = VecDeque::new();
                        // Pad with zeros to match existing snapshot count
                        for _ in 0..existing_snapshot_count {
                            metrics_deque.push_back(ProcessMetrics::default());
                        }
                        metrics_deque.push_back(metrics);
                        e.insert(ProcessInfo {
                            detail,
                            metrics: metrics_deque,
                        });
                    }
                }
            }
        }

        // Remove dead PIDs from process_info
        data.process_info.retain(|pid, _| snapshot_pids.contains(pid));

        // Push snapshot (after process_info so indices align)
        data.data.push(snapshot);

        // Also update network/disk/temp data in the latest snapshot
        if let Some(latest) = data.data.last_mut() {
            latest.network_stats = NetworkStats::take(&networks);
            latest.disk_io_stats = DiskIoStats::take(&disks);
            latest.component_stats = ComponentStats::take(&components);
        }

        // Trim old data
        let max_history = data.config.max_history_readings();
        let excess = data.data.len().saturating_sub(max_history);
        if excess > 0 {
            data.data.drain(..excess);
            for info in data.process_info.values_mut() {
                info.metrics.drain(..excess);
            }
        }
        drop(data);
        cx.request_repaint();

        let mut waited = Duration::from_secs(0);
        while waited < update_interval {
            let sleep_duration = update_interval.min(Duration::from_millis(100));
            std::thread::sleep(sleep_duration);
            waited += sleep_duration;
            if state.lock().should_stop {
                return;
            }
        }
    }
}
