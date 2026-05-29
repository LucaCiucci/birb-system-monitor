use std::collections::HashMap;

use egui::Context;

use crate::{Backend, BackendId};


pub mod debug;
pub mod docker;
pub mod sysinfo;

pub fn init_all_backends(cx: &Context) -> HashMap<BackendId, Box<dyn Backend>> {
    let mut backends: HashMap<BackendId, Box<dyn Backend>> = HashMap::new();
    backends.insert(BackendId("debug".into()), Box::new(debug::DebugBackend));
    backends.insert(BackendId("sysinfo".into()), Box::new(sysinfo::SysinfoBackend::new(cx.clone())));
    backends.insert(BackendId("docker".into()), Box::new(docker::DockerBackend::new(cx.clone())));
    backends
}
