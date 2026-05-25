use std::{collections::{HashMap, VecDeque}, sync::Arc, thread::JoinHandle, time::{Duration, Instant}};

use egui::{ProgressBar, WidgetText, mutex::Mutex};
use sysinfo::{DiskUsage, Gid, Pid, ProcessesToUpdate, System, Uid};
use ustr::Ustr;

use crate::{Backend, BackendPanel, BackendPanelId, BackendPanelInfo, backend::sysinfo::proc_list::ProcessesPanel};

mod proc_list;

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
        Self { state, updater: Some(updater) }
    }
}

impl Backend for SysinfoBackend {
    fn name(&self) -> WidgetText {
        "Sysinfo".into()
    }

    fn panels(&self) -> Vec<BackendPanelInfo> {
        vec![BackendPanelInfo {
            id: BackendPanelId("memory".into()),
            title: "Memory".into(),
            description: "Shows memory usage".into(),
        }, BackendPanelInfo {
            id: BackendPanelId("processes".into()),
            title: "Processes".into(),
            description: "Shows process information".into(),
        }]
    }

    fn new_panel(&self, panel_id: &BackendPanelId) -> Box<dyn BackendPanel> {
        match panel_id.0.as_str() {
            "memory" => Box::new(MemoryPanel::new(self.state.clone())),
            "processes" => Box::new(ProcessesPanel::new(self.state.clone())),
            _ => panic!("Unknown panel id: {}", panel_id.0),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct SysinfoConfig {
    update_interval: Duration,
}

impl Default for SysinfoConfig {
    fn default() -> Self {
        Self { update_interval: Duration::from_secs(1) }
    }
}

pub struct MemoryPanel {
    state: Arc<Mutex<SysinfoSharedState>>,
}

#[derive(Debug, Clone, PartialEq)]
struct SysinfoSharedState {
    config: SysinfoConfig,
    data: Vec<SnapshotData>,
    cx: egui::Context,
    should_stop: bool,
}

impl SysinfoSharedState {
    fn new(cx: egui::Context) -> Self {
        Self {
            config: Default::default(),
            data: Vec::new(),
            cx,
            should_stop: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
struct SnapshotData {
    general_stats: GeneralStats,
    processes: HashMap<Pid, ProcessSnapshot>,
}

impl SnapshotData {
    fn take(sys: &mut System) -> Self {
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self {
            general_stats: GeneralStats::take(sys),
            processes: sys.processes()
                .iter()
                .map(|(pid, process)| (*pid, ProcessSnapshot::from_sysinfo(process)))
                .collect(),
        }
    }
}

pub struct Snapshot<T> {
    pub time: Instant,
    pub data: T,
}

impl<T> Snapshot<T> {
    pub fn new(data: T) -> Self {
        Self { time: Instant::now(), data }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd)]
pub struct GeneralStats {
    total_memory: u64,
    used_memory: u64,
    total_swap: u64,
    used_swap: u64,
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

#[derive(Debug, Clone, PartialEq)]
struct ProcessSnapshot {
    pid: Pid,
    name: Ustr,
    cmd: VecDeque<Ustr>,
    cwd: Option<Ustr>,
    accumulated_cpu_time: Duration,
    cpu_usage: f32,
    memory: u64,
    virtual_memory: u64,
    du: DiskUsage,
    effective_group_id: Option<Gid>,
    effective_user_id: Option<Uid>,
    thread_kind: Option<sysinfo::ThreadKind>,
}

impl ProcessSnapshot {
    pub fn from_sysinfo(process: &sysinfo::Process) -> Self {
        Self {
            pid: process.pid(),
            name: process.name().to_string_lossy().into(),
            cmd: process.cmd().iter().map(|s| s.to_string_lossy().into()).collect(),
            cwd: process.cwd().map(|s| s.to_string_lossy().into()),
            accumulated_cpu_time: Duration::from_millis(process.accumulated_cpu_time()),
            cpu_usage: process.cpu_usage(),
            memory: process.memory(),
            virtual_memory: process.virtual_memory(),
            du: process.disk_usage(),
            effective_group_id: process.effective_group_id(),
            effective_user_id: process.effective_user_id().cloned(),
            thread_kind: process.thread_kind(),
        }
    }
}

fn worker_thread(state: Arc<Mutex<SysinfoSharedState>>) {
    let mut sys = System::new_all();

    loop {
        let (cx, update_interval) = {
            let data = state.lock();
            if data.should_stop {
                return;
            }
            (data.cx.clone(), data.config.update_interval)
        };

        state.lock().data.push(SnapshotData::take(&mut sys));
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

impl MemoryPanel {
    fn new(state: Arc<Mutex<SysinfoSharedState>>) -> Self {
        Self { state }
    }
}

impl BackendPanel for MemoryPanel {
    fn title(&mut self) -> WidgetText {
        "Memory".into()
    }

    fn ui(&mut self, ui: &mut egui::Ui) {
        let data = self.state.lock();

        if let Some(data) = data.data.last() {
            ui.label(format!("Total memory: {} MB", data.general_stats.total_memory / 1024 / 1024));
            ui.label(format!("Used memory: {} MB", data.general_stats.used_memory / 1024 / 1024));
            ui.label(format!("Total swap: {} MB", data.general_stats.total_swap / 1024 / 1024));
            ui.label(format!("Used swap: {} MB", data.general_stats.used_swap / 1024 / 1024));
            let pb = ProgressBar::new(data.general_stats.used_memory as f32 / data.general_stats.total_memory as f32)
                .text(format!("{:.1}%", (data.general_stats.used_memory as f64 / data.general_stats.total_memory as f64) * 100.0));
            ui.add(pb);
            let pb = ProgressBar::new(data.general_stats.used_swap as f32 / data.general_stats.total_swap as f32)
                .text(format!("{:.1}%", (data.general_stats.used_swap as f64 / data.general_stats.total_swap as f64) * 100.0));
            ui.add(pb);
        } else {
            ui.label("Loading...");
        }

        ui.collapsing("Settings", |ui| {
            let mut config = data.config.clone();
            ui.horizontal(|ui| {
                ui.label("Update interval:");
                let mut interval_secs = config.update_interval.as_secs_f32();
                if ui.add(egui::Slider::new(&mut interval_secs, 0.05..=5.0).logarithmic(true)).changed() {
                    config.update_interval = Duration::from_secs_f32(interval_secs);
                }
            });
            if config != data.config {
                drop(data);
                self.state.lock().config = config;
            }
        });
    }
}
