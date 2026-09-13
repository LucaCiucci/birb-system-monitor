use std::str::FromStr;

use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::PanelId;



#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tab {
    Panel(PanelId, Uuid),
    Other(String),
}

impl Tab {
    pub fn new_from_path(path: &str) -> Result<Self, String> {
        Ok(Tab::Panel(PanelId::from_str(path)?, Uuid::new_v4()))
    }
}

pub fn default_dock_state() -> DockState<Tab> {
    let panel = |path: &str| -> Tab {
        Tab::new_from_path(path).unwrap()
    };

    let mut dock_state = DockState::new(vec![
        panel("sysinfo/processes"),
        panel("sysinfo/temperature"),
    ]);

    let surface = dock_state.main_surface_mut();

    let [left, right] = surface.split_right(
        NodeIndex::root(),
        0.30,
        vec![panel("sysinfo/cpu")],
    );

    surface.split_below(
        left,
        0.80,
        vec![
            panel("docker/containers"),
            panel("docker/images"),
        ],
    );

    let [top, bottom] = surface.split_below(
        right,
        0.35,
        vec![
            panel("sysinfo/selected-process"),
            panel("sysinfo/settings")],
    );

    let [_cpu, _mem] = surface.split_right(
        top,
        0.50,
        vec![panel("sysinfo/memory")],
    );

    let [_bottom_left, net] = surface.split_right(
        bottom,
        0.40,
        vec![panel("sysinfo/network")],
    );

    let [net, _disk] = surface.split_below(
        net,
        0.50,
        vec![
            panel("sysinfo/temperature-chart"),
            panel("sysinfo/temperature"),
        ],
    );

    let [_net, _temps] = surface.split_right(
        net,
        0.50,
        vec![panel("sysinfo/disk-io")],
    );

    dock_state
}
