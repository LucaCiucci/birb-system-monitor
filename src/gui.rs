use std::{fmt::Display, str::FromStr};

pub mod backend;

use ecow::EcoString;
use egui::{Ui, WidgetText};
use serde::{Deserialize, Serialize};
pub mod app;
pub mod save;
pub mod tabs;
pub mod widgets;

/// Panel kind; Tab pairs this with a UUID for independently configured instances.
/// Serialize using the previous layout representation to preserve saved profiles.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(try_from = "SavedPanelId", into = "SavedPanelId")]
pub enum PanelId {
    Cpu,
    Memory,
    Processes,
    SelectedProcess,
    Network,
    DiskIo,
    Dashboard,
    Settings,
    Temperature,
    TemperatureChart,
    Containers,
    Images,
}

#[derive(Serialize, Deserialize)]
struct SavedPanelId {
    backend: BackendId,
    panel: BackendPanelId,
}

impl PanelId {
    pub fn backend(&self) -> BackendId {
        BackendId(
            match self {
                Self::Containers | Self::Images => "docker",
                _ => "sysinfo",
            }
            .into(),
        )
    }
    pub fn panel(&self) -> BackendPanelId {
        BackendPanelId(
            match self {
                Self::Cpu => "cpu",
                Self::Memory => "memory",
                Self::Processes => "processes",
                Self::SelectedProcess => "selected-process",
                Self::Network => "network",
                Self::DiskIo => "disk-io",
                Self::Dashboard => "dashboard",
                Self::Settings => "settings",
                Self::Temperature => "temperature",
                Self::TemperatureChart => "temperature-chart",
                Self::Containers => "containers",
                Self::Images => "images",
            }
            .into(),
        )
    }
    pub fn new(backend: BackendId, panel: BackendPanelId) -> Self {
        format!("{backend}/{panel}")
            .parse()
            .expect("unknown built-in panel")
    }
}

impl Display for PanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}/{}", self.backend(), self.panel())
    }
}
impl FromStr for PanelId {
    type Err = String;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        Ok(match s {
            "sysinfo/cpu" => Self::Cpu,
            "sysinfo/memory" => Self::Memory,
            "sysinfo/processes" => Self::Processes,
            "sysinfo/selected-process" => Self::SelectedProcess,
            "sysinfo/network" => Self::Network,
            "sysinfo/disk-io" => Self::DiskIo,
            "sysinfo/dashboard" => Self::Dashboard,
            "sysinfo/settings" => Self::Settings,
            "sysinfo/temperature" => Self::Temperature,
            "sysinfo/temperature-chart" => Self::TemperatureChart,
            "docker/containers" => Self::Containers,
            "docker/images" => Self::Images,
            _ => return Err(format!("Unknown panel: {s}")),
        })
    }
}
impl TryFrom<SavedPanelId> for PanelId {
    type Error = String;
    fn try_from(id: SavedPanelId) -> Result<Self, Self::Error> {
        format!("{}/{}", id.backend, id.panel).parse()
    }
}
impl From<PanelId> for SavedPanelId {
    fn from(id: PanelId) -> Self {
        Self {
            backend: id.backend(),
            panel: id.panel(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendId(pub EcoString);

impl Display for BackendId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for BackendId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BackendId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(BackendId(s.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub struct BackendPanelId(pub EcoString);

impl Display for BackendPanelId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

impl Serialize for BackendPanelId {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.0.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for BackendPanelId {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let s = String::deserialize(deserializer)?;
        Ok(BackendPanelId(s.into()))
    }
}

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub struct BackendPanelInfo {
    pub id: BackendPanelId,
    pub title: EcoString,
    pub description: EcoString,
}

pub trait BackendPanel {
    fn title(&mut self) -> WidgetText;
    fn ui(&mut self, ui: &mut Ui);
    fn scroll_bars(&self) -> [bool; 2] {
        [false, true]
    }
    fn save_config(&self) -> anyhow::Result<serde_json::Value> {
        Ok(serde_json::Value::Null)
    }
    fn load_config(&mut self, _config: &serde_json::Value) -> anyhow::Result<()> {
        Ok(())
    }
}
