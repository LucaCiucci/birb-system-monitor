use serde::{Deserialize, Serialize};
use std::time::Duration;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum SystemId {
    System,
    Components,
    Docker,
}

use crate::backend::docker::{DockerCommand, DockerMessage};
use crate::backend::sysinfo::{SysinfoCommand, SysinfoMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    SetInterval {
        target: SystemId,
        interval: Duration,
    },
    Refresh(SystemId),
    Docker(DockerCommand),
    Sysinfo(SysinfoCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    IntervalChanged {
        target: SystemId,
        interval: Duration,
    },
    CommandError(String),
    Docker(DockerMessage),
    Sysinfo(SysinfoMessage),
}
