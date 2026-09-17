use std::collections::HashMap;

use egui::Context;

use super::{BackendOLD, BackendId};

pub mod docker;
pub mod sysinfo;

pub fn init_all_backends(cx: &Context) -> HashMap<BackendId, Box<dyn BackendOLD>> {
    let mut backends: HashMap<BackendId, Box<dyn BackendOLD>> = HashMap::new();
    backends.insert(
        BackendId("sysinfo".into()),
        Box::new(sysinfo::SysinfoBackend::new(cx.clone())),
    );
    backends.insert(
        BackendId("docker".into()),
        Box::new(docker::DockerBackend::new(cx.clone())),
    );
    backends
}
