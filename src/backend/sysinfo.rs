use std::time::{Duration, SystemTime};

use serde::{Deserialize, Serialize};
use sysinfo::{Components, Disks, Networks, Pid, ProcessRefreshKind, ProcessesToUpdate, System};
use tokio::sync::mpsc;

use crate::{
    message::Message,
    utils::{TimedTaskEvent, TimedTaskHandler},
};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SysinfoCommand {
    Refresh,
}

/// Owns the live collectors; emits complete samples without retaining history.
/// Construct with an output channel and pass to `TimedTask::with_handler`.
pub struct SystemHandler {
    collector: Option<SystemCollector>,
    tx: mpsc::Sender<Message>,
}

impl SystemHandler {
    pub fn new(tx: mpsc::Sender<Message>) -> Self {
        Self {
            collector: None,
            tx,
        }
    }
}

impl TimedTaskHandler<SysinfoCommand> for SystemHandler {
    async fn handle(&mut self, event: TimedTaskEvent<SysinfoCommand>) {
        match event {
            TimedTaskEvent::Tick | TimedTaskEvent::Message(SysinfoCommand::Refresh) => {}
        }
        if self.tx.is_closed() {
            return;
        }
        let collector = self.collector.take();
        let (collector, snapshot) = tokio::task::spawn_blocking(move || {
            let mut collector = collector.unwrap_or_else(SystemCollector::new);
            let snapshot = collector.sample();
            (collector, snapshot)
        })
        .await
        .expect("sysinfo sampling failed");
        self.collector = Some(collector);
        // Bounded output applies backpressure. The owner must keep draining the
        // receiver (or close it) when waiting for graceful task shutdown.
        if self
            .tx
            .send(Message::Sysinfo(SysinfoMessage::Snapshot(snapshot)))
            .await
            .is_err()
        {
            tracing::debug!("sysinfo output receiver closed");
        }
    }
}

/// Temperature collector for a separate timed task (typically every five seconds).
/// Shares the output channel with `SystemHandler`; retains no history.
pub struct ComponentsHandler {
    components: Option<Components>,
    tx: mpsc::Sender<Message>,
}

impl ComponentsHandler {
    pub fn new(tx: mpsc::Sender<Message>) -> Self {
        Self {
            components: None,
            tx,
        }
    }
}

impl TimedTaskHandler<SysinfoCommand> for ComponentsHandler {
    async fn handle(&mut self, event: TimedTaskEvent<SysinfoCommand>) {
        match event {
            TimedTaskEvent::Tick | TimedTaskEvent::Message(SysinfoCommand::Refresh) => {}
        }
        if self.tx.is_closed() {
            return;
        }
        let components = self.components.take();
        let (components, snapshot) = tokio::task::spawn_blocking(move || {
            let mut components = components.unwrap_or_default();
            components.refresh(true);
            let snapshot = ComponentsSnapshot {
                captured_at: SystemTime::now(),
                component_stats: ComponentStats::take(&components),
            };
            (components, snapshot)
        })
        .await
        .expect("component sampling failed");
        self.components = Some(components);
        // As with system samples, output uses bounded-channel backpressure.
        if self
            .tx
            .send(Message::Sysinfo(SysinfoMessage::Components(snapshot)))
            .await
            .is_err()
        {
            tracing::debug!("component output receiver closed");
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ComponentsSnapshot {
    pub captured_at: SystemTime,
    pub component_stats: ComponentStats,
}

struct SystemCollector {
    sys: System,
    networks: Networks,
    disks: Disks,
}

impl SystemCollector {
    fn new() -> Self {
        Self {
            sys: System::new_all(),
            networks: Networks::new_with_refreshed_list(),
            disks: Disks::new_with_refreshed_list(),
        }
    }

    fn sample(&mut self) -> SnapshotData {
        self.networks.refresh(true);
        self.disks.refresh(true);
        SnapshotData::take(&mut self.sys, &self.networks, &self.disks)
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SysinfoMessage {
    Snapshot(SnapshotData),
    Components(ComponentsSnapshot),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotData {
    pub captured_at: SystemTime,
    pub general_stats: GeneralStats,
    pub cpu_stats: CpuStats,
    pub network_stats: NetworkStats,
    pub disk_io_stats: DiskIoStats,
    pub pids: Vec<PidV>,
    /// Current processes only.
    ///
    /// The frontend owns history and removal handling.
    pub processes: Vec<ProcessSnapshot>,
}

impl SnapshotData {
    pub fn take(sys: &mut System, networks: &Networks, disks: &Disks) -> Self {
        sys.refresh_cpu_all();
        // Include details for processes created after collector initialization.
        sys.refresh_processes_specifics(
            ProcessesToUpdate::All,
            true,
            ProcessRefreshKind::everything(),
        );
        Self {
            captured_at: SystemTime::now(),
            general_stats: GeneralStats::take(sys),
            cpu_stats: CpuStats::take(sys),
            network_stats: NetworkStats::take(networks),
            disk_io_stats: DiskIoStats::take(disks),
            pids: sys.processes().keys().copied().map(PidV::from).collect(),
            processes: sys
                .processes()
                .values()
                .map(ProcessSnapshot::take)
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PidV(pub u32);

/// Serializable process data for one reading, independent of frontend types.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct ProcessSnapshot {
    pub pid: PidV,
    pub parent: Option<PidV>,
    pub name: String,
    pub cmd: Vec<String>,
    pub exe: Option<String>,
    pub environ: Vec<String>,
    pub cwd: Option<String>,
    pub root: Option<String>,
    pub cpu_usage: f32,
    pub memory: u64,
    pub virtual_memory: u64,
    pub accumulated_cpu_time: Duration,
    pub disk_usage: ProcessDiskUsage,
    pub status: String,
    // Strings also accommodate non-Unix identities, such as Windows SIDs.
    pub user_id: Option<String>,
    pub effective_user_id: Option<String>,
    pub group_id: Option<String>,
    pub effective_group_id: Option<String>,
    pub start_time: u64,
    pub run_time: u64,
    pub session_id: Option<PidV>,
    pub open_files: Option<usize>,
    pub open_files_limit: Option<usize>,
    pub thread_kind: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProcessDiskUsage {
    pub total_read_bytes: u64,
    pub read_bytes: u64,
    pub total_written_bytes: u64,
    pub written_bytes: u64,
}

impl ProcessSnapshot {
    fn take(process: &sysinfo::Process) -> Self {
        let usage = process.disk_usage();
        Self {
            pid: process.pid().into(),
            parent: process.parent().map(Into::into),
            name: process.name().to_string_lossy().into_owned(),
            cmd: process
                .cmd()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            exe: process.exe().map(|p| p.to_string_lossy().into_owned()),
            environ: process
                .environ()
                .iter()
                .map(|s| s.to_string_lossy().into_owned())
                .collect(),
            cwd: process.cwd().map(|p| p.to_string_lossy().into_owned()),
            root: process.root().map(|p| p.to_string_lossy().into_owned()),
            cpu_usage: process.cpu_usage(),
            memory: process.memory(),
            virtual_memory: process.virtual_memory(),
            accumulated_cpu_time: Duration::from_millis(process.accumulated_cpu_time()),
            disk_usage: ProcessDiskUsage {
                total_read_bytes: usage.total_read_bytes,
                read_bytes: usage.read_bytes,
                total_written_bytes: usage.total_written_bytes,
                written_bytes: usage.written_bytes,
            },
            status: process.status().to_string(),
            user_id: process.user_id().map(|id| (**id).to_string()),
            effective_user_id: process.effective_user_id().map(|id| (**id).to_string()),
            group_id: process.group_id().map(|id| id.to_string()),
            effective_group_id: process.effective_group_id().map(|id| id.to_string()),
            start_time: process.start_time(),
            run_time: process.run_time(),
            session_id: process.session_id().map(Into::into),
            open_files: process.open_files(),
            open_files_limit: process.open_files_limit(),
            thread_kind: process.thread_kind().map(|kind| format!("{kind:?}")),
        }
    }
}

impl PidV {
    pub fn to_pid(&self) -> Pid {
        Pid::from_u32(self.0)
    }
}

impl From<Pid> for PidV {
    fn from(pid: Pid) -> Self {
        Self(pid.as_u32())
    }
}

impl From<PidV> for Pid {
    fn from(pidv: PidV) -> Self {
        Self::from_u32(pidv.0)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct GeneralStats {
    pub total_memory: u64,
    pub used_memory: u64,
    pub total_swap: u64,
    pub used_swap: u64,
}

impl GeneralStats {
    pub fn take(sys: &mut System) -> Self {
        sys.refresh_memory();
        Self {
            total_memory: sys.total_memory(),
            used_memory: sys.used_memory(),
            total_swap: sys.total_swap(),
            used_swap: sys.used_swap(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct CpuStats {
    pub global_usage: f32,
    pub per_cpu_usage: Vec<f32>,
}

impl CpuStats {
    pub fn take(sys: &System) -> Self {
        Self {
            global_usage: sys.global_cpu_usage(),
            per_cpu_usage: sys.cpus().iter().map(|cpu| cpu.cpu_usage()).collect(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct NetworkStats {
    pub total_received: u64,
    pub total_transmitted: u64,
}

impl NetworkStats {
    pub fn take(networks: &Networks) -> Self {
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

    pub fn take_default() -> Self {
        Self {
            total_received: 0,
            total_transmitted: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct DiskIoStats {
    pub total_read_bytes: u64,
    pub total_written_bytes: u64,
}

impl DiskIoStats {
    pub fn take(disks: &Disks) -> Self {
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

    pub fn take_default() -> Self {
        Self {
            total_read_bytes: 0,
            total_written_bytes: 0,
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ComponentStats {
    pub components: Vec<ComponentSnapshot>,
}

impl ComponentStats {
    pub fn take(components: &sysinfo::Components) -> Self {
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

    pub fn take_default() -> Self {
        Self {
            components: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, PartialOrd, Serialize, Deserialize)]
pub struct ComponentSnapshot {
    pub label: String,
    pub temperature: Option<f32>,
    pub max: Option<f32>,
    pub critical: Option<f32>,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::utils::TimedTask;

    #[test]
    fn components_tick_independently_on_shared_output() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::channel(4);
            let system = TimedTask::with_handler(
                &rt,
                Duration::from_secs(3600),
                SystemHandler::new(tx.clone()),
            );
            let components =
                TimedTask::with_handler(&rt, Duration::from_millis(10), ComponentsHandler::new(tx));
            let message = tokio::time::timeout(Duration::from_secs(30), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let encoded = serde_json::to_vec(&message).unwrap();
            let decoded: Message = serde_json::from_slice(&encoded).unwrap();
            assert!(matches!(
                decoded,
                Message::Sysinfo(SysinfoMessage::Components(_))
            ));
            components.shutdown().await.unwrap();
            while rx.try_recv().is_ok() {}
            system.send_async(SysinfoCommand::Refresh).await.unwrap();
            let message = tokio::time::timeout(Duration::from_secs(30), rx.recv())
                .await
                .unwrap()
                .unwrap();
            assert!(matches!(
                message,
                Message::Sysinfo(SysinfoMessage::Snapshot(_))
            ));
            let value = serde_json::to_value(&message).unwrap();
            assert!(
                value["Sysinfo"]["Snapshot"]
                    .get("component_stats")
                    .is_none()
            );
            drop(rx);
            system.shutdown().await.unwrap();
        });
    }

    #[test]
    fn timed_handler_emits_serializable_process_samples() {
        let rt = tokio::runtime::Runtime::new().unwrap();
        rt.block_on(async {
            let (tx, mut rx) = mpsc::channel(2);
            let task =
                TimedTask::with_handler(&rt, Duration::from_secs(3600), SystemHandler::new(tx));
            // A command uses the same sampling path as a periodic tick.
            task.send_async(SysinfoCommand::Refresh).await.unwrap();
            let message = tokio::time::timeout(Duration::from_secs(30), rx.recv())
                .await
                .unwrap()
                .unwrap();
            let encoded = serde_json::to_vec(&message).unwrap();
            let decoded: Message = serde_json::from_slice(&encoded).unwrap();
            let Message::Sysinfo(SysinfoMessage::Snapshot(snapshot)) = decoded else {
                panic!("expected a system snapshot");
            };
            assert_eq!(snapshot.pids.len(), snapshot.processes.len());
            assert!(
                snapshot
                    .processes
                    .iter()
                    .all(|p| snapshot.pids.contains(&p.pid))
            );
            if sysinfo::IS_SUPPORTED_SYSTEM {
                assert!(snapshot.pids.contains(&PidV(std::process::id())));
            }
            // Closing the output also lets pending sends finish during shutdown.
            drop(rx);
            task.shutdown().await.unwrap();
        });
    }

    #[tokio::test]
    async fn closed_output_does_not_start_collecting() {
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut handler = SystemHandler::new(tx);
        handler.handle(TimedTaskEvent::Tick).await;
        assert!(handler.collector.is_none());
        let (tx, rx) = mpsc::channel(1);
        drop(rx);
        let mut handler = ComponentsHandler::new(tx);
        handler.handle(TimedTaskEvent::Tick).await;
        assert!(handler.components.is_none());
    }
}
