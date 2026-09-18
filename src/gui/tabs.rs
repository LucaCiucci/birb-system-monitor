use egui_dock::{DockState, NodeIndex};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use super::panels::PanelId;

#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tab {
    Panel(PanelId, Uuid),
    Other(String),
}

impl Tab {
    pub fn new_panel(id: PanelId) -> Self {
        Tab::Panel(id, Uuid::new_v4())
    }
}

pub fn default_dock_state() -> DockState<Tab> {
    let panel = Tab::new_panel;

    let mut dock_state =
        DockState::new(vec![panel(PanelId::Processes), panel(PanelId::Temperature)]);

    let surface = dock_state.main_surface_mut();

    let [left, right] = surface.split_right(NodeIndex::root(), 0.30, vec![panel(PanelId::Cpu)]);

    surface.split_below(
        left,
        0.80,
        vec![panel(PanelId::Containers), panel(PanelId::Images)],
    );

    let [top, bottom] = surface.split_below(
        right,
        0.35,
        vec![panel(PanelId::SelectedProcess), panel(PanelId::Settings)],
    );

    let [_cpu, _mem] = surface.split_right(top, 0.50, vec![panel(PanelId::Memory)]);

    let [_bottom_left, net] = surface.split_right(bottom, 0.40, vec![panel(PanelId::Network)]);

    let [net, _disk] = surface.split_below(
        net,
        0.50,
        vec![
            panel(PanelId::TemperatureChart),
            panel(PanelId::Temperature),
        ],
    );

    let [_net, _temps] = surface.split_right(net, 0.50, vec![panel(PanelId::DiskIo)]);

    dock_state
}
