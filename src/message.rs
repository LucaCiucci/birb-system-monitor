use serde::{Deserialize, Serialize};

use crate::backend::sysinfo::{SysinfoCommand, SysinfoMessage};


#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Command {
    Sysinfo(SysinfoCommand),
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum Message {
    Sysinfo(SysinfoMessage),
}

