use serde::{Deserialize, Serialize};

use crate::backend::docker::{DockerCommand, DockerMessage};
use crate::backend::sysinfo::{SysinfoCommand, SysinfoMessage};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    Docker(DockerCommand),
    Sysinfo(SysinfoCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Docker(DockerMessage),
    Sysinfo(SysinfoMessage),
}
