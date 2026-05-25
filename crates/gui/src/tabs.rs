use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::PanelId;



#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum Tab {
    Panel(PanelId, Uuid),
    Other(String),
}

pub fn default_dock_state() -> egui_dock::DockState<Tab> {
    egui_dock::DockState::new(vec![
        Tab::Other("Tab 1".to_string()),
        Tab::Other("Tab 2".to_string()),
        Tab::Other("Tab 3".to_string()),
    ])
}
