use std::time::SystemTime;

use serde::{Deserialize, Serialize};
use sysinfo::{Disks, Networks, Pid, ProcessesToUpdate, System};

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SysinfoCommand {

}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum SysinfoMessage {
    Snapshot(SnapshotData),
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct SnapshotData {
    pub captured_at: SystemTime,
    pub general_stats: GeneralStats,
    pub cpu_stats: CpuStats,
    pub network_stats: NetworkStats,
    pub disk_io_stats: DiskIoStats,
    pub component_stats: ComponentStats,
    pub pids: Vec<PidV>,
}

impl SnapshotData {
    pub fn take(sys: &mut System) -> Self {
        sys.refresh_cpu_all();
        sys.refresh_processes(ProcessesToUpdate::All, true);
        Self {
            captured_at: SystemTime::now(),
            general_stats: GeneralStats::take(sys),
            cpu_stats: CpuStats::take(sys),
            network_stats: NetworkStats::take_default(),
            disk_io_stats: DiskIoStats::take_default(),
            component_stats: ComponentStats::take_default(),
            pids: sys.processes().keys().copied().map(PidV::from).collect(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct PidV(pub u32);

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
