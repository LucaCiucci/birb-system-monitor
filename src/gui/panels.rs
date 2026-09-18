use serde::{Deserialize, Serialize};
use strum_macros::{Display, EnumIter, EnumString};

/// Panel kind; Tab pairs this with a UUID for independently configured instances.
/// Saved directly as a kebab-case name.
#[derive(
    Debug,
    Clone,
    Copy,
    PartialEq,
    Eq,
    PartialOrd,
    Ord,
    Hash,
    Serialize,
    Deserialize,
    EnumString,
    Display,
    EnumIter,
)]
#[serde(rename_all = "kebab-case")]
#[strum(serialize_all = "kebab-case")]
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
